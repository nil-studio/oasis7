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
from html.parser import HTMLParser
import re

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
def _parse_markdown(text: str) -> tuple[tuple[MarkdownBlock, ...], tuple[MarkdownHtml, ...], tuple[MarkdownLink, ...], str]:
    """Parse an exact content snapshot once and retain only immutable records.

    Tokens are parser-owned mutable objects, so they are never cached or exposed.
    Bounds apply to content snapshots, not paths: a changed file cannot reuse the
    previous content's nodes. Every public adapter consumes the same parse.
    """
    blocks: list[MarkdownBlock] = []
    nodes: list[MarkdownHtml] = []
    links: list[MarkdownLink] = []
    references: list[str] = []
    source_lines = text.splitlines()
    for token in _MARKDOWN.parse(text):
        if token.type in {"fence", "code_block"} and token.map:
            start, end = token.map
            blocks.append(MarkdownBlock(token.type, start + 1, end))
            references.append(token.content)
        if token.type == "html_block" and token.map:
            start, _end = token.map
            nodes.append(MarkdownHtml(line=start + 1, content=token.content))
            continue
        if token.type != "inline" or not token.children or not token.map:
            continue
        depth = 0
        for child in token.children:
            if child.type == "link_open":
                depth += 1
            elif child.type == "link_close":
                depth -= 1
            elif not depth and child.type in {"text", "code_inline"}:
                references.append(child.content)
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
    return tuple(blocks), tuple(nodes), tuple(links), "\n".join(references)


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


def parse_markdown_reference_text(text: str) -> str:
    """Text/code references outside links; their paths use repository notation."""
    return _parse_markdown(text)[3]


@lru_cache(maxsize=64)
def markdown_anchor_ids(text: str) -> frozenset[str]:
    """Return rendered heading and explicit HTML anchors, excluding code examples."""
    anchors: set[str] = set()

    class AnchorParser(HTMLParser):
        def handle_starttag(self, tag, attrs):
            for name, value in attrs:
                if value and (name == "id" or (tag == "a" and name == "name")):
                    anchors.add(value.casefold())

    parser = AnchorParser()
    for node in parse_markdown_html(text):
        parser.feed(node.content)
    headings: set[str] = set()
    tokens = _MARKDOWN.parse(text)
    for index, token in enumerate(tokens):
        if token.type != "heading_open":
            continue
        inline = tokens[index + 1]
        visible = "".join(child.content for child in (inline.children or ())
                          if child.type in {"text", "code_inline", "image"})
        slug = "".join(char for char in visible.strip().casefold()
                       if char.isalnum() or char in "_-" or char.isspace())
        slug = re.sub(r"\s", "-", slug)
        candidate, suffix = slug, 0
        while candidate in headings:
            suffix += 1
            candidate = f"{slug}-{suffix}"
        headings.add(candidate)
    return frozenset(anchors | headings)
