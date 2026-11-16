#!/usr/bin/env python3
"""Check changed product-document content against the adopted mechanical contract."""

from __future__ import annotations

import argparse
from doc_content_warnings import report_findings, report_classified_findings
from dataclasses import dataclass
from datetime import date
import html
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote

try:
    from product_doc_markdown import (
        parse_markdown_blocks,
        parse_markdown_html,
        parse_markdown_links,
    )
except RuntimeError as exc:
    print(f"product-doc-content: error: {exc}", file=sys.stderr)
    raise SystemExit(2) from exc


PRODUCT_ROOT = Path("doc/product")
PRODUCT_SUFFIXES = (".prd.md", ".design.md")
LIFECYCLE_VALUES = frozenset({"proposed", "draft", "active", "superseded", "retired"})
LIFECYCLE_PLACEHOLDERS = frozenset(
    {
        "",
        "-",
        "—",
        "–",
        "n/a",
        "na",
        "none",
        "tbd",
        "todo",
        "to be determined",
        "later",
        "待定",
        "待补",
        "待填写",
        "未定",
        "无",
    }
)
DESIGN_EXEMPTION_REASON_PLACEHOLDERS = LIFECYCLE_PLACEHOLDERS
HTML_COMMENT_RE = re.compile(r"<!--[\s\S]*?-->")
ID_RE = re.compile(r"\b((?:REQ|AC)-[A-Z0-9][A-Z0-9_*-]*)", re.IGNORECASE)
# Aggregate trace tables use short domain-specific criterion IDs (for example
# PL-6). REQ/AC and professional PRD IDs have separate contracts.
TRACE_CRITERION_ID_RE = re.compile(r"\b[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)*-\d+(?:[A-Z])?\b")
HEADING_PREFIX_RE = re.compile(r"^ {0,3}#{2,6}\s+")
DECLARATION_PREFIX_RE = re.compile(r"^\s*(?:[-+*]\s+|\|\s*)")
TABLE_ID_CELL_RE = re.compile(r"^\s*((?:REQ|AC)-[A-Z0-9][A-Z0-9_*-]*)\s*$", re.IGNORECASE)
ANCHOR_RE = re.compile(r"<a\s+[^>]*\bid\s*=\s*[\"']([^\"']+)[\"'][^>]*>", re.IGNORECASE)
RAW_PATH_RE = re.compile(r"(?:`|\b)(?:doc|\.\.?/)[^`\s,，。；;）)]+(?:\.md)?(?:#[^`\s,，。；;）)]+)?")
HTML_TAG_RE = re.compile(r"<[^>]+>")

# Writing conventions are useful diagnostics, not factual admission criteria.
# References, identities, declared relations and positive claims stay strict;
# future codes fail closed until their contract is explicitly classified here.
ADVISORY_CODES = frozenset({
    "missing-metadata", "invalid-review-date", "missing-player-scenario", "missing-scope",
    "missing-normal-path", "missing-decision", "missing-failure-recovery",
    "missing-acceptance", "missing-evidence-boundary", "missing-unresolved",
    "missing-design-decision", "missing-validation-boundary", "missing-design-or-exemption",
    "active-topic-missing-requirement", "active-topic-missing-acceptance",
    "paired-trace-missing-table", "paired-trace-missing-column", "paired-trace-empty-owner",
    "paired-trace-empty-evidence", "paired-trace-invalid-test-tier",
    "paired-trace-authority-not-link", "paired-trace-missing-relation",
    "root-section-contract", "root-metadata-content", "missing-anchor",
    "req-missing-acceptance", "ac-missing-requirement",
    "design-missing-prd-trace-fragment", "design-prd-mapping-incomplete",
    "missing-paired-prd-link", "lifecycle-missing-receiving-authority",
    "lifecycle-missing-remaining-semantics", "lifecycle-missing-stable-reference",
    "lifecycle-missing-deletion-condition", "lifecycle-placeholder",
    "lifecycle-missing-receiving-authority-link", "lifecycle-missing-stable-reference-link",
})


@dataclass(frozen=True)
class ChangedDocument:
    path: str
    status: str
    old_text: str | None
    new_text: str


def fail(errors: list[str], code: str, path: str, detail: str) -> None:
    errors.append(f"product-doc-content: {code}: {path}: {detail}")


def run_git(root: Path, *args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        check=False,
        capture_output=True,
        text=True,
    )
    if check and result.returncode != 0:
        raise ValueError((result.stderr or result.stdout).strip() or "git command failed")
    return result.stdout


def resolve_commit(root: Path, value: str | None, label: str) -> str:
    if not value:
        raise ValueError(f"missing {label} commit OID")
    if not re.fullmatch(r"[0-9a-fA-F]{40}", value):
        raise ValueError(f"{label} must be a full 40-character commit OID")
    try:
        resolved = run_git(root, "rev-parse", "--verify", f"{value}^{{commit}}").strip()
    except ValueError as exc:
        raise ValueError(f"invalid {label} commit OID {value!r}: {exc}") from exc
    if not re.fullmatch(r"[0-9a-f]{40}", resolved):
        raise ValueError(f"{label} did not resolve to a full commit OID")
    return resolved


def is_product_doc(path: str) -> bool:
    return path.startswith(f"{PRODUCT_ROOT.as_posix()}/") and path.endswith(PRODUCT_SUFFIXES)


def is_product_root(path: str) -> bool:
    if not path.startswith(f"{PRODUCT_ROOT.as_posix()}/") or not path.endswith("/prd.md"):
        return False
    try:
        relative = Path(path).relative_to(PRODUCT_ROOT)
    except ValueError:
        return False
    return len(relative.parts) == 2 and relative.parts[-1] == "prd.md"


def parse_name_status(output: str) -> dict[str, str]:
    changed: dict[str, str] = {}
    for line in output.splitlines():
        if not line.strip():
            continue
        fields = line.split("\t")
        status = fields[0]
        if status.startswith("R") or status.startswith("C"):
            if len(fields) >= 3:
                changed[fields[2]] = status
            continue
        if len(fields) >= 2:
            changed[fields[1]] = status
    return changed


def diff_paths(root: Path, base: str, head: str, worktree: bool, scope: str = "doc/product") -> tuple[dict[str, str], set[str]]:
    source_base = run_git(root, "merge-base", base, head).strip()
    if not source_base:
        raise ValueError(f"base/head have no merge-base: {base} {head}")
    changed = parse_name_status(run_git(root, "diff", "--name-status", "-M", source_base, head, "--", scope))
    if not worktree:
        return changed, set()
    checkout_head = run_git(root, "rev-parse", "--verify", "HEAD^{commit}").strip()
    overlay = parse_name_status(
        run_git(root, "diff", "--name-status", "-M", checkout_head, "--", scope)
    )
    overlay.update(
        parse_name_status(
            run_git(root, "diff", "--cached", "--name-status", "-M", checkout_head, "--", scope)
        )
    )
    for path in run_git(root, "ls-files", "--others", "--exclude-standard", "--", scope).splitlines():
        if path.strip():
            overlay[path.strip()] = "??"
    for path, status in overlay.items():
        changed[path] = status
    return changed, set(overlay)


def git_text(root: Path, commit: str, path: str) -> str | None:
    entry = run_git(root, "ls-tree", "-z", commit, "--", path)
    if not entry:
        return None
    mode = entry.split(" ", 1)[0]
    if mode == "120000":
        raise ValueError(f"symlink-not-allowed: {path}: selected Git document is a symlink")
    if mode not in {"100644", "100755"}:
        raise ValueError(f"selected document is not a regular file: {path}")
    result = subprocess.run(
        ["git", "-C", str(root), "show", f"{commit}:{path}"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise ValueError((result.stderr or result.stdout).strip() or f"cannot read selected document: {path}")
    return result.stdout


def require_safe_worktree_path(root: Path, path: str) -> Path:
    relative = Path(path)
    if relative.is_absolute() or ".." in relative.parts:
        raise ValueError(f"document path escapes repository: {path}")
    target = root
    for part in relative.parts:
        target = target / part
        if target.is_symlink():
            raise ValueError(f"symlink-not-allowed: {path}: source or parent is a symlink")
    return target


def current_text(root: Path, head: str, path: str, use_worktree_content: bool) -> str | None:
    if use_worktree_content:
        target = require_safe_worktree_path(root, path)
        if not target.is_file():
            return None
        return target.read_text(encoding="utf-8")
    return git_text(root, head, path)


def without_html_comments(text: str) -> str:
    def preserve_newlines(match: re.Match[str]) -> str:
        return "".join(character for character in match.group(0) if character in "\r\n")

    return HTML_COMMENT_RE.sub(preserve_newlines, text)


def normalized_for_change(text: str | None) -> str:
    if text is None:
        return ""
    text = without_html_comments(text).replace("\r\n", "\n").replace("\r", "\n")
    return "\n".join(line.rstrip() for line in text.splitlines() if line.strip())


def visible_lines(text: str) -> list[tuple[int, str]]:
    """Return line-numbered prose, excluding comments and code blocks."""
    text = without_html_comments(text)
    excluded_lines = {
        number
        for block in parse_markdown_blocks(text)
        for number in range(block.start_line, block.end_line + 1)
    }
    visible: list[tuple[int, str]] = []
    for number, line in enumerate(text.splitlines(), start=1):
        if number in excluded_lines:
            continue
        visible.append((number, line))
    return visible


def actual_anchor_occurrences(text: str) -> list[tuple[str, int]]:
    """Return ``(anchor_id, line)`` pairs from parser-recognized HTML only."""
    occurrences: list[tuple[str, int]] = []
    for node in parse_markdown_html(without_html_comments(text)):
        for match in ANCHOR_RE.finditer(node.content):
            line = node.line + node.content[: match.start()].count("\n")
            occurrences.append((match.group(1), line))
    return occurrences


def strip_actual_anchors(
    line_number: int,
    line: str,
    anchors_by_line: dict[int, set[str]],
) -> str:
    """Remove only parser-recognized anchors from a visible source line."""
    allowed = anchors_by_line.get(line_number)
    if not allowed:
        return line
    return ANCHOR_RE.sub(
        lambda match: ""
        if match.group(1).strip().lower() in allowed
        else match.group(0),
        line,
    )


def metadata_value(text: str, label: str) -> str | None:
    match = re.search(rf"^- {re.escape(label)}：(.+)$", text, re.MULTILINE)
    return match.group(1).strip() if match else None


def metadata_any(text: str, labels: tuple[str, ...]) -> str | None:
    for label in labels:
        value = metadata_value(text, label)
        if value:
            return value
    return None


def is_review_date(raw_value: str) -> bool:
    """Accept only a zero-padded, real ISO calendar date."""
    value = raw_value.strip().strip("`").strip()
    try:
        parsed = date.fromisoformat(value)
    except ValueError:
        return False
    return value == parsed.isoformat()


def document_identity_text(text: str) -> str:
    match = re.search(r"^## 文档身份\s*$([\s\S]*?)(?=^##\s|\Z)", text, re.MULTILINE)
    return match.group(1) if match else text


def split_link_target(raw: str) -> tuple[str, str | None]:
    target = raw.strip().split(None, 1)[0].strip("<>")
    if "#" not in target:
        return target, None
    path, fragment = target.split("#", 1)
    return path, unquote(fragment)


def is_external_link_target(target: str) -> bool:
    return "://" in target or target.startswith(("mailto:", "//"))


def github_heading_slug(value: str) -> str:
    value = HTML_TAG_RE.sub("", html.unescape(value)).strip().lower()
    value = re.sub(r"[`*_~]", "", value)
    value = re.sub(r"[^\w\-\u0080-\uffff ]", "", value, flags=re.UNICODE)
    return re.sub(r"\s+", "-", value)


def fragment_exists(text: str, fragment: str) -> bool:
    wanted = fragment.strip().lower()
    for anchor, _number in actual_anchor_occurrences(text):
        if anchor.strip().lower() == wanted:
            return True
    visible = "\n".join(line for _, line in visible_lines(text))
    for line in visible.splitlines():
        match = re.match(r"^ {0,3}#{1,6}\s+(.+?)\s*#*\s*$", line)
        if match and github_heading_slug(match.group(1)) == wanted:
            return True
    return False


def selected_target_text(
    root: Path,
    head: str,
    source: Path,
    target_path: Path,
    source_text: str,
    use_worktree_content: bool,
) -> str | None:
    if target_path == source:
        return source_text
    try:
        target_rel = target_path.relative_to(root)
    except ValueError:
        return None
    if use_worktree_content:
        require_safe_worktree_path(root, target_rel.as_posix())
        if not target_path.is_file():
            return None
        return target_path.read_text(encoding="utf-8")
    return git_text(root, head, target_rel.as_posix())


def resolve_link_path(source: Path, target: str, use_worktree_content: bool) -> Path:
    candidate = source.parent / target
    if use_worktree_content:
        return candidate.resolve()
    # Do not follow live-worktree symlinks while resolving a frozen revision.
    return Path(os.path.abspath(candidate))


def validate_link(
    root: Path,
    head: str,
    source: Path,
    raw_target: str,
    source_text: str,
    errors: list[str],
    path: str,
    code: str,
    use_worktree_content: bool,
) -> tuple[Path | None, str | None]:
    target, fragment = split_link_target(raw_target)
    if not target and fragment is not None:
        target_path = source
    elif not target or "://" in target or target.startswith("mailto:"):
        return None, fragment
    else:
        target_path = resolve_link_path(source, target, use_worktree_content)
    try:
        target_rel = target_path.relative_to(root)
    except ValueError:
        fail(errors, code, path, f"link escapes repository: {raw_target}")
        return None, fragment
    target_text = selected_target_text(
        root,
        head,
        source,
        target_path,
        source_text,
        use_worktree_content,
    )
    if target_text is None:
        fail(errors, code, path, f"missing target: {target_rel.as_posix()}")
        return None, fragment
    if fragment and not fragment_exists(target_text, fragment):
        fail(errors, "invalid-fragment", path, f"{target_rel.as_posix()}#{fragment}")
    return target_path, fragment


def markdown_links(text: str) -> list[tuple[int, str, str]]:
    return [(link.line, link.target, link.target) for link in parse_markdown_links(text)]


def authority_lines(lines: list[tuple[int, str]]) -> list[tuple[int, str]]:
    return [
        (number, line)
        for number, line in lines
        if re.search(r"专业(?:域)?权威\s*[:：]", line)
    ]


def id_tokens(text: str) -> set[str]:
    tokens: set[str] = set()
    # Cross-layer Trace IDs have their own declaration and reference contract.
    # Strip only valid UUID v4 IDs so malformed local references remain visible.
    text = re.sub(
        r"\bO7-REQ-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}(?![\w-])",
        "",
        text,
    )
    for match in ID_RE.finditer(text):
        token = match.group(1).upper()
        # A wildcard such as REQ-TOPIC-* is prose describing an ID family,
        # not a declaration or a cross-file reference.  Likewise, do not
        # turn a dangling hyphen into a truncated ID when scanning Markdown.
        if "*" in token or token.endswith("-"):
            continue
        tokens.add(token)
    return tokens


def declaration_identifier(line: str) -> str | None:
    """Return an explicit heading/bullet/table REQ or AC declaration."""
    remainder = line
    heading = HEADING_PREFIX_RE.match(remainder)
    if heading:
        remainder = remainder[heading.end() :]
    else:
        if line.lstrip().startswith("|"):
            cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
            if len(cells) < 2 or not re.search(
                r"要求|验收|说明|描述|条件|requirement|acceptance|description|condition",
                "|".join(cells[1:]),
                re.IGNORECASE,
            ):
                return None
            table = TABLE_ID_CELL_RE.fullmatch(cells[0])
            if not table:
                return None
            token = table.group(1).upper()
            return None if "*" in token or token.endswith("-") else token
        bullet = DECLARATION_PREFIX_RE.match(remainder)
        if not bullet:
            return None
        remainder = remainder[bullet.end() :]
    match = ID_RE.match(remainder)
    if not match:
        return None
    token = match.group(1).upper()
    if "*" in token or token.endswith("-"):
        return None
    return token


def heading_identifier(line: str) -> str | None:
    if not HEADING_PREFIX_RE.match(line):
        return None
    return declaration_identifier(line)


def check_metadata(path: str, text: str, errors: list[str]) -> None:
    text = document_identity_text("\n".join(line for _, line in visible_lines(text)))
    required = ["生命周期", "Owner role", "Last reviewed"]
    authority_labels = ("专业域权威", "专业权威")
    if path.endswith(".prd.md"):
        required.extend(["所属产品模块", "上位产品 PRD"])
    else:
        required.extend(["配对产品 PRD", "上位产品 PRD"])
    for label in required:
        if not metadata_value(text, label):
            fail(errors, "missing-metadata", path, label)
    review_date = metadata_value(text, "Last reviewed")
    if review_date and not is_review_date(review_date):
        fail(errors, "invalid-review-date", path, review_date)
    if not metadata_any(text, authority_labels):
        fail(errors, "missing-metadata", path, "专业域权威 or 专业权威")
    lifecycle = metadata_value(text, "生命周期")
    normalized_lifecycle = lifecycle.strip().strip("`").lower() if lifecycle else None
    if lifecycle and normalized_lifecycle not in LIFECYCLE_VALUES:
        fail(errors, "invalid-lifecycle", path, lifecycle)


def check_minimum_topic_content(path: str, text: str, errors: list[str]) -> None:
    visible = "\n".join(line for _, line in visible_lines(text))
    required_groups = {
        "player-scenario": ("玩家", "使用者"),
        "scope": ("范围", "Non-Goals"),
        "normal-path": ("正常", "路径", "流程", "循环", "walkthrough", "链路"),
        "decision": ("选择", "决策", "取舍", "可比较"),
        "failure-recovery": ("失败", "恢复", "阻塞", "blocked"),
        "acceptance": ("验收", "AC-"),
        "evidence-boundary": ("证据", "evidence", "非证明项"),
        "unresolved": ("未决", "假设", "待决", "临时范围"),
    }
    for code, needles in required_groups.items():
        if not any(needle.lower() in visible.lower() for needle in needles):
            fail(errors, f"missing-{code}", path, "topic minimum content marker")


def check_minimum_design_content(path: str, text: str, errors: list[str]) -> None:
    visible = "\n".join(line for _, line in visible_lines(text))
    required_groups = {
        "player-scenario": ("玩家", "使用者"),
        "design-decision": ("选择", "决策", "后果", "取舍"),
        "failure-recovery": ("失败", "恢复", "阻塞", "blocked"),
        "validation-boundary": ("验证", "证据", "evidence"),
    }
    for code, needles in required_groups.items():
        if not any(needle.lower() in visible.lower() for needle in needles):
            fail(errors, f"missing-{code}", path, "design minimum content marker")


def check_active_topic_cardinality(path: str, text: str, errors: list[str]) -> None:
    if not path.endswith(".prd.md") or path.endswith("/prd.md"):
        return
    identity = document_identity_text("\n".join(line for _, line in visible_lines(text)))
    lifecycle = metadata_value(identity, "生命周期")
    if not lifecycle or lifecycle.strip().strip("`").lower() != "active":
        return
    declarations = {
        identifier
        for _number, line in visible_lines(text)
        if (identifier := declaration_identifier(line))
    }
    if not any(identifier.startswith("REQ-") for identifier in declarations):
        fail(errors, "active-topic-missing-requirement", path, "active topic must declare at least one REQ-* requirement")
    if not any(identifier.startswith("AC-") for identifier in declarations):
        fail(errors, "active-topic-missing-acceptance", path, "active topic must declare at least one AC-* acceptance")


def lifecycle_field_value(raw_value: str) -> str:
    value = re.sub(r"!\[([^\]]*)\]\([^)]*\)", r"\1", raw_value)
    value = re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", value)
    value = HTML_TAG_RE.sub("", value)
    return value.strip().strip("`").strip().lower()


def is_lifecycle_placeholder(raw_value: str) -> bool:
    return lifecycle_field_value(raw_value) in LIFECYCLE_PLACEHOLDERS


def lifecycle_repository_fragment_link(
    root: Path,
    head: str,
    source: Path,
    source_text: str,
    path: str,
    field: str,
    raw_target: str,
    errors: list[str],
    use_worktree_content: bool,
) -> bool:
    target, fragment = split_link_target(raw_target)
    code = f"lifecycle-invalid-{field}-link"
    if not target or is_external_link_target(target):
        return False
    target_path = resolve_link_path(source, target, use_worktree_content)
    try:
        target_rel = target_path.relative_to(root)
    except ValueError:
        return False
    target_text = selected_target_text(
        root,
        head,
        source,
        target_path,
        source_text,
        use_worktree_content,
    )
    if target_text is None:
        fail(errors, code, path, f"lifecycle {field} link target is missing: {target_rel.as_posix()}")
        return False
    if fragment and not fragment_exists(target_text, fragment):
        fail(errors, code, path, f"lifecycle {field} link fragment is unresolved: {target_rel.as_posix()}#{fragment}")
        return False
    return True


def check_lifecycle_closure(
    root: Path,
    head: str,
    path: str,
    text: str,
    errors: list[str],
    use_worktree_content: bool,
) -> None:
    if not path.endswith(".prd.md") or path.endswith("/prd.md"):
        return
    identity = document_identity_text("\n".join(line for _, line in visible_lines(text)))
    lifecycle = metadata_value(identity, "生命周期")
    if not lifecycle or lifecycle.strip().strip("`").lower() not in {"superseded", "retired"}:
        return
    lines = visible_lines(text)
    required_fields = {
        "receiving-authority": r"^\s*(?:[-+*]\s*)?(?:接收|承接)\s*(?:authority|owner|方|责任)\s*[:：]\s*(?P<value>.*)$",
        "remaining-semantics": r"^\s*(?:[-+*]\s*)?(?:剩余语义|保留语义|仍然适用|剩余范围)\s*[:：]\s*(?P<value>.*)$",
        "stable-reference": r"^\s*(?:[-+*]\s*)?稳定(?:引用|链接)\s*[:：]\s*(?P<value>.*)$",
        "deletion-condition": r"^\s*(?:[-+*]\s*)?(?:删除条件|删除时机|可删除|清理条件)\s*[:：]\s*(?P<value>.*)$",
    }
    codes = {
        "receiving-authority": "lifecycle-missing-receiving-authority",
        "remaining-semantics": "lifecycle-missing-remaining-semantics",
        "stable-reference": "lifecycle-missing-stable-reference",
        "deletion-condition": "lifecycle-missing-deletion-condition",
    }
    links_by_line: dict[int, list[str]] = {}
    for number, _raw, target in markdown_links(text):
        links_by_line.setdefault(number, []).append(target)
    source = root / path
    for field, pattern in required_fields.items():
        match = next(
            (pattern_match for _number, line in lines if (pattern_match := re.match(pattern, line, re.IGNORECASE))),
            None,
        )
        if not match:
            fail(errors, codes[field], path, f"{lifecycle.strip().strip('`')} topic requires lifecycle closure field {field}")
            continue
        raw_value = match.group("value")
        if is_lifecycle_placeholder(raw_value) and not (
            field == "remaining-semantics"
            and lifecycle.strip().strip("`").lower() == "retired"
            and lifecycle_field_value(raw_value) == "无"
        ):
            fail(errors, "lifecycle-placeholder", path, f"lifecycle closure field {field} has a placeholder value")
            continue
        if field not in {"receiving-authority", "stable-reference"}:
            continue
        field_line = next(
            number for number, line in lines if re.match(pattern, line, re.IGNORECASE)
        )
        candidates = links_by_line.get(field_line, [])
        if not candidates:
            fail(errors, f"lifecycle-missing-{field}-link", path, f"lifecycle closure field {field} requires a repository-relative fragment link")
            continue
        if not any(
            lifecycle_repository_fragment_link(
                root,
                head,
                source,
                text,
                path,
                field,
                candidate,
                errors,
                use_worktree_content,
            )
            for candidate in candidates
        ):
            if not any(error.startswith(f"product-doc-content: lifecycle-invalid-{field}-link: {path}:") for error in errors):
                fail(errors, f"lifecycle-invalid-{field}-link", path, f"lifecycle closure field {field} requires a resolvable repository-relative fragment link")


def linked_repository_paths(
    root: Path,
    source: Path,
    text: str,
    use_worktree_content: bool,
) -> set[str]:
    paths: set[str] = set()
    for _number, _raw, target in markdown_links(text):
        target_path, _fragment = split_link_target(target)
        if not target_path or is_external_link_target(target_path):
            continue
        resolved = resolve_link_path(source, target_path, use_worktree_content)
        try:
            paths.add(resolved.relative_to(root).as_posix())
        except ValueError:
            continue
    return paths


def declared_prd_relations(text: str) -> tuple[set[str], set[str], set[tuple[str, str]]]:
    """Return declared REQ/AC IDs and their local heading-level relations."""
    lines = visible_lines(text)
    declarations = {
        identifier
        for _number, line in lines
        if (identifier := declaration_identifier(line))
    }
    anchors_by_line: dict[int, set[str]] = {}
    for anchor, number in actual_anchor_occurrences(text):
        anchors_by_line.setdefault(number, set()).add(anchor.strip().lower())
    requirements = {identifier for identifier in declarations if identifier.startswith("REQ-")}
    acceptances = {identifier for identifier in declarations if identifier.startswith("AC-")}
    relations: set[tuple[str, str]] = set()
    for _number, line in lines:
        identifier = declaration_identifier(line)
        if not identifier or heading_identifier(line):
            continue
        references = id_tokens(line) - {identifier}
        if identifier in requirements:
            relations.update(
                (identifier, acceptance)
                for acceptance in references & acceptances
            )
        elif identifier in acceptances:
            relations.update(
                (requirement, identifier)
                for requirement in references & requirements
            )
    headings: list[tuple[int, str | None, int]] = []
    for index, (_number, line) in enumerate(lines):
        heading = HEADING_PREFIX_RE.match(line)
        if not heading:
            continue
        headings.append(
            (
                index,
                heading_identifier(line),
                len(heading.group(0).lstrip().split()[0]),
            )
        )
    for heading_index, (index, identifier, level) in enumerate(headings):
        if not identifier:
            continue
        end = len(lines)
        for next_index, _next_identifier, next_level in headings[heading_index + 1 :]:
            if next_level <= level:
                end = next_index
                break
        block = "\n".join(
            strip_actual_anchors(number, line, anchors_by_line)
            for number, line in lines[index + 1 : end]
        )
        if identifier in requirements:
            relations.update((identifier, acceptance) for acceptance in id_tokens(block) & acceptances)
        elif identifier in acceptances:
            relations.update((requirement, identifier) for requirement in id_tokens(block) & requirements)
    return requirements, acceptances, relations


def design_mapping_relations(
    root: Path,
    source: Path,
    expected_prd: str,
    text: str,
    use_worktree_content: bool,
) -> tuple[set[str], set[str], set[tuple[str, str]]]:
    """Extract same-row REQ/AC links targeting the paired PRD."""
    lines = visible_lines(text)
    # Every explicit same-row association is checked. A named mapping section
    # must not hide contradicting relations in another table.
    mapping_line_numbers = {
        number for number, line in lines if line.lstrip().startswith("|")
    }
    row_requirements: dict[int, set[str]] = {}
    row_acceptances: dict[int, set[str]] = {}
    for number, _raw, target in markdown_links(text):
        if number not in mapping_line_numbers:
            continue
        target_path, fragment = split_link_target(target)
        if not target_path or is_external_link_target(target_path) or not fragment:
            continue
        resolved = resolve_link_path(source, target_path, use_worktree_content)
        try:
            relative = resolved.relative_to(root).as_posix()
        except ValueError:
            continue
        if relative != expected_prd:
            continue
        normalized = fragment.upper()
        if normalized.startswith("REQ-") and re.fullmatch(r"REQ-[A-Z0-9][A-Z0-9_-]*", normalized):
            row_requirements.setdefault(number, set()).add(normalized)
        elif normalized.startswith("AC-") and re.fullmatch(r"AC-[A-Z0-9][A-Z0-9_-]*", normalized):
            row_acceptances.setdefault(number, set()).add(normalized)
    relations: set[tuple[str, str]] = set()
    for number in row_requirements.keys() & row_acceptances.keys():
        if len(row_requirements[number]) > 1 and len(row_acceptances[number]) > 1:
            # A grouped summary does not assert every Cartesian-product pair.
            # Each endpoint still receives the ordinary ID/link validation.
            continue
        relations.update(
            (requirement, acceptance)
            for requirement in row_requirements[number]
            for acceptance in row_acceptances[number]
        )
    return (
        set().union(*row_requirements.values()) if row_requirements else set(),
        set().union(*row_acceptances.values()) if row_acceptances else set(),
        relations,
    )


def check_design_prd_mapping(
    root: Path,
    head: str,
    path: str,
    text: str,
    errors: list[str],
    use_worktree_content: bool,
    paired_prd: str | None = None,
) -> None:
    source = root / path
    expected_prd = paired_prd or path.removesuffix(".design.md") + ".prd.md"
    prd_path = root / expected_prd
    prd_text = selected_target_text(root, head, source, prd_path, text, use_worktree_content)
    if prd_text is None:
        return
    expected_requirements, expected_acceptances, expected_relations = declared_prd_relations(prd_text)
    mapped_requirements, mapped_acceptances, mapped_relations = design_mapping_relations(
        root,
        source,
        expected_prd,
        text,
        use_worktree_content,
    )
    missing_requirements = expected_requirements - mapped_requirements
    missing_acceptances = expected_acceptances - mapped_acceptances
    missing_relations = expected_relations - mapped_relations
    if missing_requirements or missing_acceptances:
        detail = []
        if missing_requirements:
            detail.append(f"REQ missing={','.join(sorted(missing_requirements))}")
        if missing_acceptances:
            detail.append(f"AC missing={','.join(sorted(missing_acceptances))}")
        if missing_relations:
            detail.append(
                "relations missing="
                + ",".join(f"{requirement}->{acceptance}" for requirement, acceptance in sorted(missing_relations))
            )
        fail(errors, "design-prd-mapping-incomplete", path, "; ".join(detail))
    extra_requirements = mapped_requirements - expected_requirements
    extra_acceptances = mapped_acceptances - expected_acceptances
    known_requirements = {requirement for requirement, _ in expected_relations}
    known_acceptances = {acceptance for _, acceptance in expected_relations}
    extra_relations = {(requirement, acceptance) for requirement, acceptance in mapped_relations - expected_relations
                       if requirement in known_requirements and acceptance in known_acceptances}
    if extra_requirements or extra_acceptances or (expected_relations and extra_relations):
        detail = []
        if extra_requirements:
            detail.append(f"REQ extra={','.join(sorted(extra_requirements))}")
        if extra_acceptances:
            detail.append(f"AC extra={','.join(sorted(extra_acceptances))}")
        relation_mismatch = extra_relations if expected_relations else set()
        if relation_mismatch:
            detail.append(
                "relations extra="
                + ",".join(f"{requirement}->{acceptance}" for requirement, acceptance in sorted(relation_mismatch))
            )
        fail(errors, "design-prd-mapping-inconsistent", path, "; ".join(detail))


def declared_pair_paths(root: Path, source: Path, text: str, label: str,
                        use_worktree_content: bool) -> tuple[bool, set[str]]:
    """Read explicit pairing metadata without prescribing the paired filename."""
    lines = {number for number, line in visible_lines(text)
             if re.match(rf"^\s*(?:[-+*]\s*)?{label}\s*[:：]", line)}
    targets = set()
    for number, _raw, target in markdown_links(text):
        if number not in lines:
            continue
        raw_path, _fragment = split_link_target(target)
        if not raw_path or is_external_link_target(raw_path):
            continue
        resolved = resolve_link_path(source, raw_path, use_worktree_content)
        try:
            targets.add(resolved.relative_to(root).as_posix())
        except ValueError:
            continue
    return bool(lines), targets


def check_active_topic_design_contract(
    root: Path,
    head: str,
    path: str,
    text: str,
    errors: list[str],
    use_worktree_content: bool,
) -> None:
    identity = document_identity_text("\n".join(line for _, line in visible_lines(text)))
    lifecycle = metadata_value(identity, "生命周期")
    if not lifecycle or lifecycle.strip().strip("`").lower() != "active":
        return
    visible = "\n".join(line for _, line in visible_lines(text))
    decision = re.search(
        r"设计判定\s*[:：]\s*`?([a-z][a-z-]+)`?",
        visible,
        re.IGNORECASE,
    )
    source = root / path
    expected_design = path.removesuffix(".prd.md") + ".design.md"
    expected_design_exists = (root / expected_design).is_file()
    pair_declared, paired_designs = declared_pair_paths(root, source, text, r"配对产品设计", use_worktree_content)
    if not decision:
        code = "missing-design-decision" if expected_design_exists else "missing-design-or-exemption"
        fail(errors, code, path, "active topic must declare paired-design or simple-topic-exemption")
        if not pair_declared:
            return
    mode = decision.group(1).lower() if decision else "paired-design"
    if mode == "paired-design":
        if not paired_designs:
            if expected_design in linked_repository_paths(root, source, text, use_worktree_content):
                paired_designs.add(expected_design)
        if not paired_designs:
            fail(errors, "missing-paired-design-link", path, "declared paired-design requires a readable product design link")
            return
        for paired_design in sorted(paired_designs):
            if not paired_design.startswith("doc/product/") or not paired_design.endswith(".design.md"):
                fail(errors, "invalid-paired-design-link", path, f"declared pair must target a product design: {paired_design}")
                continue
            design_text = selected_target_text(root, head, source, root / paired_design, text, use_worktree_content)
            if design_text is None:
                fail(errors, "missing-paired-design-link", path, paired_design)
                continue
            design_identity = document_identity_text("\n".join(line for _, line in visible_lines(design_text)))
            design_lifecycle = metadata_value(design_identity, "生命周期")
            if design_lifecycle and design_lifecycle.strip().strip("`").lower() != "active":
                fail(errors, "inactive-paired-design", path, f"active topic declares inactive design: {paired_design}")
        return
    if mode == "simple-topic-exemption":
        reason_match = re.search(r"设计适用性理由[ \t]*[:：][ \t]*(.*)", visible)
        if not reason_match or lifecycle_field_value(reason_match.group(1)) in DESIGN_EXEMPTION_REASON_PLACEHOLDERS:
            fail(errors, "missing-design-exemption-reason", path, "simple-topic-exemption requires 设计适用性理由")
        # A reasoned local design decision is sufficient. Optional historical
        # Issue/PR references remain readable but do not admit ordinary edits.
        return
    fail(errors, "missing-design-or-exemption", path, "active topic design decision must be paired-design or simple-topic-exemption")


def trace_table_cells(line: str) -> list[str]:
    stripped = line.strip()
    if not stripped.startswith("|"):
        return []
    if stripped.endswith("|"):
        stripped = stripped[:-1]
    stripped = stripped[1:]
    return [cell.strip() for cell in re.split(r"(?<!\\)\|", stripped)]


def is_trace_table_separator(line: str) -> bool:
    cells = trace_table_cells(line)
    if not cells:
        return False
    return all(bool(re.fullmatch(r":?\s*-{3,}\s*:?", cell)) for cell in cells)


def normalized_trace_header(cell: str) -> str:
    value = html.unescape(cell)
    value = re.sub(r"`([^`]*)`", r"\1", value)
    value = HTML_TAG_RE.sub("", value)
    return value.strip().lower()


def trace_header_has_id(header: str, identifier: str) -> bool:
    return bool(re.search(rf"(?<![a-z0-9]){identifier}(?![a-z0-9])", header, re.IGNORECASE))


def trace_heading_is_explicit(line: str) -> bool:
    if not HEADING_PREFIX_RE.match(line):
        return False
    return (
        bool(re.search(r"追踪|trace", line, re.IGNORECASE))
        and bool(re.search(r"owner|authority|测试层级|test[_ ]?tier", line, re.IGNORECASE))
    )


def trace_table_semantics(
    text: str,
) -> list[tuple[int, dict[str, tuple[int, ...]], list[tuple[int, list[str]]]]]:
    """Return explicitly structured semantic trace tables.

    Ordinary two-column REQ/AC mapping tables are intentionally ignored. A
    table is a semantic trace table only when it declares a relation header
    plus at least one trace field, declares aggregate criteria plus at least
    one trace field, or sits under an explicit owner/authority/test-tier trace
    heading. This keeps prose and legacy mapping tables out of the row-level
    contract while allowing aggregate criterion definitions to be checked
    separately.
    """
    lines = visible_lines(text)
    tables: list[tuple[int, dict[str, tuple[int, ...]], list[tuple[int, list[str]]]]] = []
    for index in range(len(lines) - 1):
        header_number, header_line = lines[index]
        headers = trace_table_cells(header_line)
        if not headers or not is_trace_table_separator(lines[index + 1][1]):
            continue
        rows: list[tuple[int, list[str]]] = []
        row_index = index + 2
        while row_index < len(lines):
            number, line = lines[row_index]
            cells = trace_table_cells(line)
            if not cells:
                break
            rows.append((number, cells))
            row_index += 1

        normalized = [normalized_trace_header(header) for header in headers]
        req_columns = tuple(
            index for index, header in enumerate(normalized) if trace_header_has_id(header, "REQ")
        )
        ac_columns = tuple(
            index for index, header in enumerate(normalized) if trace_header_has_id(header, "AC")
        )
        relation_columns = tuple(
            index for index in req_columns if index in ac_columns
        )
        if not relation_columns and req_columns and ac_columns:
            relation_columns = tuple(sorted(set(req_columns + ac_columns)))
        semantic_columns = {
            "owner": tuple(
                index
                for index, header in enumerate(normalized)
                if re.search(r"\bowner\b|专业\s*(?:owner|负责人|责任)", header, re.IGNORECASE)
            ),
            "authority": tuple(
                index
                for index, header in enumerate(normalized)
                if re.search(r"\bauthority\b|权威", header, re.IGNORECASE)
            ),
            "evidence": tuple(
                index
                for index, header in enumerate(normalized)
                if re.search(r"\bevidence\b|证据", header, re.IGNORECASE)
            ),
            "tier": tuple(
                index
                for index, header in enumerate(normalized)
                if re.search(r"测试层级|test[_ ]?tier|\btier\b", header, re.IGNORECASE)
            ),
        }
        criterion_columns = tuple(
            index
            for index, header in enumerate(normalized)
            if re.search(
                r"成功标准|产品承诺|验收标准|\bcriterion\b|\bcriteria\b",
                header,
                re.IGNORECASE,
            )
        )
        relation_signal = bool(req_columns or ac_columns or relation_columns)
        semantic_signal = any(semantic_columns.values())
        criterion_signal = bool(
            criterion_columns
            and any(
                trace_criterion_ids(cells[index])
                or strict_trace_ids(cells[index])
                for _number, cells in rows
                for index in criterion_columns
                if index < len(cells)
            )
        )
        explicit_heading = False
        for heading_index in range(index - 1, -1, -1):
            if HEADING_PREFIX_RE.match(lines[heading_index][1]):
                explicit_heading = trace_heading_is_explicit(lines[heading_index][1])
                break
        if ((relation_signal or criterion_signal) and semantic_signal) or explicit_heading:
            tables.append(
                (
                    header_number,
                    {
                        "req": req_columns,
                        "ac": ac_columns,
                        "relation": relation_columns,
                        "criterion": criterion_columns,
                        **semantic_columns,
                    },
                    rows,
                )
            )
    return tables


def trace_cell_text(cell: str) -> str:
    value = re.sub(r"!\[([^]]*)\]\([^)]*\)", r"\1", cell)
    value = re.sub(r"\[([^]]+)\]\([^)]*\)", r"\1", value)
    value = re.sub(r"`([^`]*)`", r"\1", value)
    value = HTML_TAG_RE.sub("", html.unescape(value))
    return re.sub(r"\s+", " ", value).strip()


def trace_criterion_ids(value: str) -> set[str]:
    """Return aggregate criterion IDs from a trace-table cell."""
    return {
        match.group(0).upper()
        for match in TRACE_CRITERION_ID_RE.finditer(trace_cell_text(value))
        if not match.group(0).upper().startswith(("REQ-", "AC-", "PRD-"))
    }


TRACE_EMPTY_VALUES = frozenset(
    {
        "",
        "-",
        "—",
        "–",
        "n/a",
        "na",
        "none",
        "tbd",
        "todo",
        "待补",
        "待定",
        "未定",
        "证据",
        "evidence",
    }
)


def trace_cell_is_nonempty(cell: str) -> bool:
    value = trace_cell_text(cell)
    return value.casefold() not in TRACE_EMPTY_VALUES and bool(value)


def strict_trace_ids(value: str) -> set[str]:
    return {
        token
        for token in id_tokens(value)
        if re.fullmatch(r"(?:REQ|AC)-[A-Z0-9][A-Z0-9_-]*", token, re.IGNORECASE)
    }


def trace_navigable_ids(
    source: Path,
    cell: str,
    use_worktree_content: bool,
) -> set[str]:
    """Return only fragment IDs that navigate within the current topic."""
    identifiers: set[str] = set()
    for link in parse_markdown_links(cell):
        target_path, fragment = split_link_target(link.target)
        if not fragment or is_external_link_target(target_path):
            continue
        if target_path and not target_path.lower().endswith(".md"):
            continue
        resolved_target = (
            source
            if not target_path
            else resolve_link_path(source, target_path, use_worktree_content)
        )
        same_topic = (
            resolved_target.resolve() == source.resolve()
            if use_worktree_content
            else resolved_target == source
        )
        if not same_topic:
            continue
        identifier = fragment.upper()
        if re.fullmatch(r"(?:REQ|AC)-[A-Z0-9][A-Z0-9_-]*", identifier, re.IGNORECASE):
            identifiers.add(identifier)
    return identifiers


def check_active_topic_trace_tables(
    root: Path,
    path: str,
    text: str,
    errors: list[str],
    use_worktree_content: bool,
) -> None:
    """Check active-topic leaf traces and aggregate criterion definitions.

    Existing error codes retain the ``paired-trace`` prefix for compatibility
    with the focused gate and its consumers; the contract now applies to
    every active topic that declares a row-level REQ/AC trace table.
    """
    if not path.endswith(".prd.md") or path.endswith("/prd.md"):
        return
    identity = document_identity_text("\n".join(line for _, line in visible_lines(text)))
    lifecycle = metadata_value(identity, "生命周期")
    if (
        not lifecycle
        or lifecycle.strip().strip("`").lower() != "active"
    ):
        return
    traced_relations: set[tuple[str, str]] = set()
    traced_ids: set[str] = set()
    relation_table_seen = False
    source = root / path
    for header_line, columns, rows in trace_table_semantics(text):
        trace_id_columns = tuple(
            sorted(set(columns["relation"] + columns["criterion"]))
        )
        for _number, cells in rows:
            for index in trace_id_columns:
                if index < len(cells):
                    traced_ids.update(strict_trace_ids(cells[index]))
        criterion_ids = {
            criterion_id
            for _number, cells in rows
            for index in columns["criterion"]
            if index < len(cells)
            for criterion_id in trace_criterion_ids(cells[index])
        }
        if criterion_ids:
            body_text = "\n".join(
                line
                for _number, line in visible_lines(text)
                if not line.lstrip().startswith("|")
            )
            for criterion_id in sorted(criterion_ids):
                if not re.search(
                    rf"(?<![A-Z0-9]){re.escape(criterion_id)}(?![A-Z0-9])",
                    body_text,
                ):
                    fail(
                        errors,
                        "trace-criterion-missing-body-definition",
                        path,
                        f"aggregate criterion {criterion_id} must be defined outside its trace table",
                    )

        # Existing aggregate contracts intentionally remain valid without
        # REQ/AC columns. Only relation tables enter the same-row leaf
        # contract below.
        if not (columns["req"] or columns["ac"] or columns["relation"]):
            continue
        relation_table_seen = True
        missing_columns = []
        if not columns["req"] or not columns["ac"] or not columns["relation"]:
            missing_columns.append("REQ/AC relation")
        for field in ("owner", "authority", "evidence", "tier"):
            if not columns[field]:
                missing_columns.append(field)
        if missing_columns:
            fail(
                errors,
                "paired-trace-missing-column",
                path,
                f"trace table at line {header_line} requires {', '.join(missing_columns)} column(s)",
            )

        for number, cells in rows:
            relation_cells = [
                cells[index]
                for index in columns["relation"]
                if index < len(cells)
            ]
            relation_text = " ".join(relation_cells)
            declared_ids = strict_trace_ids(relation_text)
            linked_ids = set().union(
                *(
                    trace_navigable_ids(source, cell, use_worktree_content)
                    for cell in relation_cells
                )
            )
            requirement_ids = {
                identifier for identifier in linked_ids if identifier.startswith("REQ-")
            }
            acceptance_ids = {
                identifier for identifier in linked_ids if identifier.startswith("AC-")
            }
            if not requirement_ids or not acceptance_ids:
                fail(
                    errors,
                    "paired-trace-missing-relation",
                    path,
                    f"trace row at line {number} must contain REQ and AC in the same row",
                )
            unlinked_ids = declared_ids - linked_ids
            if unlinked_ids:
                fail(
                    errors,
                    "paired-trace-unlinked-relation",
                    path,
                    f"trace row at line {number} requires navigable links for {', '.join(sorted(unlinked_ids))}",
                )
            if len(requirement_ids) <= 1 or len(acceptance_ids) <= 1:
                traced_relations.update(
                    (requirement, acceptance)
                    for requirement in requirement_ids
                    for acceptance in acceptance_ids
                )

            for field in ("owner", "evidence"):
                indices = columns[field]
                values = [cells[index] for index in indices if index < len(cells)]
                if not values or not any(trace_cell_is_nonempty(value) for value in values):
                    fail(
                        errors,
                        f"paired-trace-empty-{field}",
                        path,
                        f"trace row at line {number} requires non-empty {field}",
                    )

            authority_values = [
                cells[index] for index in columns["authority"] if index < len(cells)
            ]
            authority_link = False
            for cell in authority_values:
                for link in parse_markdown_links(cell):
                    target_path, fragment = split_link_target(link.target)
                    if (
                        target_path
                        and not is_external_link_target(target_path)
                        and target_path.lower().endswith(".md")
                        and fragment
                    ):
                        authority_link = True
                        break
                if authority_link:
                    break
            if not authority_link:
                fail(
                    errors,
                    "paired-trace-authority-not-link",
                    path,
                    f"trace row at line {number} requires a repository-relative authority Markdown link",
                )

            tier_values = [cells[index] for index in columns["tier"] if index < len(cells)]
            tier_text = " ".join(tier_values)
            if not re.search(
                r"(?<![A-Za-z0-9_])test_tier_(?:required|full)(?![A-Za-z0-9_])",
                tier_text,
            ):
                fail(
                    errors,
                    "paired-trace-invalid-test-tier",
                    path,
                    f"trace row at line {number} requires exact test_tier_required or test_tier_full",
                )

    if not relation_table_seen:
        fail(
            errors,
            "paired-trace-missing-table",
            path,
            "active topic requires a semantic REQ/AC trace table",
        )
        return

    declared_requirements, declared_acceptances, expected_relations = declared_prd_relations(text)
    known_requirements = {requirement for requirement, _ in expected_relations}
    known_acceptances = {acceptance for _, acceptance in expected_relations}
    extra_relations = {(requirement, acceptance) for requirement, acceptance in traced_relations - expected_relations
                       if requirement in known_requirements and acceptance in known_acceptances}
    if expected_relations and extra_relations:
        fail(errors, "paired-trace-inconsistent-relation", path,
             "declared trace rows contradict requirement/acceptance associations: "
             + ", ".join(f"{requirement}->{acceptance}" for requirement, acceptance in sorted(extra_relations)))
    missing_relations = expected_relations - traced_relations
    if missing_relations:
        fail(
            errors,
            "paired-trace-missing-relation",
            path,
            "declared REQ/AC relations lack same-row trace: "
            + ", ".join(
                f"{requirement}->{acceptance}"
                for requirement, acceptance in sorted(missing_relations)
            ),
        )
    missing_ids = (declared_requirements | declared_acceptances) - traced_ids
    if missing_ids:
        fail(
            errors,
            "paired-trace-missing-relation",
            path,
            "declared REQ/AC IDs lack same-row trace: " + ", ".join(sorted(missing_ids)),
        )


def check_requirements(path: str, text: str, errors: list[str]) -> None:
    lines = visible_lines(text)
    anchors_by_line: dict[int, set[str]] = {}
    actual_anchors = actual_anchor_occurrences(text)
    for anchor, number in actual_anchors:
        anchors_by_line.setdefault(number, set()).add(anchor.strip().lower())
    prose = "\n".join(
        strip_actual_anchors(number, line, anchors_by_line) for number, line in lines
    )
    anchors: dict[str, int] = {}
    for anchor, number in actual_anchors:
        key = anchor.strip().lower()
        if key in anchors:
            fail(errors, "duplicate-anchor", path, f"{anchor} at lines {anchors[key]} and {number}")
        else:
            anchors[key] = number

    declarations: dict[str, int] = {}
    declaration_kinds: dict[str, str] = {}
    declaration_levels: dict[str, int] = {}
    for number, line in lines:
        identifier = declaration_identifier(line)
        if not identifier:
            continue
        if identifier in declarations:
            fail(errors, "duplicate-id", path, f"{identifier} at lines {declarations[identifier]} and {number}")
        else:
            declarations[identifier] = number
            if HEADING_PREFIX_RE.match(line):
                declaration_kinds[identifier] = "heading"
                heading = HEADING_PREFIX_RE.match(line)
                assert heading is not None
                declaration_levels[identifier] = len(heading.group(0).lstrip().split()[0])
            else:
                declaration_kinds[identifier] = "legacy"
        if identifier.lower() not in anchors:
            fail(errors, "missing-anchor", path, f"{identifier} has no <a id=\"{identifier.lower()}\"> anchor")

    # An anchor only becomes a usable local target when a declaration also
    # exists.  The common anchor-before-heading form was already accounted for
    # above; an orphan REQ/AC anchor violates the declaration+anchor contract.
    for anchor, number in anchors.items():
        identifier = anchor.upper()
        if identifier.startswith(("REQ-", "AC-")) and identifier not in declarations and identifier in id_tokens(anchor):
            fail(errors, "anchor-without-declaration", path, f"{anchor} at line {number} has no REQ/AC declaration")

    all_tokens = id_tokens(prose)
    local_tokens = set(declarations)
    links = markdown_links(text)
    linked_fragments: list[tuple[int, str]] = []
    external_fragments: set[tuple[int, str]] = set()
    for number, _raw, target in links:
        link_path, fragment = split_link_target(target)
        if fragment:
            if is_external_link_target(link_path):
                external_fragments.add((number, fragment.upper()))
            else:
                linked_fragments.append((number, fragment.upper()))

    for number, line in lines:
        tokens = id_tokens(strip_actual_anchors(number, line, anchors_by_line)) - local_tokens
        if not tokens:
            continue
        line_fragments = {
            fragment.upper()
            for link_number, fragment in linked_fragments
            if link_number == number
        }
        external_line_fragments = {
            fragment
            for link_number, fragment in external_fragments
            if link_number == number
        }
        for token in sorted(tokens):
            if token in line_fragments:
                continue
            if token in external_line_fragments:
                fail(errors, "external-cross-file-id", path, f"{token} must use a repository-relative Markdown path#{token.lower()} link")
                continue
            if path.endswith(".design.md"):
                fail(errors, "unresolved-cross-file-id", path, f"{token} must use a Markdown path#{token.lower()} link")
            else:
                fail(errors, "unresolved-id-reference", path, f"{token} is not a declared local REQ/AC or path#{token.lower()} reference")

    for index, (number, line) in enumerate(lines):
        identifier = heading_identifier(line)
        if not identifier or declaration_kinds.get(identifier) != "heading":
            continue
        level = declaration_levels[identifier]
        end = len(lines)
        for next_index in range(index + 1, len(lines)):
            next_line = lines[next_index][1]
            next_level_match = re.match(r"^ {0,3}(#{1,6})\s+", next_line)
            if next_level_match and len(next_level_match.group(1)) <= level:
                end = next_index
                break
        block = "\n".join(
            strip_actual_anchors(item_number, item, anchors_by_line)
            for item_number, item in lines[index + 1 : end]
        )
        if identifier.startswith("REQ-"):
            acceptance_refs = id_tokens(block) & {token for token in all_tokens if token.startswith("AC-")}
            if not acceptance_refs:
                fail(errors, "req-missing-acceptance", path, identifier)
            for reference in acceptance_refs:
                if reference not in local_tokens and not any(fragment == reference for _, fragment in linked_fragments):
                    fail(errors, "unresolved-acceptance", path, f"{identifier} -> {reference}")
        else:
            requirement_refs = id_tokens(block) & {token for token in all_tokens if token.startswith("REQ-")}
            if not requirement_refs:
                fail(errors, "ac-missing-requirement", path, identifier)
            for reference in requirement_refs:
                if reference not in local_tokens and not any(fragment == reference for _, fragment in linked_fragments):
                    fail(errors, "unresolved-requirement", path, f"{identifier} -> {reference}")


def check_root_document(path: str, text: str, errors: list[str]) -> None:
    """Validate the root identity/content subset owned by the full-corpus contract."""
    identity = document_identity_text("\n".join(line for _, line in visible_lines(text)))
    slug = Path(path).parent.name
    expected = {
        "产品模块 slug": slug,
        "产品层唯一 PRD": path,
        "产品模块总入口": "doc/product/README.md",
        "生命周期": "active",
        "Owner role": "producer_system_designer",
        "后继文档": "无",
    }
    for label, value in expected.items():
        actual = metadata_value(identity, label)
        if not actual or actual.strip().strip("`") != value:
            code = "root-metadata-content" if label in {"生命周期", "Owner role", "后继文档"} else "root-metadata-contract"
            fail(errors, code, path, f"{label} expected {value!r}, got {actual!r}")
    prd_id = metadata_value(identity, "Product PRD-ID")
    if not prd_id or not re.fullmatch(r"`?PRD-PRODUCT-\d{3}`?", prd_id.strip()):
        fail(errors, "root-metadata-contract", path, f"Product PRD-ID is missing or invalid: {prd_id!r}")
    if not metadata_value(identity, "产品模块"):
        fail(errors, "root-metadata-contract", path, "产品模块 is required")
    review_date = metadata_value(identity, "Last reviewed")
    if not review_date:
        fail(errors, "root-metadata-content", path, "Last reviewed is required")
    elif not is_review_date(review_date):
        fail(errors, "root-metadata-content", path, f"Last reviewed is invalid: {review_date}")
    if not re.search(r"^- 下层专业域：.+", identity, re.MULTILINE):
        fail(errors, "root-authority-contract", path, "下层专业域 is required")
    for heading in (
        "## 1. 产品承诺",
        "## 2. 范围",
        "## 3. 权威与冲突处理",
        "## 4. 路线图",
        "## 5. Done：成功标准与验收",
        "### 5.1 验收追踪",
        "## 6. Non-Goals",
    ):
        if heading not in text:
            fail(errors, "root-section-contract", path, f"missing heading prefix {heading!r}")


def check_document(
    root: Path,
    head: str,
    path: str,
    text: str,
    errors: list[str],
    use_worktree_content: bool,
    full_corpus: bool = False,
) -> None:
    source = root / path
    is_root_document = path.endswith("/prd.md")
    if is_root_document:
        check_root_document(path, text, errors)
    lines = visible_lines(text)
    inactive_lifecycle = False
    if not is_root_document:
        check_metadata(path, text, errors)
        check_lifecycle_closure(root, head, path, text, errors, use_worktree_content)
        identity = document_identity_text("\n".join(line for _, line in lines))
        lifecycle = metadata_value(identity, "生命周期")
        inactive_lifecycle = bool(
            lifecycle
            and lifecycle.strip().strip("`").lower() in {"superseded", "retired"}
        )
        if path.endswith(".prd.md") and not inactive_lifecycle:
            check_minimum_topic_content(path, text, errors)
            check_active_topic_cardinality(path, text, errors)
            check_active_topic_trace_tables(root, path, text, errors, use_worktree_content)
            check_active_topic_design_contract(root, head, path, text, errors, use_worktree_content)
    if not is_root_document and path.endswith(".design.md"):
        if not inactive_lifecycle:
            check_minimum_design_content(path, text, errors)
        expected_prd = path.removesuffix(".design.md") + ".prd.md"
        pair_declared, paired_prds = declared_pair_paths(root, source, text, r"配对(?:产品)?\s*PRD", use_worktree_content)
        pair_targets = set()
        links = markdown_links(text)
        fragments_by_prd: dict[str, set[str]] = {}
        for _number, _raw, target in links:
            target_path, fragment = split_link_target(target)
            if target_path:
                resolved = resolve_link_path(source, target_path, use_worktree_content)
                try:
                    relative = resolved.relative_to(root).as_posix()
                    pair_targets.add(relative)
                    if fragment:
                        if re.fullmatch(r"req-[a-z0-9][a-z0-9_-]*", fragment, re.IGNORECASE):
                            fragments_by_prd.setdefault(relative, set()).add("requirement")
                        if re.fullmatch(r"ac-[a-z0-9][a-z0-9_-]*", fragment, re.IGNORECASE):
                            fragments_by_prd.setdefault(relative, set()).add("acceptance")
                except ValueError:
                    pass
        if not paired_prds:
            if expected_prd in pair_targets:
                paired_prds.add(expected_prd)
        if not paired_prds:
            code = "invalid-paired-prd-link" if pair_declared else "missing-paired-prd-link"
            fail(errors, code, path, "declared product pairing needs a readable PRD link" if pair_declared else "add a product PRD association when applicable; same-name files are optional")
        for paired_prd in sorted(paired_prds):
            if not paired_prd.startswith("doc/product/") or not paired_prd.endswith((".prd.md", "/prd.md")):
                fail(errors, "invalid-paired-prd-link", path, f"declared pair must target a product PRD: {paired_prd}")
                continue
            if not inactive_lifecycle and fragments_by_prd.get(paired_prd, set()) != {"requirement", "acceptance"}:
                fail(errors, "design-missing-prd-trace-fragment", path, f"add precise PRD requirement/acceptance relations when applicable: {paired_prd}")
            if not inactive_lifecycle:
                check_design_prd_mapping(root, head, path, text, errors, use_worktree_content, paired_prd)
    links = markdown_links(text)
    for number, line in authority_lines(lines):
        authority_targets = [target for line_number, _raw, target in links if line_number == number]
        if not authority_targets:
            fail(errors, "authority-not-link", path, f"line {number} must use a Markdown path target")
        for target in authority_targets:
            target_path, _fragment = split_link_target(target)
            if "://" in target_path or target_path.startswith(("mailto:", "//")):
                fail(errors, "authority-external-link", path, f"line {number} must resolve inside the repository: {target}")
                continue
            validate_link(
                root,
                head,
                source,
                target,
                text,
                errors,
                f"{path}:{number}",
                "authority-link",
                use_worktree_content,
            )
    for number, _raw, target in links:
        validate_link(
            root,
            head,
            source,
            target,
            text,
            errors,
            f"{path}:{number}",
            "markdown-link",
            use_worktree_content,
        )
    check_requirements(path, text, errors)


def collect_documents(root: Path, base: str, head: str, worktree: bool) -> tuple[list[ChangedDocument], str | None]:
    changed, worktree_paths = diff_paths(root, base, head, worktree)
    source_base = run_git(root, "merge-base", base, head).strip()
    if not source_base:
        raise ValueError(f"base/head have no merge-base: {base} {head}")
    documents: list[ChangedDocument] = []
    for path, status in sorted(changed.items()):
        if not (is_product_doc(path) or is_product_root(path)) or status.startswith("D"):
            continue
        old_text = git_text(root, source_base, path)
        new_text = current_text(root, head, path, path in worktree_paths)
        if new_text is None:
            raise OSError(f"selected document cannot be read: {path}")
        if status.startswith(("A", "R", "C", "??")) or normalized_for_change(old_text) != normalized_for_change(new_text):
            documents.append(ChangedDocument(path, status, old_text, new_text))
    if not documents:
        return [], "no new or substantive product-document changes in selected range"
    return documents, None


def collect_full_corpus(root: Path) -> tuple[list[ChangedDocument], list[str]]:
    """Select regular current-tree product PRD/design files in stable path order."""
    documents: list[ChangedDocument] = []
    errors: list[str] = []
    product_root = require_safe_worktree_path(root, PRODUCT_ROOT.as_posix())
    try:
        product_root.stat()
    except FileNotFoundError:
        return documents, errors
    if not product_root.is_dir():
        raise ValueError(f"product-document root is not a directory: {PRODUCT_ROOT}")
    paths = []
    def walk_error(error: OSError) -> None:
        raise error
    for directory, subdirectories, filenames in os.walk(product_root, followlinks=False, onerror=walk_error):
        for name in subdirectories:
            child = Path(directory) / name
            if child.is_symlink():
                errors.append(f"product-doc-content: symlink-not-allowed: {child.relative_to(root).as_posix()}: product-document directory must be real")
        paths.extend(Path(directory) / name for name in filenames)
    for path in sorted(paths):
        relative = path.relative_to(root).as_posix()
        if not is_product_doc(relative) and not is_product_root(relative):
            continue
        if path.is_symlink():
            errors.append(
                f"product-doc-content: symlink-not-allowed: {relative}: "
                "product-document corpus entries must be regular files"
            )
            continue
        if not path.is_file():
            continue
        documents.append(
            ChangedDocument(
                path=relative,
                status="full-corpus",
                old_text=None,
                new_text=path.read_text(encoding="utf-8"),
            )
        )
    return documents, errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--base")
    parser.add_argument("--head")
    parser.add_argument("--worktree", action="store_true")
    parser.add_argument("--full-corpus", action="store_true")
    parser.add_argument("--warn-only", action="store_true", help="Advisory historical inventory; changed-document integrity errors remain blocking")
    args = parser.parse_args()
    root = args.repo_root.resolve()
    if args.full_corpus and (args.base is not None or args.head is not None or args.worktree):
        parser.error("--full-corpus cannot be combined with --base, --head, or --worktree")
    if args.full_corpus:
        try:
            head = run_git(root, "rev-parse", "--verify", "HEAD^{commit}").strip()
            documents, corpus_errors = collect_full_corpus(root)
        except (OSError, ValueError) as exc:
            print(f"product-doc-content: error: {exc}")
            return 2
        if corpus_errors:
            print("\n".join(corpus_errors))
            return 1
        if not documents:
            print("product-doc-content: checked 0: reason=no current-tree product PRD/design documents")
            return 0
        errors: list[str] = []
        for document in documents:
            check_document(root, head, document.path, document.new_text, errors, True, full_corpus=True)
        if errors:
            return report_findings(errors, True) if args.warn_only else report_classified_findings(errors, ADVISORY_CODES)
        print(f"product-doc-content: OK (full-corpus checked {len(documents)} current-tree product documents)")
        return 0
    if args.worktree and not (args.base and args.head):
        print("product-doc-content: error: --worktree requires explicit --base and --head commit OIDs")
        return 2
    try:
        base = resolve_commit(root, args.base, "base")
        head = resolve_commit(root, args.head, "head")
        documents, reason = collect_documents(root, base, head, args.worktree)
    except ValueError as exc:
        print(f"product-doc-content: error: {exc}")
        return 2
    if not documents:
        print(f"product-doc-content: checked 0: reason={reason}")
        return 0
    errors: list[str] = []
    for document in documents:
        check_document(root, head, document.path, document.new_text, errors, args.worktree)
    if errors:
        return report_classified_findings(errors, ADVISORY_CODES)
    print(f"product-doc-content: OK (checked {len(documents)} changed/new documents)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
