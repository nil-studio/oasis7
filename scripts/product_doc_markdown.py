#!/usr/bin/env python3
"""Small shared Markdown link-node adapter for product-document gates.

The gates need actual CommonMark link nodes, rather than a second regular
expression approximation.  ``markdown-it-py`` is pinned in
``scripts/doc-governance-requirements.txt`` and uses its CommonMark preset;
GFM extensions are intentionally not required for this contract.
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import lru_cache

try:
    from markdown_it import MarkdownIt
except ModuleNotFoundError as exc:  # pragma: no cover - exercised by setup checks
    raise RuntimeError(
        "product-document Markdown parsing requires markdown-it-py; "
        "install scripts/doc-governance-requirements.txt"
    ) from exc


_MARKDOWN = MarkdownIt("commonmark")


@dataclass(frozen=True)
class MarkdownLink:
    """A genuine Markdown link node and its source block line."""

    line: int
    target: str


@dataclass(frozen=True)
class MarkdownBlock:
    """A parsed Markdown block and its inclusive one-based source lines."""

    kind: str
    start_line: int
    end_line: int


@dataclass(frozen=True)
class MarkdownHtml:
    """A genuine HTML node emitted by the CommonMark parser."""

    line: int
    content: str


@lru_cache(maxsize=128)
def _parse_markdown(text: str) -> tuple[tuple[MarkdownBlock, ...], tuple[MarkdownHtml, ...], tuple[MarkdownLink, ...]]:
    """Parse an exact content snapshot once and retain only immutable records.

    Tokens are parser-owned mutable objects, so they are never cached or exposed.
    Bounds apply to content snapshots, not paths: a changed file cannot reuse the
    previous content's nodes. Every public adapter consumes the same parse.
    """
    blocks: list[MarkdownBlock] = []
    nodes: list[MarkdownHtml] = []
    links: list[MarkdownLink] = []
    source_lines = text.splitlines()
    for token in _MARKDOWN.parse(text):
        if token.type in {"fence", "code_block"} and token.map:
            start, end = token.map
            blocks.append(MarkdownBlock(token.type, start + 1, end))
        if token.type == "html_block" and token.map:
            start, _end = token.map
            nodes.append(MarkdownHtml(line=start + 1, content=token.content))
            continue
        if token.type != "inline" or not token.children or not token.map:
            continue
        start, end = token.map
        search_line = start
        search_offset = 0
        for child in token.children:
            if child.type != "html_inline":
                continue
            line = start + 1
            for source_index in range(search_line, min(end, len(source_lines))):
                offset = search_offset if source_index == search_line else 0
                position = source_lines[source_index].find(child.content, offset)
                if position < 0:
                    continue
                line = source_index + 1
                search_line = source_index
                search_offset = position + len(child.content)
                break
            nodes.append(MarkdownHtml(line=line, content=child.content))
        # Link and HTML source searches have independent positions, preserving
        # the adapters' original source-map association for interleaved nodes.
        line = start + 1
        search_line = start
        search_offset = 0
        for child in token.children:
            if child.type != "link_open":
                continue
            attrs = dict(child.attrs or ())
            target = attrs.get("href")
            if target:
                for source_index in range(search_line, min(end, len(source_lines))):
                    offset = search_offset if source_index == search_line else 0
                    position = source_lines[source_index].find(target, offset)
                    if position < 0:
                        continue
                    line = source_index + 1
                    search_line = source_index
                    search_offset = position + len(target)
                    break
                links.append(MarkdownLink(line=line, target=target))
    return tuple(blocks), tuple(nodes), tuple(links)


@lru_cache(maxsize=64)
def parse_markdown_blocks(text: str) -> tuple[MarkdownBlock, ...]:
    """Return actual CommonMark fenced/indented code spans, including nesting."""
    return _parse_markdown(text)[0]


@lru_cache(maxsize=64)
def parse_markdown_html(text: str) -> tuple[MarkdownHtml, ...]:
    """Return CommonMark HTML nodes, excluding code spans and code blocks."""
    return _parse_markdown(text)[1]


@lru_cache(maxsize=64)
def parse_markdown_links(text: str) -> tuple[MarkdownLink, ...]:
    """Return clickable links and first source lines, excluding images/code."""
    return _parse_markdown(text)[2]
