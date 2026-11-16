#!/usr/bin/env python3
"""Validate the closed four-module product documentation overlay."""

from __future__ import annotations

import argparse
from doc_content_warnings import report_findings, report_classified_findings
from dataclasses import dataclass
import html
from pathlib import Path
import re
import sys
from urllib.parse import unquote

try:
    from product_doc_markdown import parse_markdown_blocks, parse_markdown_html, parse_markdown_links
except RuntimeError as exc:
    print(f"product-doc-governance: error: {exc}", file=sys.stderr)
    raise SystemExit(2) from exc


@dataclass(frozen=True)
class ProductModule:
    slug: str
    prd_id: str
    authorities: tuple[str, ...]
    accepted_names: tuple[str, ...]

    @property
    def path(self) -> str:
        return f"doc/product/{self.slug}/prd.md"


MODULES = (
    # C1: active landing/root identities use only the final display label.
    # Historical aliases remain valid as ordinary prose/provenance because the
    # checker only inspects canonical identity declarations and entry rows.
    ProductModule(
        "world-rules-core-gameplay",
        "PRD-PRODUCT-001",
        ("doc/game/prd.md", "doc/world-runtime/prd.md", "doc/world-simulator/prd.md", "doc/p2p/prd.md"),
        ("世界规则与玩法系统",),
    ),
    ProductModule(
        "world-infrastructure",
        "PRD-PRODUCT-002",
        ("doc/game/prd.md", "doc/world-runtime/prd.md", "doc/p2p/prd.md"),
        ("权威世界基础设施",),
    ),
    ProductModule(
        "agents-world-simulation",
        "PRD-PRODUCT-003",
        ("doc/world-simulator/prd.md",),
        ("智能体、世界模拟与交互",),
    ),
    ProductModule(
        "player-entry-distribution",
        "PRD-PRODUCT-004",
        ("README.md", "doc/world-simulator/prd.md"),
        ("玩家接入与发行",),
    ),
)
LIFECYCLES = {"proposed", "draft", "active", "superseded", "retired"}
RESERVED_PRODUCT_IDENTITY_METADATA = (
    "产品模块",
    "产品模块 slug",
    "产品层唯一 PRD",
    "产品模块总入口",
    "Product PRD-ID",
)
PRODUCT_IDENTITY_EXAMPLE_PATHS = frozenset(
    {"doc/engineering/doc-governance/doc-structure-standard.design.md"}
)
CANONICAL_MODULE_ROOT_PATHS = frozenset(module.path for module in MODULES)
TOPIC_PROFESSIONAL_AUTHORITIES = {
    "doc/product/agents-world-simulation/provider-agent-experience-continuity.prd.md": (
        "doc/world-simulator/llm/provider-agent-experience-parity.prd.md",
    ),
}
REQUIRED_HEADINGS = (
    "## 文档身份",
    "## 1. 产品承诺",
    "## 2. 范围",
    "## 3. 权威与冲突处理",
    "## 4. 路线图",
    "## 5. Done：成功标准与验收",
    "### 5.1 验收追踪",
    "## 6. Non-Goals",
)
HTML_COMMENT_RE = re.compile(r"<!--[\s\S]*?-->")
ANCHOR_RE = re.compile(r"<a\s+[^>]*\bid\s*=\s*[\"']([^\"']+)[\"'][^>]*>", re.IGNORECASE)
HTML_TAG_RE = re.compile(r"<[^>]+>")


def without_html_comments(text: str) -> str:
    """Match product-content's fragment parser while preserving line numbers."""
    def preserve_newlines(match: re.Match[str]) -> str:
        return "".join(character for character in match.group(0) if character in "\r\n")

    return HTML_COMMENT_RE.sub(preserve_newlines, text)


def visible_lines(text: str) -> list[tuple[int, str]]:
    """Return line-numbered prose, excluding comments and Markdown code blocks."""
    text = without_html_comments(text)
    excluded_lines = {
        number
        for block in parse_markdown_blocks(text)
        for number in range(block.start_line, block.end_line + 1)
    }
    return [
        (number, line)
        for number, line in enumerate(text.splitlines(), start=1)
        if number not in excluded_lines
    ]


def actual_anchor_occurrences(text: str) -> list[tuple[str, int]]:
    """Return parser-recognized HTML anchors, excluding comments."""
    occurrences: list[tuple[str, int]] = []
    for node in parse_markdown_html(without_html_comments(text)):
        for match in ANCHOR_RE.finditer(node.content):
            line = node.line + node.content[: match.start()].count("\n")
            occurrences.append((match.group(1), line))
    return occurrences


def split_link_target(raw: str) -> tuple[str, str | None]:
    """Split a CommonMark target using the product-content checker contract."""
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
    """Apply the same explicit-anchor and GitHub-heading semantics as content checks."""
    return fragment.strip().lower() in fragment_ids(text)


def fragment_ids(text: str) -> set[str]:
    """Build the target's navigable fragment set once per product document."""
    fragments = {
        anchor.strip().lower()
        for anchor, _number in actual_anchor_occurrences(text)
    }
    visible = "\n".join(line for _, line in visible_lines(text))
    for line in visible.splitlines():
        match = re.match(r"^ {0,3}#{1,6}\s+(.+?)\s*#*\s*$", line)
        if match:
            fragments.add(github_heading_slug(match.group(1)))
    return fragments


def validate_product_fragments(root: Path, errors: list[str]) -> None:
    """Validate local fragments across product docs, without scanning other doc trees."""
    product_root = root / "doc/product"
    fragments_by_path: dict[Path, set[str]] = {}
    for source in sorted(product_root.glob("**/*.md")):
        if not source.is_file():
            continue
        source_relative = source.relative_to(root).as_posix()
        source_text = source.read_text(encoding="utf-8")
        for link in parse_markdown_links(source_text):
            target, fragment = split_link_target(link.target)
            if not fragment or is_external_link_target(target):
                continue
            target_path = source if not target else (source.parent / target).resolve()
            try:
                target_relative = target_path.relative_to(root)
            except ValueError:
                continue
            # Structural inventory checks report missing paths; this pass only
            # adds the fragment contract when the local target is readable.
            if not target_path.is_file():
                continue
            if target_path not in fragments_by_path:
                fragments_by_path[target_path] = fragment_ids(
                    target_path.read_text(encoding="utf-8")
                )
            if fragment.strip().lower() not in fragments_by_path[target_path]:
                fail(
                    errors,
                    "invalid-fragment",
                    f"{source_relative}:{link.line}: {target_relative.as_posix()}#{fragment}",
                )


def metadata(text: str, label: str) -> str | None:
    match = re.search(rf"^- {re.escape(label)}：(?:`([^`]+)`|(.+))$", text, re.MULTILINE)
    if not match:
        return None
    return (match.group(1) or match.group(2)).strip()


def reserved_product_identity_metadata_lines(text: str) -> list[tuple[int, str]]:
    """Return exact reserved metadata declarations, never semantic references."""
    labels = "|".join(re.escape(label) for label in RESERVED_PRODUCT_IDENTITY_METADATA)
    pattern = re.compile(rf"^- ({labels})：", re.MULTILINE)
    declarations: list[tuple[int, str]] = []
    fence: tuple[str, int] | None = None
    for line_number, line in enumerate(text.splitlines(), start=1):
        if fence:
            character, length = fence
            if re.fullmatch(rf" {{0,3}}{re.escape(character)}{{{length},}}[ \t]*", line):
                fence = None
            continue
        opener = re.match(r" {0,3}([`~])\1{2,}", line)
        if opener:
            fence = (opener.group(1), len(opener.group(0).lstrip()))
            continue
        match = pattern.match(line)
        if match:
            declarations.append((line_number, match.group(1)))
    return declarations


def fail(errors: list[str], code: str, detail: str) -> None:
    errors.append(f"product-doc-governance: {code}: {detail}")


def markdown_targets(root: Path, source: Path, text: str) -> set[str]:
    """Resolve repository targets from actual CommonMark link nodes."""
    targets: set[str] = set()
    for link in parse_markdown_links(text):
        target = link.target.split("#", 1)[0].strip()
        if not target or "://" in target:
            continue
        resolved = (source.parent / target).resolve()
        try:
            targets.add(resolved.relative_to(root).as_posix())
        except ValueError:
            continue
    return targets


def markdown_targets_outside_fenced_code(root: Path, source: Path, text: str) -> set[str]:
    """Compatibility wrapper for the prose-only Markdown target scanner."""
    return markdown_targets(root, source, text)


def declared_pair_targets(root: Path, source: Path, text: str, label: str) -> tuple[bool, set[str]]:
    lines = [line for _number, line in visible_lines(text)
             if re.match(rf"^\s*(?:[-+*]\s*)?{label}\s*[:：]", line)]
    return bool(lines), markdown_targets(root, source, "\n".join(lines))


def topic_targets_for_section(
    root: Path, module_path: Path, text: str, heading: str
) -> set[str]:
    """Return topic PRDs declared by one dedicated module inventory section."""
    section = re.search(
        rf"^### {re.escape(heading)}\s*$([\s\S]*?)(?=^#{{1,3}}\s|\Z)",
        text,
        re.MULTILINE,
    )
    if not section:
        return set()
    return {
        target
        for target in markdown_targets(root, module_path, section.group(1))
        if target.endswith(".prd.md") and target != module_path.relative_to(root).as_posix()
    }


def active_topic_targets(root: Path, module_path: Path, text: str) -> set[str]:
    active, _migration = classified_topic_targets(root, module_path, text)
    return active


def migration_topic_targets(root: Path, module_path: Path, text: str) -> set[str]:
    _active, migration = classified_topic_targets(root, module_path, text)
    return migration


def classified_topic_targets(root: Path, module_path: Path, text: str) -> tuple[set[str], set[str]]:
    """Keep explicit legacy declarations and accept equivalent navigation prose."""
    active = topic_targets_for_section(root, module_path, text, "活跃产品专题")
    migration = topic_targets_for_section(root, module_path, text, "非权威迁移索引")
    linked_topics = {target for target in markdown_targets(root, module_path, text)
                     if target.endswith(".prd.md") and (root / target).parent == module_path.parent}
    for target in linked_topics - active - migration:
        path = root / target
        lifecycle = metadata(path.read_text(encoding="utf-8"), "生命周期") if path.is_file() else None
        (migration if lifecycle in {"superseded", "retired"} else active).add(target)
    return active, migration


def check(root: Path) -> list[str]:
    errors: list[str] = []
    retired_ledgers = sorted(
        path.relative_to(root).as_posix()
        for path in root.joinpath("doc/product").glob("**/*.project.md")
        if path.is_file()
    )
    for retired_ledger in retired_ledgers:
        fail(errors, "retired-project-ledger", retired_ledger)
    for path in sorted(root.joinpath("doc").glob("**/*.md")):
        relative_path = path.relative_to(root).as_posix()
        if (
            relative_path in CANONICAL_MODULE_ROOT_PATHS
            or relative_path in PRODUCT_IDENTITY_EXAMPLE_PATHS
        ):
            continue
        for line, label in reserved_product_identity_metadata_lines(path.read_text(encoding="utf-8")):
            fail(
                errors,
                "identity-metadata-location",
                f"{relative_path}:{line}: {label} is reserved for canonical product module roots",
            )
    landing_path = root / "doc/product/README.md"
    if not landing_path.is_file():
        return ["product-doc-governance: entry-missing: doc/product/README.md"]
    landing = landing_path.read_text(encoding="utf-8")
    row_re = re.compile(r"^\|\s*([^|]+?)\s*\|\s*\[[^]]+\]\(([^)]+)\)\s*\|", re.MULTILINE)
    rows = [(name.strip(), target.strip()) for name, target in row_re.findall(landing)]
    if len(rows) != len(MODULES):
        fail(errors, "entry-contract", f"expected exact four-row manifest, got {rows!r}")
    for index, module in enumerate(MODULES):
        if index >= len(rows):
            break
        actual_name, actual_target = rows[index]
        expected_target = f"{module.slug}/prd.md"
        if actual_name not in module.accepted_names or actual_target != expected_target:
            fail(
                errors,
                "entry-contract",
                f"row {index + 1} expected name in {module.accepted_names!r} and target {expected_target!r}, "
                f"got {(actual_name, actual_target)!r}",
            )
    if len({target for _, target in rows}) != len(rows):
        fail(errors, "entry-duplicate", "product entry targets must be unique")

    expected_paths = {module.path for module in MODULES}
    product_root = root / "doc/product"
    actual_paths = {
        path.relative_to(root).as_posix()
        for pattern in ("**/prd.md", "**/*.prd.md")
        for path in product_root.glob(pattern)
        if path.is_file()
    }
    declared_topics: set[str] = set()
    declared_migration_topics: set[str] = set()

    seen_ids: set[str] = set()
    for module in MODULES:
        path = root / module.path
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        actual_name = metadata(text, "产品模块")
        if actual_name not in module.accepted_names:
            fail(
                errors,
                "metadata-contract",
                f"{module.path}: 产品模块 expected one of {module.accepted_names!r}, got {actual_name!r}",
            )
        expected_metadata = {
            "产品模块 slug": module.slug,
            "产品层唯一 PRD": module.path,
            "产品模块总入口": "doc/product/README.md",
            "Product PRD-ID": module.prd_id,
            "生命周期": "active",
            "Owner role": "producer_system_designer",
        }
        for label, expected in expected_metadata.items():
            actual = metadata(text, label)
            if actual != expected:
                fail(errors, "metadata-content" if label in {"生命周期", "Owner role"} else "metadata-contract", f"{module.path}: {label} expected {expected!r}, got {actual!r}")
        prd_id = metadata(text, "Product PRD-ID")
        if prd_id:
            if prd_id in seen_ids:
                fail(errors, "metadata-duplicate-id", prd_id)
            seen_ids.add(prd_id)
            if "xxx" in prd_id.lower() or not re.fullmatch(r"PRD-PRODUCT-\d{3}", prd_id):
                fail(errors, "metadata-placeholder-id", f"{module.path}: {prd_id}")
        lifecycle = metadata(text, "生命周期")
        if lifecycle is not None and lifecycle not in LIFECYCLES:
            fail(errors, "lifecycle-contract", f"{module.path}: {lifecycle!r}")
        if lifecycle == "active" and metadata(text, "后继文档") not in {None, "无"}:
            fail(errors, "lifecycle-successor", f"{module.path}: active PRD must use successor `无`")
        if not metadata(text, "Last reviewed"):
            fail(errors, "metadata-last-reviewed", module.path)
        for heading in REQUIRED_HEADINGS:
            if heading not in text:
                fail(errors, "section-contract", f"{module.path}: missing heading prefix {heading!r}")
        declared_match = re.search(r"^- 下层专业域：(.+)$", text, re.MULTILINE)
        declared_paths = tuple(
            target
            for target in module.authorities
            if target in markdown_targets(root, path, declared_match.group(1) if declared_match else "")
        )
        if declared_paths != module.authorities:
            fail(errors, "authority-contract", f"{module.path}: expected {module.authorities!r}, got {declared_paths!r}")
        for authority in module.authorities:
            authority_path = root / authority
            if not authority_path.is_file():
                fail(errors, "authority-missing", f"{module.path}: {authority}")
            elif module.path not in markdown_targets(root, authority_path, authority_path.read_text(encoding="utf-8")):
                fail(errors, "authority-backlink", f"{authority} must link {module.path}")

        topics = active_topic_targets(root, path, text)
        for topic in topics:
            topic_path = root / topic
            if topic_path.parent != path.parent:
                fail(errors, "topic-module-boundary", f"{module.path}: {topic}")
            if not topic_path.is_file():
                fail(errors, "topic-missing", f"{module.path}: declared active topic {topic} does not exist")
                continue
            topic_lifecycle = metadata(topic_path.read_text(encoding="utf-8"), "生命周期")
            if topic_lifecycle is not None and topic_lifecycle != "active":
                fail(errors, "topic-lifecycle", f"active topic must declare active lifecycle: {topic}")
            declared_topics.add(topic)

        migration_topics = migration_topic_targets(root, path, text)
        for topic in migration_topics:
            topic_path = root / topic
            if topic_path.parent != path.parent:
                fail(errors, "topic-module-boundary", f"{module.path}: {topic}")
            if not topic_path.is_file():
                fail(errors, "topic-missing", f"{module.path}: declared migration topic {topic} does not exist")
                continue
            lifecycle = metadata(topic_path.read_text(encoding="utf-8"), "生命周期")
            if lifecycle is not None and lifecycle not in {"superseded", "retired"}:
                fail(errors, "topic-lifecycle", f"migration topic must be superseded or retired: {topic}")
            declared_migration_topics.add(topic)

        visible = "\n".join(line for _, line in visible_lines(text))
        success_ids = re.findall(r"^\s*[-+*]\s+`?(SC-\d+)`?\s*[:：]", visible, re.MULTILINE)
        # A criterion can be introduced by a heading instead of a list item.
        # Repeated subrange headings/mentions are references to that criterion.
        heading_ids = re.findall(r"^\s*#{1,6}\s+`?(SC-\d+)`?\s*[:：]", visible, re.MULTILINE)
        listed_ids = set(success_ids)
        success_ids.extend(identifier for identifier in heading_ids if identifier not in listed_ids)
        trace_rows = re.findall(r"^\|\s*(SC-\d+)\s*\|\s*([^|]+)\|\s*([^|]+)\|\s*([^|]+)\|\s*([^|]+)\|\s*([^|]+)\|$", text, re.MULTILINE)
        trace_ids = [row[0] for row in trace_rows]
        if len(success_ids) != len(set(success_ids)) or len(trace_ids) != len(set(trace_ids)):
            fail(errors, "traceability-duplicate-id", f"{module.path}: success={success_ids!r}, trace={trace_ids!r}")
        if success_ids and set(trace_ids) - set(success_ids):
            fail(errors, "traceability-undeclared-id", f"{module.path}: undeclared success criteria {sorted(set(trace_ids) - set(success_ids))!r}")
        if not success_ids or success_ids != trace_ids:
            fail(errors, "traceability-contract", f"{module.path}: success={success_ids!r}, trace={trace_ids!r}")
        for sc_id, owner, prd_ids, authority_docs, evidence, tier in trace_rows:
            prd_refs = re.findall(r"PRD-[A-Z][A-Z0-9_]*(?:-[A-Z0-9]+)+(?:/[A-Z0-9]+)*", prd_ids)
            if not owner.strip() or not prd_refs:
                fail(errors, "traceability-owner-prd", f"{module.path}: {sc_id}")
            row_paths = re.findall(r"`([^`]+\.md)`", authority_docs)
            if not row_paths:
                fail(errors, "traceability-authority-format", f"{module.path}: {sc_id}")
            elif any(not (root / row_path).is_file() for row_path in row_paths):
                fail(errors, "traceability-authority", f"{module.path}: {sc_id}")
            else:
                authority_texts = [(root / row_path).read_text(encoding="utf-8") for row_path in row_paths]
                for prd_ref in prd_refs:
                    if not any(prd_ref in authority_text for authority_text in authority_texts):
                        fail(errors, "traceability-prd-resolution", f"{module.path}: {sc_id} missing {prd_ref} in {row_paths!r}")
            if not evidence.strip() or tier.strip() not in {"test_tier_required", "test_tier_full"}:
                fail(errors, "traceability-evidence-tier", f"{module.path}: {sc_id}")

    if declared_topics & declared_migration_topics:
        fail(errors, "topic-duplicate-declaration", "a topic cannot be both active and migration-only")
    expected_inventory = expected_paths | declared_topics | declared_migration_topics
    if actual_paths != expected_inventory:
        fail(
            errors,
            "topic-index-contract",
            f"expected roots plus declared topics {sorted(expected_inventory)!r}, got {sorted(actual_paths)!r}",
        )
    if len(declared_topics) != sum(
        len(active_topic_targets(root, root / module.path, (root / module.path).read_text(encoding="utf-8")))
        for module in MODULES
        if (root / module.path).is_file()
    ):
        fail(errors, "topic-duplicate-declaration", "an active product topic may have one module owner")

    for topic in declared_topics:
        topic_path = root / topic
        if not topic_path.is_file():
            continue
        topic_text = topic_path.read_text(encoding="utf-8")
        module_root = topic_path.parent.joinpath("prd.md").relative_to(root).as_posix()
        if module_root not in markdown_targets(root, topic_path, topic_text):
            fail(
                errors,
                "topic-module-backlink",
                f"{topic} must link its owning module PRD {module_root}",
            )
        _pair_declared, paired_targets = declared_pair_targets(root, topic_path, topic_text, r"配对产品设计")
        for paired_target in paired_targets:
            paired_path = root / paired_target
            if not paired_target.endswith(".design.md") or not paired_path.is_file():
                fail(errors, "topic-pair-target-missing", f"{topic}: declared product design is invalid: {paired_target}")
            elif topic not in markdown_targets_outside_fenced_code(root, paired_path, paired_path.read_text(encoding="utf-8")):
                fail(errors, "topic-pair-backlink", f"{paired_target} must link declared source {topic}")
        if metadata(topic_text, "产品层唯一 PRD") not in {None, module_root}:
            fail(errors, "topic-module-authority", f"{topic}: 产品层唯一 PRD must name its module root")
        for authority in TOPIC_PROFESSIONAL_AUTHORITIES.get(topic, ()):
            if authority not in markdown_targets(root, topic_path, topic_text):
                fail(errors, "topic-professional-authority", f"{topic}: missing link to {authority}")
            elif not (root / authority).is_file():
                fail(errors, "topic-professional-authority", f"{topic}: missing authority {authority}")

    paired_files = [
        path
        for suffix in ("*.design.md",)
        for path in (root / "doc/product").glob(f"**/{suffix}")
        if path.is_file()
    ]
    for paired_path in paired_files:
        paired_text = paired_path.read_text(encoding="utf-8")
        pair_declared, topics = declared_pair_targets(root, paired_path, paired_text, r"配对(?:产品)?\s*PRD")
        same_name = paired_path.with_name(paired_path.name.removesuffix(".design.md") + ".prd.md").relative_to(root).as_posix()
        if not topics:
            if same_name in markdown_targets(root, paired_path, paired_text):
                topics.add(same_name)
        if not topics:
            code = "topic-pair-backlink" if pair_declared else "topic-pair-format"
            fail(errors, code, f"{paired_path.relative_to(root)}: add a readable declared product PRD association when applicable")
        for topic in topics:
            if topic not in declared_topics | CANONICAL_MODULE_ROOT_PATHS:
                fail(errors, "topic-pair-orphan", f"{paired_path.relative_to(root)} has no declared {topic}")
    if any((root / "doc/product").glob("**/archive")):
        fail(errors, "lifecycle-archive", "doc/product/**/archive is forbidden")
    validate_product_fragments(root, errors)
    return errors


# Explicit allowlist: new findings fail closed until classified. Do not suppress
# read exceptions, missing files, root identity, or module topology contracts.
CONTENT_FINDING_CODES = frozenset({
    "metadata-content", "metadata-last-reviewed", "section-contract",
    "traceability-contract", "traceability-authority-format", "traceability-owner-prd", "traceability-evidence-tier",
    "topic-pair-format",
})
# Existing global identity/topology checks remain global. Relation findings
# formerly treated as content are strict only in their affected source/target
# range, so an unrelated edit does not inherit historical content debt.
SCOPED_INTEGRITY_CODES = frozenset({
    "invalid-fragment", "lifecycle-contract", "lifecycle-successor", "topic-lifecycle",
    "traceability-prd-resolution", "authority-backlink", "topic-pair-backlink",
    "topic-pair-target-missing",
})


def is_content_finding(error: str) -> bool:
    return error.split(": ", 2)[1] in CONTENT_FINDING_CODES


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--warn-only", action="store_true", help="Warn for content conventions; identity/topology/integrity errors still fail")
    parser.add_argument("--enforce-changed", action="store_true", help="Validate affected relation facts; writing conventions remain advisory")
    parser.add_argument("--base")
    parser.add_argument("--head")
    parser.add_argument("--worktree", action="store_true")
    args = parser.parse_args()
    if args.enforce_changed and args.warn_only:
        parser.error("--enforce-changed cannot be combined with --warn-only")
    if not args.enforce_changed and (args.base or args.head or args.worktree):
        parser.error("range arguments require --enforce-changed")
    changed = set()
    if args.enforce_changed:
        # Reuse the same Git range and normalization as product content checks.
        import importlib.util
        spec = importlib.util.spec_from_file_location("overlay_product_content", Path(__file__).with_name("product-doc-content-check.py"))
        content = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = content
        spec.loader.exec_module(content)
        root = args.repo_root.resolve()
        try:
            base = content.resolve_commit(root, args.base, "base")
            head = content.resolve_commit(root, args.head, "head")
            paths, worktree_paths = content.diff_paths(root, base, head, args.worktree, scope="doc")
            source_base = content.run_git(root, "merge-base", base, head).strip()
            for path, status in paths.items():
                if not path.endswith(".md"):
                    continue
                if status.startswith("D"):
                    changed.add(path)
                    continue
                old = content.git_text(root, source_base, path)
                new = content.current_text(root, head, path, path in worktree_paths)
                if new is None:
                    raise OSError(f"selected document cannot be read: {path}")
                if status.startswith(("A", "R", "C", "??")) or content.normalized_for_change(old) != content.normalized_for_change(new):
                    changed.add(path)
        except (ValueError, OSError) as exc:
            print(f"product-doc-governance: error: {exc}")
            return 2
    errors = check(args.repo_root.resolve())
    if errors:
        current, historical = [], []
        for error in errors:
            code = error.split(": ", 2)[1]
            if args.enforce_changed and code in SCOPED_INTEGRITY_CODES:
                references = set(re.findall(r"doc/[\w./-]+\.md", error))
                if references and not references.intersection(changed):
                    historical.append(error)
                    continue
            current.append(error)
        if historical:
            report_findings(historical, True)
        return report_classified_findings(current, CONTENT_FINDING_CODES)
    print("product-doc-governance: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
