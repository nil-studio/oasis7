#!/usr/bin/env python3
"""Regression contract for the changed-scope system-design traceability gate.

The fixtures intentionally use small temporary repositories.  They exercise the
range selector separately from the document contract so target-only and
worktree-overlay behavior cannot be hidden by the repository's own corpus.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parent.parent
CHECKER = ROOT / "scripts/system-design-traceability-check.py"
DESIGN = "doc/system/sample.design.md"
PRODUCT = "doc/product/sample.prd.md"
TEST_SOURCE = "doc/tests/sample.test.py"
MANUAL_SOURCE = "doc/manual/sample.md"


PRODUCT_TEXT = """# Sample product requirement

<a id="req-sample"></a>
## REQ-SAMPLE-001

The product requirement has a stable fragment for the system design to consume.
"""


DESIGN_HEADER = """# Sample system design

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-SAMPLE-001](../product/sample.prd.md#req-sample) | 说明系统必须承接的结果与适用条件。 | [DES-SAMPLE-001](#des-sample-001) | `repository_health_engineer` | 不覆盖未声明的入口。 |

### DES-SAMPLE-001

系统设计条款说明边界、拒绝条件和可观察结果。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-SAMPLE-001](../product/sample.prd.md#req-sample) | [DES-SAMPLE-001](#des-sample-001) | 验证该设计义务的拒绝与成功边界。 | [sample test](../tests/sample.test.py) | GitHub task evidence | 不覆盖生产发布。 |
"""


DEMAND_ROW = "| [REQ-SAMPLE-001](../product/sample.prd.md#req-sample) | 说明系统必须承接的结果与适用条件。 | [DES-SAMPLE-001](#des-sample-001) | `repository_health_engineer` | 不覆盖未声明的入口。 |"
VALIDATION_ROW = "| [REQ-SAMPLE-001](../product/sample.prd.md#req-sample) | [DES-SAMPLE-001](#des-sample-001) | 验证该设计义务的拒绝与成功边界。 | [sample test](../tests/sample.test.py) | GitHub task evidence | 不覆盖生产发布。 |"


def load_checker():
    if not CHECKER.is_file():
        raise AssertionError(
            "system-design-traceability: checker missing; RED must be caused by the unimplemented checker"
        )
    spec = importlib.util.spec_from_file_location("system_design_traceability_check", CHECKER)
    if spec is None or spec.loader is None:
        raise AssertionError("system-design-traceability: checker has no import loader")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def run_git(root: Path, *args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        check=False,
        capture_output=True,
        text=True,
    )
    if check and result.returncode != 0:
        raise AssertionError(result.stderr or result.stdout)
    return result.stdout.strip()


def make_repo(*, design_text: str | None = DESIGN_HEADER, include_test: bool = True) -> tuple[Path, str, str]:
    root = Path(tempfile.mkdtemp(prefix="system-design-traceability-"))
    (root / "doc/product").mkdir(parents=True)
    (root / "doc/system").mkdir(parents=True)
    if include_test:
        (root / "doc/tests").mkdir(parents=True)
    (root / "doc/manual").mkdir(parents=True)
    (root / PRODUCT).write_text(PRODUCT_TEXT, encoding="utf-8")
    if design_text is not None:
        (root / DESIGN).write_text(design_text, encoding="utf-8")
    if include_test:
        (root / TEST_SOURCE).write_text("# test source\n", encoding="utf-8")
    (root / MANUAL_SOURCE).write_text("# manual case\n", encoding="utf-8")
    run_git(root, "init", "-q", "-b", "main")
    run_git(root, "config", "user.email", "test@example.invalid")
    run_git(root, "config", "user.name", "Traceability Test")
    run_git(root, "add", ".")
    run_git(root, "commit", "-qm", "fixture")
    base = run_git(root, "rev-parse", "HEAD")
    return root, base, base


def commit(root: Path, message: str) -> str:
    run_git(root, "add", ".")
    run_git(root, "commit", "-qm", message)
    return run_git(root, "rev-parse", "HEAD")


def assert_no_errors(module, path: Path) -> None:
    errors = module.check_system_design(path)
    assert errors == [], "unexpected traceability errors: " + repr(errors)


def assert_code(module, path: Path, code: str) -> None:
    errors = module.check_system_design(path)
    assert any(f"system-design-traceability: {code}:" in error for error in errors), errors
    assert all("repair" in error.lower() or "add " in error.lower() for error in errors), errors


def scenario_valid_new_design() -> None:
    module = load_checker()
    root, base, _head = make_repo(design_text=None)
    try:
        (root / DESIGN).write_text(DESIGN_HEADER, encoding="utf-8")
        head = commit(root, "add valid system design")
        paths = module.changed_system_design_paths(root, base, head, False)
        assert [path.relative_to(root).as_posix() for path in paths] == [DESIGN], paths
        assert_no_errors(module, root / DESIGN)
    finally:
        shutil.rmtree(root)


def scenario_committed_range_reads_trusted_head_content() -> None:
    root, base, _head = make_repo(design_text=None)
    try:
        run_git(root, "checkout", "-qb", "source")
        (root / DESIGN).write_text(DESIGN_HEADER, encoding="utf-8")
        head = commit(root, "add source design")
        run_git(root, "checkout", "-q", "main")
        result = subprocess.run(
            [
                sys.executable,
                str(CHECKER),
                "--repo-root",
                str(root),
                "--base",
                base,
                "--head",
                head,
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        output = result.stdout + result.stderr
        assert result.returncode == 0, output
        assert "checked 1 changed/new system designs" in output, output
    finally:
        shutil.rmtree(root)


def scenario_committed_target_symlink_does_not_redirect_selected_head() -> None:
    root, base, _head = make_repo()
    try:
        (root / "doc/product/alternate.prd.md").write_text("# alternate product\n", encoding="utf-8")
        (root / DESIGN).write_text(DESIGN_HEADER + "\nSubstantive committed design revision.\n", encoding="utf-8")
        head = commit(root, "revise design with alternate target")
        product = root / PRODUCT
        product.unlink()
        product.symlink_to("alternate.prd.md")
        result = subprocess.run(
            [
                sys.executable,
                str(CHECKER),
                "--repo-root",
                str(root),
                "--base",
                base,
                "--head",
                head,
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        output = result.stdout + result.stderr
        assert result.returncode == 0, output
        assert "checked 1 changed/new system designs" in output, output
    finally:
        shutil.rmtree(root)


def scenario_worktree_symlinks_are_contained() -> None:
    module = load_checker()
    root, base, _head = make_repo()
    outside = Path(tempfile.mkdtemp(prefix="system-design-traceability-outside-"))
    try:
        outside_target = outside / "alternate.prd.md"
        outside_target.write_text(PRODUCT_TEXT, encoding="utf-8")
        product = root / PRODUCT
        product.unlink()
        product.symlink_to(outside_target)
        (root / DESIGN).write_text(DESIGN_HEADER + "\nSubstantive worktree design revision.\n", encoding="utf-8")
        paths = module.changed_system_design_paths(root, base, base, True)
        assert [path.relative_to(root).as_posix() for path in paths] == [DESIGN], paths
        result = subprocess.run(
            [
                sys.executable,
                str(CHECKER),
                "--repo-root",
                str(root),
                "--base",
                base,
                "--head",
                base,
                "--worktree",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        output = result.stdout + result.stderr
        assert result.returncode == 1, output
        assert "reference escapes the repository" in output, output
    finally:
        shutil.rmtree(root)
        shutil.rmtree(outside)


def scenario_missing_demand_allocation() -> None:
    module = load_checker()
    root, _base, _head = make_repo(design_text=DESIGN_HEADER.replace("### 2.1 需求承接与相关角色", "### 2.1 需求承接与相关角色"))
    try:
        (root / DESIGN).write_text(DESIGN_HEADER.replace("### 2.1 需求承接与相关角色", "### 2.1 需求承接与相关角色"), encoding="utf-8")
        # Remove the complete allocation table while retaining the validation table.
        start = DESIGN_HEADER.index("### 2.1")
        end = DESIGN_HEADER.index("### DES-SAMPLE-001")
        (root / DESIGN).write_text(DESIGN_HEADER[:start] + DESIGN_HEADER[end:], encoding="utf-8")
        assert_code(module, root / DESIGN, "trace-upstream-missing")
    finally:
        shutil.rmtree(root)


def scenario_missing_validation_mapping() -> None:
    module = load_checker()
    root, _base, _head = make_repo()
    try:
        marker = "## 11. 验证设计与可追溯性"
        (root / DESIGN).write_text(DESIGN_HEADER[: DESIGN_HEADER.index(marker)] + marker + "\n", encoding="utf-8")
        assert_code(module, root / DESIGN, "trace-validation-missing")
    finally:
        shutil.rmtree(root)


def scenario_unresolved_upstream_fragment() -> None:
    module = load_checker()
    root, _base, _head = make_repo(
        design_text=DESIGN_HEADER.replace("#req-sample)", "#missing-upstream)")
    )
    try:
        assert_code(module, root / DESIGN, "trace-ref-unresolved")
    finally:
        shutil.rmtree(root)


def scenario_required_row_shape_and_cells() -> None:
    module = load_checker()
    cases = (
        (
            DESIGN_HEADER.replace(DEMAND_ROW, "| [REQ-SAMPLE-001](../product/sample.prd.md#req-sample) | obligation | [DES-SAMPLE-001](#des-sample-001) |"),
            "trace-demand-row-invalid",
        ),
        (
            DESIGN_HEADER.replace(DEMAND_ROW, DEMAND_ROW.replace("`repository_health_engineer`", "")),
            "trace-demand-row-invalid",
        ),
        (
            DESIGN_HEADER.replace(VALIDATION_ROW, "| [REQ-SAMPLE-001](../product/sample.prd.md#req-sample) | [DES-SAMPLE-001](#des-sample-001) | obligation | [sample test](../tests/sample.test.py) |"),
            "trace-validation-row-invalid",
        ),
        (
            DESIGN_HEADER.replace(VALIDATION_ROW, VALIDATION_ROW.replace("GitHub task evidence", "")),
            "trace-validation-row-invalid",
        ),
    )
    for text, code in cases:
        root, _base, _head = make_repo(design_text=text)
        try:
            assert_code(module, root / DESIGN, code)
        finally:
            shutil.rmtree(root)


def scenario_validation_source_must_be_test_or_manual() -> None:
    module = load_checker()
    for method in (
        "[self](#des-sample-001)",
        "[product](../product/sample.prd.md#req-sample)",
    ):
        root, _base, _head = make_repo(
            design_text=DESIGN_HEADER.replace("[sample test](../tests/sample.test.py)", method)
        )
        try:
            assert_code(module, root / DESIGN, "trace-validation-source-invalid")
        finally:
            shutil.rmtree(root)

    root, _base, _head = make_repo(
        design_text=DESIGN_HEADER.replace(
            "[sample test](../tests/sample.test.py)",
            "[manual case](../manual/sample.md)",
        )
    )
    try:
        assert_no_errors(module, root / DESIGN)
    finally:
        shutil.rmtree(root)


def scenario_anchor_and_heading_fragment_collision_is_ambiguous() -> None:
    module = load_checker()
    root, _base, _head = make_repo(
        design_text=DESIGN_HEADER.replace(
            "### DES-SAMPLE-001",
            '<a id="des-sample-001"></a>\n### DES-SAMPLE-001',
            1,
        )
    )
    try:
        assert_code(module, root / DESIGN, "trace-ref-ambiguous")
    finally:
        shutil.rmtree(root)


def scenario_shared_markdown_adapters_cache_immutable_results_by_exact_text() -> None:
    module = load_checker()
    adapter = sys.modules["product_doc_markdown"]
    text = """## Repeated Heading
## Repeated Heading
<a id="explicit-anchor"></a>

[target](doc.md#target-anchor)

```html
<a id="code-anchor"></a>
<a href="code.md#hidden">code link</a>
## Code Heading
```

<!--
<a id="comment-anchor"></a>
<a href="comment.md#hidden">comment link</a>
## Comment Heading
-->
"""
    parsers = (
        adapter.parse_markdown_blocks,
        adapter.parse_markdown_html,
        adapter.parse_markdown_links,
    )
    for parser in parsers:
        parser.cache_clear()
    adapter._parse_markdown.cache_clear()
    parse_calls = 0
    had_local_parse = "parse" in adapter._MARKDOWN.__dict__
    original_parse = adapter._MARKDOWN.parse

    def counted_parse(source, *args, **kwargs):
        nonlocal parse_calls
        parse_calls += 1
        return original_parse(source, *args, **kwargs)

    adapter._MARKDOWN.parse = counted_parse
    try:
        first = tuple(parser(text) for parser in parsers)
        assert parse_calls == 1, parse_calls
        assert tuple(parser(text) for parser in parsers) == first
        assert parse_calls == 1, parse_calls
        assert all(isinstance(result, tuple) for result in first)

        blocks, html_nodes, links = first
        assert blocks and all(item.kind in {"fence", "code_block"} for item in blocks)
        assert any("explicit-anchor" in item.content for item in html_nodes)
        assert any(item.target == "doc.md#target-anchor" for item in links)
        for item, attribute in ((blocks[0], "kind"), (html_nodes[0], "content"), (links[0], "target")):
            try:
                setattr(item, attribute, "mutated")
            except AttributeError:
                pass
            else:
                raise AssertionError("cached markdown records must be immutable")

        changed = text.replace('id="explicit-anchor"', 'id="changed-anchor"', 1)
        changed = changed.replace("doc.md#target-anchor", "doc.md#changed-anchor", 1)
        changed_results = tuple(parser(changed) for parser in parsers)
        assert parse_calls == 2, parse_calls
        assert changed_results[1] != html_nodes
        assert changed_results[2] != links
        assert tuple(parser(changed) for parser in parsers) == changed_results
        assert parse_calls == 2, parse_calls
        assert tuple(parser(text) for parser in parsers) == first
        assert parse_calls == 2, parse_calls
    finally:
        if had_local_parse:
            adapter._MARKDOWN.parse = original_parse
        else:
            del adapter._MARKDOWN.parse

    assert module.fragment_occurrences(text, "REPEATED-HEADING") == 2
    assert module.fragment_occurrences(text, "EXPLICIT-ANCHOR") == 1
    assert module.fragment_occurrences(text, "code-anchor") == 0
    assert module.fragment_occurrences(text, "code-heading") == 0
    assert module.fragment_occurrences(text, "comment-anchor") == 0
    assert module.fragment_occurrences(text, "comment-heading") == 0
    assert module.fragment_occurrences(text, "hidden") == 0
    # The same path may be read from BASE and the candidate in one process.
    # Cached fragments must follow bytes, including newly ambiguous anchors.
    assert isinstance(module.visible_lines(text), tuple)
    assert module.fragment_section(text, "explicit-anchor") is not None
    duplicate = text + '\n<a id="explicit-anchor"></a>\n'
    assert module.fragment_occurrences(duplicate, "explicit-anchor") == 2
    assert module.fragment_section(duplicate, "explicit-anchor") is None
    assert module.fragment_occurrences(text, "explicit-anchor") == 1
    assert module.fragment_section(text, "explicit-anchor") is not None
    changed = text.replace('id="explicit-anchor"', 'id="new-anchor"')
    assert module.fragment_occurrences(changed, "explicit-anchor") == 0
    assert module.fragment_occurrences(changed, "new-anchor") == 1


def scenario_fenced_code_trace_decoy_is_ignored() -> None:
    module = load_checker()
    decoy = """```markdown\n<a id=\"des-sample-001\"></a>\n### 2.1 需求承接与分配表\n| upstream | obligation | design | owner | excluded |\n| --- | --- | --- | --- | --- |\n| [bad](missing.md#bad) | bad | [bad](#des-sample-001) | bad | bad |\n```\n\n"""
    root, _base, _head = make_repo(design_text=decoy + DESIGN_HEADER)
    try:
        assert_no_errors(module, root / DESIGN)
    finally:
        shutil.rmtree(root)


def na_design(*, disposition: str) -> str:
    return DESIGN_HEADER.replace(
        "[sample test](../tests/sample.test.py)",
        disposition,
    )


def scenario_complete_na() -> None:
    module = load_checker()
    root, _base, _head = make_repo(
        design_text=na_design(
            disposition="N/A: reason=该条款不涉及运行时技术义务；scope=本次文档治理变更；owner_role=repository_health_engineer；evidence_ref=https://github.com/eng-cc/oasis7/issues/3671#issuecomment-5636906114；re-evaluate=下次改变跨组件行为时。"
        )
    )
    try:
        assert_no_errors(module, root / DESIGN)
    finally:
        shutil.rmtree(root)


def scenario_na_accepts_readable_repository_path_fragment() -> None:
    module = load_checker()
    root, _base, _head = make_repo(
        design_text=na_design(
            disposition="N/A: reason=该条款不涉及运行时技术义务；scope=本次文档治理变更；owner_role=repository_health_engineer；evidence_ref=doc/product/sample.prd.md#req-sample；re-evaluate=下次改变跨组件行为时。"
        )
    )
    try:
        assert_no_errors(module, root / DESIGN)
    finally:
        shutil.rmtree(root)


def scenario_na_rejects_unreadable_or_noncanonical_evidence_locator() -> None:
    module = load_checker()
    for locator in (
        "garbage",
        "../tests/sample.test.py",
        "../tests/missing.test.py#sample",
        "../../outside.md#sample",
        "https://github.com/example/oasis7/issues/3671#issuecomment-5636906114",
        "https://github.com/eng-cc/oasis7/issues/0#issuecomment-5636906114",
        "https://github.com/eng-cc/oasis7/issues/3671#comment-5636906114",
    ):
        root, _base, _head = make_repo(
            design_text=na_design(
                disposition=f"N/A: reason=范围外；scope=本次文档治理变更；owner_role=repository_health_engineer；evidence_ref={locator}；re-evaluate=下次改变跨组件行为时。"
            )
        )
        try:
            assert_code(module, root / DESIGN, "trace-na-incomplete")
        finally:
            shutil.rmtree(root)


def scenario_incomplete_na() -> None:
    module = load_checker()
    root, _base, _head = make_repo(
        design_text=na_design(disposition="N/A: reason=范围外。")
    )
    try:
        assert_code(module, root / DESIGN, "trace-na-incomplete")
    finally:
        shutil.rmtree(root)


def scenario_target_only_design_change_is_excluded() -> None:
    module = load_checker()
    root, common, _head = make_repo(design_text=None)
    try:
        run_git(root, "checkout", "-qb", "feature")
        feature = run_git(root, "rev-parse", "HEAD")
        run_git(root, "checkout", "-q", "main")
        (root / DESIGN).write_text(DESIGN_HEADER, encoding="utf-8")
        target = commit(root, "target-only design change")
        paths = module.changed_system_design_paths(root, target, feature, False)
        assert paths == [], paths
        assert common != target
    finally:
        shutil.rmtree(root)


def scenario_whitespace_and_comment_only_source_change_is_excluded() -> None:
    module = load_checker()
    root, base, _head = make_repo()
    try:
        (root / DESIGN).write_text("<!-- changed comment -->\n" + DESIGN_HEADER.replace("\n", "  \n"), encoding="utf-8")
        head = commit(root, "non-semantic design formatting")
        assert module.changed_system_design_paths(root, base, head, False) == []
    finally:
        shutil.rmtree(root)


def scenario_untracked_worktree_design_is_included() -> None:
    module = load_checker()
    root, base, _head = make_repo(design_text=None)
    try:
        (root / DESIGN).write_text(DESIGN_HEADER, encoding="utf-8")
        paths = module.changed_system_design_paths(root, base, base, True)
        assert [path.relative_to(root).as_posix() for path in paths] == [DESIGN], paths
    finally:
        shutil.rmtree(root)


def scenario_staged_worktree_design_is_included() -> None:
    module = load_checker()
    root, base, _head = make_repo(design_text=None)
    try:
        (root / DESIGN).write_text(DESIGN_HEADER, encoding="utf-8")
        run_git(root, "add", DESIGN)
        paths = module.changed_system_design_paths(root, base, base, True)
        assert [path.relative_to(root).as_posix() for path in paths] == [DESIGN], paths
    finally:
        shutil.rmtree(root)


def scenario_staged_tracked_worktree_design_uses_index_content() -> None:
    module = load_checker()
    root, base, _head = make_repo()
    try:
        staged_text = DESIGN_HEADER.replace(VALIDATION_ROW, "")
        (root / DESIGN).write_text(staged_text, encoding="utf-8")
        run_git(root, "add", DESIGN)
        run_git(root, "restore", "--source=HEAD", "--worktree", "--", DESIGN)
        paths = module.changed_system_design_paths(root, base, base, True)
        assert [path.relative_to(root).as_posix() for path in paths] == [DESIGN], paths
        result = subprocess.run(
            [
                sys.executable,
                str(CHECKER),
                "--repo-root",
                str(root),
                "--base",
                base,
                "--head",
                base,
                "--worktree",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        output = result.stdout + result.stderr
        assert result.returncode == 1, output
        assert "trace-validation-missing" in output, output
    finally:
        shutil.rmtree(root)


def scenario_unstaged_worktree_design_is_included() -> None:
    module = load_checker()
    root, base, _head = make_repo()
    try:
        (root / DESIGN).write_text(DESIGN_HEADER + "\nAdditional substantive behavior.\n", encoding="utf-8")
        paths = module.changed_system_design_paths(root, base, base, True)
        assert [path.relative_to(root).as_posix() for path in paths] == [DESIGN], paths
    finally:
        shutil.rmtree(root)


def scenario_duplicate_validation_rows_fail_cardinality() -> None:
    module = load_checker()
    root, _base, _head = make_repo(
        design_text=DESIGN_HEADER.replace(VALIDATION_ROW, VALIDATION_ROW + "\n" + VALIDATION_ROW)
    )
    try:
        assert_code(module, root / DESIGN, "trace-slot-cardinality")
    finally:
        shutil.rmtree(root)


def scenario_committed_validation_symlink_sources_fail_closed() -> None:
    for label in ("dangling", "external"):
        root, base, _head = make_repo()
        outside = Path(tempfile.mkdtemp(prefix="system-design-traceability-outside-"))
        try:
            source = root / TEST_SOURCE
            source.unlink()
            if label == "dangling":
                source.symlink_to("missing.test.py")
            else:
                outside_target = outside / "external.test.py"
                outside_target.write_text("# external source\n", encoding="utf-8")
                source.symlink_to(outside_target)
            (root / DESIGN).write_text(
                DESIGN_HEADER + f"\nCommitted {label} validation source revision.\n",
                encoding="utf-8",
            )
            head = commit(root, f"add committed {label} validation source")
            result = subprocess.run(
                [
                    sys.executable,
                    str(CHECKER),
                    "--repo-root",
                    str(root),
                    "--base",
                    base,
                    "--head",
                    head,
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            output = result.stdout + result.stderr
            assert result.returncode == 1, output
            assert "trace-validation-source-invalid" in output, output
        finally:
            shutil.rmtree(root)
            shutil.rmtree(outside)


def scenario_untouched_legacy_design_is_excluded() -> None:
    module = load_checker()
    root, base, _head = make_repo(design_text="# legacy design without current trace contract\n")
    try:
        (root / "README.md").write_text("unrelated change\n", encoding="utf-8")
        head = commit(root, "unrelated change")
        assert module.changed_system_design_paths(root, base, head, False) == []
    finally:
        shutil.rmtree(root)


def run_checker(root: Path, base: str, head: str, *, worktree: bool = False) -> tuple[int, str]:
    command = [
            sys.executable,
            str(CHECKER),
            "--repo-root",
            str(root),
            "--base",
            base,
            "--head",
            head,
        ]
    if worktree:
        command.append("--worktree")
    result = subprocess.run(
        command,
        check=False,
        capture_output=True,
        text=True,
    )
    return result.returncode, result.stdout + result.stderr


def scenario_authority_only_prompt_control_change_rechecks_current_consumer() -> None:
    root, base, _head = make_repo()
    try:
        (root / PRODUCT).write_text(PRODUCT_TEXT.replace("req-sample", "req-renamed"), encoding="utf-8")
        head = commit(root, "change prompt control authority anchor")
        code, output = run_checker(root, base, head)
        assert code == 1, output
        assert "trace-ref-unresolved" in output, output
    finally:
        shutil.rmtree(root)


def scenario_authority_only_root_prd_prompt_control_change_rechecks_consumer() -> None:
    root, _initial_base, _head = make_repo()
    try:
        authority = root / "doc/world-simulator/prd.md"
        authority.parent.mkdir(parents=True)
        authority.write_text(PRODUCT_TEXT, encoding="utf-8")
        (root / DESIGN).write_text(
            DESIGN_HEADER.replace("../product/sample.prd.md", "../world-simulator/prd.md"),
            encoding="utf-8",
        )
        fixture_base = commit(root, "add root professional PromptControl authority")
        authority.write_text(PRODUCT_TEXT.replace("req-sample", "req-renamed"), encoding="utf-8")
        head = commit(root, "change root PromptControl authority anchor")
        code, output = run_checker(root, fixture_base, head)
        assert code == 1, output
        assert "trace-ref-unresolved" in output, output
    finally:
        shutil.rmtree(root)


def scenario_authority_rename_and_delete_fail_closed() -> None:
    for action in ("rename", "delete"):
        root, base, _head = make_repo()
        try:
            if action == "rename":
                run_git(root, "mv", PRODUCT, "doc/product/renamed.prd.md")
            else:
                (root / PRODUCT).unlink()
            head = commit(root, f"{action} authority endpoint")
            code, output = run_checker(root, base, head)
            assert code == 1, output
            assert "reference target is missing" in output, output
        finally:
            shutil.rmtree(root)


def scenario_worktree_authority_only_change_rechecks_current_consumer() -> None:
    root, base, _head = make_repo()
    try:
        (root / PRODUCT).write_text(PRODUCT_TEXT.replace("req-sample", "req-renamed"), encoding="utf-8")
        code, output = run_checker(root, base, base, worktree=True)
        assert code == 1, output
        assert "trace-ref-unresolved" in output, output
    finally:
        shutil.rmtree(root)


def scenario_untouched_legacy_consumer_is_excluded_from_authority_change() -> None:
    legacy = "# legacy design\n\n[old requirement](../product/sample.prd.md#req-sample)\n"
    root, base, _head = make_repo(design_text=legacy)
    try:
        (root / PRODUCT).write_text(PRODUCT_TEXT.replace("req-sample", "req-renamed"), encoding="utf-8")
        head = commit(root, "change authority with legacy consumer")
        code, output = run_checker(root, base, head)
        assert code == 0, output
        assert "checked 0" in output, output
    finally:
        shutil.rmtree(root)


def scenario_partial_or_malformed_range_is_rejected() -> None:
    module = load_checker()
    root, base, _head = make_repo(design_text=None)
    try:
        for bad_base, bad_head in (("", base), (base, ""), ("HEAD", base), (base, "not-an-oid")):
            try:
                module.changed_system_design_paths(root, bad_base, bad_head, False)
            except ValueError as error:
                assert "commit OID" in str(error) or "invalid" in str(error), error
            else:
                raise AssertionError((bad_base, bad_head))
    finally:
        shutil.rmtree(root)


def scenario_tracking_metadata_does_not_hide_design_changes() -> None:
    module = load_checker()
    old = "# Legacy design\n\n- 当前任务与执行证据: GitHub Issue（`Task UID` + evidence comments）\n\n## Behavior\nKeep durable state.\n"
    new = old.replace("- 当前任务与执行证据: GitHub Issue（`Task UID` + evidence comments）", "- 执行记录: Git、PR、实际 CI 和评审记录；Issue 与 Project 按需使用")
    root, base, _ = make_repo(design_text=old)
    try:
        (root / DESIGN).write_text(new, encoding="utf-8")
        head = commit(root, "update optional tracking metadata")
        assert module.changed_system_design_paths(root, base, head, False) == [], "metadata requires no design migration"
        (root / DESIGN).write_text(new.replace("Keep durable state.", "Discard durable state."), encoding="utf-8")
        head = commit(root, "change real persistence behavior")
        assert (root / DESIGN) in module.changed_system_design_paths(root, base, head, False), "behavior must still be inspected"
    finally:
        shutil.rmtree(root)


def relation_run(root: Path, *args: str):
    return subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root), *args], capture_output=True, text=True)


def scenario_product_fragment_must_identify_feature() -> None:
    module = load_checker()
    root, _, _ = make_repo()
    try:
        (root / PRODUCT).write_text('# Product\n<a id="req-sample"></a>\n## Overview\n\n### REQ-OTHER-001\nUnrelated nested feature.\n')
        errors = module.check_system_design(root / DESIGN)
        assert any('trace-upstream-feature-invalid' in error or 'trace-upstream-label-mismatch' in error for error in errors), errors
        (root / PRODUCT).write_text(PRODUCT_TEXT)
        assert_no_errors(module, root / DESIGN)
        (root / 'doc/product/README.md').write_text(PRODUCT_TEXT)
        (root / DESIGN).write_text(DESIGN_HEADER.replace('sample.prd.md', 'README.md'))
        errors = module.check_system_design(root / DESIGN)
        assert any('trace-upstream-authority-invalid' in error for error in errors), errors
    finally:
        shutil.rmtree(root)


def scenario_trace_refs_require_addressed_declaration() -> None:
    module = load_checker()
    identifier = 'O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6'
    other = 'O7-REQ-7b97fe34-2be1-4b6c-9e8a-54fb03b52c16'
    root, _, _ = make_repo()
    try:
        (root / DESIGN).write_text(DESIGN_HEADER.replace('说明系统必须承接的结果与适用条件。', 'Trace refs: ' + identifier + '；说明系统必须承接的结果与适用条件。'))
        errors = module.check_system_design(root / DESIGN)
        assert any('trace-id-unresolved' in e for e in errors), errors
        (root / PRODUCT).write_text(PRODUCT_TEXT.replace('## REQ-SAMPLE-001', '## REQ-SAMPLE-001\n\nTrace ID: ' + identifier))
        assert_no_errors(module, root / DESIGN)
        valid_design = (root / DESIGN).read_text()
        for malformed in ('O7-REQ-not-a-valid-id', identifier + 'extra', 'garbage', ''):
            (root / DESIGN).write_text(valid_design.replace(identifier, malformed))
            assert any('trace-id-invalid' in e for e in module.check_system_design(root / DESIGN))
        (root / DESIGN).write_text(valid_design)
        (root / PRODUCT).write_text(PRODUCT_TEXT.replace('## REQ-SAMPLE-001', '## REQ-SAMPLE-001\n\nTrace ID: ' + other) + '\n## Unrelated\nTrace ID: ' + identifier)
        errors = module.check_system_design(root / DESIGN)
        assert any('trace-row-id-mismatch' in e for e in errors), errors
    finally:
        shutil.rmtree(root)


def scenario_engineering_only_needs_bounded_na() -> None:
    module = load_checker()
    root, _, _ = make_repo()
    try:
        (root / 'doc/system/rules.prd.md').write_text('# Rules\n## Build invariant\nOnly deterministic builds.\n')
        text = DESIGN_HEADER.replace('../product/sample.prd.md#req-sample', 'rules.prd.md#build-invariant').replace('[REQ-SAMPLE-001]', '[build invariant]')
        (root / DESIGN).write_text(text)
        assert any('trace-product-na-incomplete' in e for e in module.check_system_design(root / DESIGN))
        for disposition, valid in [('产品适用性：N/A；reason=只维护构建检查；scope=构建输入', True), ('产品适用性：N/A；reason=无；scope=待补', False)]:
            (root / DESIGN).write_text(text.replace('系统设计条款说明边界、拒绝条件和可观察结果。', disposition))
            errors = module.check_system_design(root / DESIGN)
            assert (not errors) == valid, errors
        (root / 'doc/system/rules.manual.md').write_text('# Rules\n## Build invariant\nOnly deterministic builds.\n')
        (root / DESIGN).write_text(text.replace('rules.prd.md', 'rules.manual.md').replace('系统设计条款说明边界、拒绝条件和可观察结果。', '产品适用性：N/A；reason=仅维护构建检查；scope=构建输入'))
        assert_no_errors(module, root / DESIGN)
    finally:
        shutil.rmtree(root)


def scenario_feature_boundaries_and_explicit_label_mismatch() -> None:
    module = load_checker()
    text = '# Product\n<a id="one"></a>\n## REQ-ONE-1\nFirst.\n\n<a id="two"></a>\n## REQ-TWO-1\nSecond.'
    section = module.fragment_section(text, 'one')
    assert section and 'id="two"' not in section
    assert module.requirement_identity(text, 'one') == {'REQ-ONE-1'}
    assert module.trace_reference_ids('Use `Trace refs: <complete ID>` when applicable.') == (set(), set())
    assert module.trace_reference_ids('Trace refs: garbage')[1] == {'garbage'}
    root, _, _ = make_repo(design_text=DESIGN_HEADER.replace('[REQ-SAMPLE-001]', '[REQ-NONEXISTENT-001]'))
    try:
        assert any('trace-upstream-label-mismatch' in e for e in module.check_system_design(root / DESIGN))
    finally:
        shutil.rmtree(root)


def scenario_validation_na_rejects_placeholders() -> None:
    module = load_checker()
    assert not module.na_is_complete('N/A: reason=无; scope=无; owner_role=无; evidence_ref=https://github.com/eng-cc/oasis7/issues/1#issuecomment-1; re-evaluate=无')


def scenario_root_scoped_and_contract_downgrade_fail() -> None:
    module = load_checker()
    root, base, _ = make_repo(design_text=None)
    try:
        path = root / 'doc/system/design.md'
        path.write_text(DESIGN_HEADER)
        contract = commit(root, 'add root contract')
        assert path in module.changed_system_design_paths(root, base, contract, False)
        p = relation_run(root, '--base', base, '--head', contract)
        assert p.returncode == 0, p.stdout + p.stderr
        path.write_text('# Module overview\n## Architecture\nNavigation only now.\n')
        p = relation_run(root, '--full-corpus')
        assert p.returncode == 0 and 'trace-upstream-missing' in p.stdout, p.stdout + p.stderr
        head = commit(root, 'remove root contract headings and tables')
        p = relation_run(root, '--base', contract, '--head', head)
        assert p.returncode == 1 and 'trace-upstream-missing' in p.stdout, p.stdout + p.stderr
        assert module.design_requires_traceability('doc/system/design.md', DESIGN_HEADER.replace('### 2.1 需求承接与分配表', '### Allocation').replace('### 11.1 验证映射表', '### Verification'))
    finally:
        shutil.rmtree(root)


def scenario_reverse_consumer_only_new_breakage_is_strict() -> None:
    root, base, _ = make_repo(design_text=DESIGN_HEADER.replace('[sample test](../tests/sample.test.py)', '[missing test](../tests/old-missing.test.py)'))
    try:
        (root / PRODUCT).write_text(PRODUCT_TEXT + '\nExtra explanation.\n')
        head = commit(root, 'clarify upstream without breaking its fragment')
        p = relation_run(root, '--base', base, '--head', head)
        assert p.returncode == 0, p.stdout + p.stderr
        (root / PRODUCT).write_text(PRODUCT_TEXT.replace('req-sample', 'renamed-feature'))
        broken = commit(root, 'remove consumed feature anchor')
        p = relation_run(root, '--base', head, '--head', broken)
        assert p.returncode == 1 and 'trace-ref-unresolved' in p.stdout, p.stdout + p.stderr
    finally:
        shutil.rmtree(root)


def scenario_manual_only_change_rechecks_unchanged_design() -> None:
    text = DESIGN_HEADER.replace('../product/sample.prd.md#req-sample', '../manual/rules.md#build-invariant').replace('[REQ-SAMPLE-001]', '[build invariant]')
    text = text.replace('系统设计条款说明边界、拒绝条件和可观察结果。', '产品适用性：N/A；reason=仅维护构建检查；scope=构建输入')
    text = text.replace('[sample test](../tests/sample.test.py)', '[old missing test](../tests/old-missing.test.py)')
    root, _, _ = make_repo(design_text=text)
    try:
        authority = root / 'doc/manual/rules.md'
        authority.write_text('# Rules\n## Build invariant\nDeterministic inputs.\n')
        base = commit(root, 'add precise professional acceptance source')
        authority.write_text(authority.read_text() + '\nClarification only.\n')
        head = commit(root, 'clarify manual without breaking relation')
        p = relation_run(root, '--base', base, '--head', head)
        assert p.returncode == 0, p.stdout + p.stderr
        authority.write_text('# Rules\n## Renamed invariant\nDeterministic inputs.\n')
        broken = commit(root, 'break consumed manual anchor')
        p = relation_run(root, '--base', head, '--head', broken)
        assert p.returncode == 1 and 'trace-ref-unresolved' in p.stdout, p.stdout + p.stderr
    finally:
        shutil.rmtree(root)


def scenario_formal_trace_table_and_multirefs_compatibility() -> None:
    module = load_checker()
    first = 'O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6'
    second = 'O7-REQ-7b97fe34-2be1-4b6c-9e8a-54fb03b52c16'
    assert module.declared_trace_ids('| Trace ID | Meaning |\n| --- | --- |\n| `' + first + '` | Requirement |') == {first}
    assert module.declared_trace_ids('| Meaning | Value |\n| --- | --- |\n| declaration | `Trace ID:` `' + first + '` |') == {first}
    assert module.trace_reference_ids('| `Trace refs` | Meaning |\n| --- | --- |\n| `' + first + '`; `' + second + '` | Both |') == ({first, second}, set())
    assert module.trace_reference_ids('- `Trace refs:` `' + first + '`; ' + second) == ({first, second}, set())
    assert module.declared_trace_ids('## Example\nTrace ID: ' + first) == set()
    assert module.declared_trace_ids('`Trace ID: ' + first + '`') == set()


def scenario_product_ac_and_professional_chain_preserve_unique_declaration() -> None:
    module = load_checker()
    identifier = 'O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6'
    root, _, _ = make_repo()
    try:
        product = PRODUCT_TEXT.replace('## REQ-SAMPLE-001', '## REQ-SAMPLE-001\n\n| Trace ID | Meaning |\n| --- | --- |\n| ' + identifier + ' | Product requirement |')
        product += '\n<a id="ac-sample"></a>\n## AC-SAMPLE-001\nTrace refs: ' + identifier + '\n[requirement](#req-sample)\n'
        (root / PRODUCT).write_text(product)
        text = DESIGN_HEADER.replace('#req-sample)', '#ac-sample)').replace('[REQ-SAMPLE-001]', '[AC-SAMPLE-001]')
        text = text.replace('说明系统必须承接的结果与适用条件。', 'Trace refs: ' + identifier)
        (root / DESIGN).write_text(text)
        assert_no_errors(module, root / DESIGN)
        rule = root / 'doc/system/rules.prd.md'
        rule.write_text('# Rules\n## Product rule\nTrace refs: ' + identifier + '\n[product acceptance](../product/sample.prd.md#ac-sample)\n')
        indirect = text.replace('../product/sample.prd.md#ac-sample', 'rules.prd.md#product-rule').replace('[AC-SAMPLE-001]', '[professional rule]')
        (root / DESIGN).write_text(indirect)
        assert_no_errors(module, root / DESIGN)
        assert module.declared_trace_ids(rule.read_text()) == set(), 'intermediate references must not redeclare ownership'
        # An exact chain to another section is mandatory; an unlinked mention
        # or unrelated ID elsewhere in the file cannot establish the relation.
        rule.write_text('# Rules\n## Product rule\nTrace refs: ' + identifier + '\nNo declaration link.\n')
        errors = module.check_system_design(root / DESIGN)
        assert any('trace-row-id-mismatch' in e for e in errors), errors
        rule.write_text('# Rules\n## Product rule\nTrace refs: ' + identifier + '\n[loop](rules.prd.md#product-rule)\n')
        errors = module.check_system_design(root / DESIGN)
        assert any('requirement chain cycle' in e for e in errors), errors
    finally:
        shutil.rmtree(root)


def scenario_root_rename_cannot_drop_previous_contract() -> None:
    module = load_checker()
    root, _, _ = make_repo(design_text=None)
    try:
        old = root / 'doc/system/design.md'
        padding = '\n'.join('Stable architecture paragraph %d: component ownership and boundaries remain unchanged.' % i for i in range(100))
        old.write_text(DESIGN_HEADER + padding)
        base = commit(root, 'root contract with stable architecture')
        new = root / 'doc/new/design.md'
        new.parent.mkdir()
        old.rename(new)
        new.write_text('# Module overview\n' + padding)
        # Staged rename also retains the HEAD contract during a full scan.
        run_git(root, 'add', '.')
        assert module.previous_design_text(root, base, base, 'doc/new/design.md', True) == DESIGN_HEADER + padding
        head = commit(root, 'rename and remove mapping contract')
        assert 'R' in run_git(root, 'diff', '--name-status', '-M', base, head)
        p = relation_run(root, '--base', base, '--head', head)
        assert p.returncode == 1 and 'trace-upstream-missing' in p.stdout, p.stdout + p.stderr
    finally:
        shutil.rmtree(root)


def scenario_validation_source_reverse_selection() -> None:
    root, base, _ = make_repo()
    try:
        (root / TEST_SOURCE).unlink()
        head = commit(root, 'delete consumed verification source')
        p = relation_run(root, '--base', base, '--head', head)
        assert p.returncode == 1 and 'reference target is missing' in p.stdout, p.stdout + p.stderr
        unrelated = root / 'unrelated.py'
        unrelated.write_text('# unrelated implementation change\n')
        binary = root / 'unrelated.bin'
        binary.write_bytes(b'\xff\xfe\x00')
        later = commit(root, 'unrelated sources do not collect existing verification debt')
        p = relation_run(root, '--base', head, '--head', later)
        assert p.returncode == 0, p.stdout + p.stderr
    finally:
        shutil.rmtree(root)


def scenario_transitive_reverse_dependency_selection() -> None:
    identifier = 'O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6'
    for mutation in ('id', 'anchor', 'delete'):
        root, _, _ = make_repo()
        try:
            product = PRODUCT_TEXT.replace('## REQ-SAMPLE-001', '## REQ-SAMPLE-001\n\nTrace ID: ' + identifier)
            (root / PRODUCT).write_text(product)
            (root / 'doc/manual/rules.md').write_text('# Rules\n## Rule\nTrace refs: ' + identifier + '\n[product requirement](../product/sample.prd.md#req-sample)\n')
            design = DESIGN_HEADER.replace('../product/sample.prd.md#req-sample', '../manual/rules.md#rule').replace('[REQ-SAMPLE-001]', '[professional rule]')
            design = design.replace('说明系统必须承接的结果与适用条件。', 'Trace refs: ' + identifier)
            design = design.replace('[sample test](../tests/sample.test.py)', '[old missing test](../tests/old-missing.test.py)')
            (root / DESIGN).write_text(design)
            base = commit(root, 'product to professional acceptance to design chain')
            (root / PRODUCT).write_text(product + '\nClarification without relationship change.\n')
            unchanged = commit(root, 'clarify transitive endpoint without collecting old debt')
            p = relation_run(root, '--base', base, '--head', unchanged)
            assert p.returncode == 0, p.stdout + p.stderr
            if mutation == 'delete':
                (root / PRODUCT).unlink()
            elif mutation == 'anchor':
                (root / PRODUCT).write_text(product.replace('req-sample', 'new-anchor'))
            else:
                (root / PRODUCT).write_text(product.replace(identifier, 'O7-REQ-7b97fe34-2be1-4b6c-9e8a-54fb03b52c16'))
            broken = commit(root, 'break transitive endpoint ' + mutation)
            p = relation_run(root, '--base', unchanged, '--head', broken)
            assert p.returncode == 1 and 'trace-id-unresolved' in p.stdout, p.stdout + p.stderr
        finally:
            shutil.rmtree(root)


def scenario_clause_graph_diamonds_are_parsed_once_per_call() -> None:
    module = load_checker()
    identifier = 'O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6'
    other = 'O7-REQ-7b97fe34-2be1-4b6c-9e8a-54fb03b52c16'
    root = Path('/virtual-clause-graph')
    documents = {}
    depth = 12
    for level in range(depth):
        for label in ('a', 'b'):
            links = (f'[a]({level + 1}a.prd.md#rule) [repeat a]({level + 1}a.prd.md#rule) [b]({level + 1}b.prd.md#rule)'
                     if level < depth - 1 else '[origin](../product/feature.prd.md#req-actual-001)')
            documents[root / f'doc/system/{level}{label}.prd.md'] = '# Rules\n## Rule\nTrace refs: ' + identifier + '\n' + links
    product = root / 'doc/product/feature.prd.md'
    documents[product] = '# Product\n## REQ-ACTUAL-001\nTrace ID: ' + identifier
    initial = root / 'doc/system/0a.prd.md'
    parse_calls, reads = [], []
    original = module.fragment_section
    def counted_parse(*args):
        parse_calls.append(args[1])
        assert len(parse_calls) <= len(documents), 'diamond paths must not cause repeated clause parsing'
        return original(*args)
    def reader(path):
        reads.append(path)
        return documents.get(path)
    module.fragment_section = counted_parse
    dependencies = set()
    ids, product_chain = module.resolved_requirement_context(root, initial, documents[initial], 'rule', reader, dependencies=dependencies)
    assert ids == {identifier} and product_chain
    assert len(reads) == len(set(reads)), 'each source is read at most once inside one clause graph'
    expected = {path.relative_to(root).as_posix() for path in documents if path != root / 'doc/system/0b.prd.md'}
    assert dependencies == expected, (dependencies, expected)
    # A later snapshot/call cannot inherit cached identities from this one.
    documents[product] = documents[product].replace(identifier, other)
    parse_calls.clear()
    ids, product_chain = module.resolved_requirement_context(root, initial, documents[initial], 'rule', reader)
    assert not ids and not product_chain


def scenario_clause_graph_preserves_depth_cycle_and_ref_contexts() -> None:
    module = load_checker()
    identifier = 'O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6'
    other = 'O7-REQ-7b97fe34-2be1-4b6c-9e8a-54fb03b52c16'
    root = Path('/virtual-clause-contexts')
    source = root / 'doc/system/root.prd.md'
    product = root / 'doc/product/feature.prd.md'
    wrong = root / 'doc/product/other.prd.md'
    documents = {product: '# Product\n## REQ-ACTUAL-001\nTrace ID: ' + identifier,
                 wrong: '# Other\n## REQ-OTHER-001\nTrace ID: ' + other}
    for i in range(1, 32):
        target = f'{i + 1}.prd.md#rule' if i < 31 else '../product/feature.prd.md#req-actual-001'
        documents[root / f'doc/system/{i}.prd.md'] = '# Clause file\n## Rule\nTrace refs: ' + identifier + '\n[next](' + target + ')'
    branches = ['[shallow](../product/feature.prd.md#req-actual-001)', '[deep](1.prd.md#rule)', '[wrong](../product/other.prd.md#req-other-001)']
    results = []
    for links in (branches, list(reversed(branches))):
        documents[source] = '# Root\n## Rule\nTrace refs: ' + identifier + '\n' + ' '.join(links)
        issues, paths = [], set()
        ids, has_product = module.resolved_requirement_context(root, source, documents[source], 'rule', documents.get, diagnostics=issues, dependencies=paths)
        assert ids == {identifier} and has_product, (ids, has_product)
        assert any('depth limit (32)' in issue for issue in issues), issues
        results.append((ids, has_product, set(issues), paths))
    assert results[0] == results[1], 'shallow cache must not hide deep cutoffs, or vice versa'
    documents[source] = '# Root\n## Rule\nTrace refs: ' + identifier + '\n[next](cycle.prd.md#rule)'
    documents[root / 'doc/system/cycle.prd.md'] = '# Cycle\n## Rule\nTrace refs: ' + identifier + '\n[back](root.prd.md#rule) [origin](../product/feature.prd.md#req-actual-001)'
    issues, paths = [], set()
    ids, has_product = module.resolved_requirement_context(root, source, documents[source], 'rule', documents.get, diagnostics=issues, dependencies=paths)
    assert ids == {identifier} and has_product
    assert any('chain cycle:' in issue for issue in issues), issues
    assert paths == {'doc/system/root.prd.md', 'doc/system/cycle.prd.md', 'doc/product/feature.prd.md'}, paths


def scenario_plain_navigation_is_not_a_requirement_chain() -> None:
    module = load_checker()
    identifier = 'O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6'
    own_identifier = 'O7-REQ-7b97fe34-2be1-4b6c-9e8a-54fb03b52c16'
    root = Path('/virtual-explicit-chain')
    source = root / 'doc/system/rules.prd.md'
    bridge = root / 'doc/system/navigation.prd.md'
    product = root / 'doc/product/feature.prd.md'
    documents = {source: '# Rules\n## Rule\nTrace ID: ' + own_identifier + '\n[navigation](navigation.prd.md#bridge)',
                 bridge: '# Navigation\n## Bridge\nTrace refs: ' + identifier + '\n[product](../product/feature.prd.md#req-actual-001)',
                 product: '# Product\n## REQ-ACTUAL-001\nTrace ID: ' + identifier + '\n'}
    reads, dependencies = [], set()
    def reader(path):
        reads.append(path)
        return documents.get(path)
    ids, has_product = module.resolved_requirement_context(root, source, documents[source], 'rule', reader, dependencies=dependencies)
    assert ids == {own_identifier} and not has_product
    assert not reads, 'no-refs professional navigation must not read its recursive neighborhood'
    assert dependencies == {'doc/system/rules.prd.md'}, dependencies
    # A direct precise product citation remains sufficient; no invented ID or
    # repeated declaration is needed for this existing local relationship.
    documents[source] = '# Rules\n## Rule\n[product](../product/feature.prd.md#req-actual-001)'
    ids, has_product = module.resolved_requirement_context(root, source, documents[source], 'rule', reader)
    assert ids == set() and has_product
    # Direct AC may itself explicitly reference its unique REQ declaration.
    documents[product] += '\n## Acceptance\nTrace refs: ' + identifier + '\n[requirement](#req-actual-001)'
    documents[source] = '# Rules\n## Rule\n[acceptance](../product/feature.prd.md#acceptance)'
    _, has_product = module.resolved_requirement_context(root, source, documents[source], 'rule', reader)
    assert has_product
    # A product generic heading with only ordinary navigation is not a relay.
    documents[product] += '\n## Overview\n[requirement](#req-actual-001)'
    documents[source] = '# Rules\n## Rule\n[overview](../product/feature.prd.md#overview)'
    _, has_product = module.resolved_requirement_context(root, source, documents[source], 'rule', reader)
    assert not has_product
    # Adding an actual same-ID field establishes the intended multi-hop chain.
    documents[source] = '# Rules\n## Rule\nTrace refs: ' + identifier + '\n[rule](navigation.prd.md#bridge)'
    ids, has_product = module.resolved_requirement_context(root, source, documents[source], 'rule', reader)
    assert ids == {identifier} and has_product


def scenario_validation_source_filename_tokens() -> None:
    module = load_checker()
    design = Path("doc/system/sample.design.md")
    accepted = (
        "transfer_submit_api_tests.rs", "transfer_submit_api_explorer_tests.rs",
        "main_tests.rs", "explorer_tests.rs", "tests.rs", "specs.rs",
        "tests_sample.rs", "sample.tests.rs", "sample-tests.rs",
        "sample_specs.rs", "sample.specs.ts", "sample-specs.py",
        "SAMPLE_TESTS.RS", "sample_test.rs", "sample.spec.ts",
    )
    rejected = (
        "transfer_auth.rs", "main.rs", "contest.rs", "latest.rs",
        "sample_teststuff.rs", "sample_testsuite.rs", "mytests.rs",
        "sample_specsification.rs", "aspects.rs", "specsheet.rs",
        "sample_tests2.rs", "sample_2tests.rs", "sample_tests.design.md",
        "sample_specs.design.md", "source.unknown",
    )
    for name in accepted:
        assert module.is_test_or_manual_source(Path("crates/sample/src") / name, design), name
    for name in rejected:
        assert not module.is_test_or_manual_source(Path("crates/sample/src") / name, design), name
    assert not module.is_test_or_manual_source(design, design)


def scenario_plural_sources_preserve_reference_guards() -> None:
    module = load_checker()
    root, _base, _head = make_repo()
    source = root / "crates/sample/src/transfer_submit_api_tests.rs"
    source.parent.mkdir(parents=True)
    source.write_text("#[test]\nfn rejects_invalid_request() {}\n", encoding="utf-8")
    original = "[sample test](../tests/sample.test.py)"
    method = "[request tests](../../crates/sample/src/transfer_submit_api_tests.rs)"
    try:
        (root / DESIGN).write_text(DESIGN_HEADER.replace(original, method), encoding="utf-8")
        assert_no_errors(module, root / DESIGN)
        source.unlink()
        assert_code(module, root / DESIGN, "trace-validation-missing")
        source.write_text("#[test]\nfn rejects_invalid_request() {}\n", encoding="utf-8")
        for invalid_method, code in (
            (method + " " + original, "trace-validation-source-invalid"),
            (method.replace(".rs)", ".rs#missing)"), "trace-ref-unresolved"),
            ("[escape](../../../outside_tests.rs)", "trace-validation-missing"),
        ):
            (root / DESIGN).write_text(DESIGN_HEADER.replace(original, invalid_method), encoding="utf-8")
            assert_code(module, root / DESIGN, code)
    finally:
        shutil.rmtree(root)


def scenario_plain_source_content_cannot_impersonate_test_path() -> None:
    module = load_checker()
    root, _base, _head = make_repo()
    source = root / "crates/sample/src/transfer_auth.rs"
    source.parent.mkdir(parents=True)
    method = "[source](../../crates/sample/src/transfer_auth.rs)"
    (root / DESIGN).write_text(
        DESIGN_HEADER.replace("[sample test](../tests/sample.test.py)", method), encoding="utf-8"
    )
    try:
        for text in (
            "fn test_request() {}\n",
            "// #[test] fn fake() {}\n",
            "/* #[test] fn fake() {} */\n",
            'const S: &str = "#[test] fn fake() {}";\n',
            'const S: &str = r###"#[test] fn fake() {}"###;\n',
            "#[cfg(test)] mod tests {}\n",
            "#[test_case] fn fake() {}\n",
            # Inline Rust test parsing is deliberately outside this path-only fix.
            "#[cfg(test)] mod tests { #[test] fn actual() {} }\n",
        ):
            source.write_text(text, encoding="utf-8")
            assert_code(module, root / DESIGN, "trace-validation-source-invalid")
    finally:
        shutil.rmtree(root)


def main() -> None:
    scenarios = (
        scenario_validation_source_filename_tokens,
        scenario_plural_sources_preserve_reference_guards,
        scenario_plain_source_content_cannot_impersonate_test_path,
        scenario_plain_navigation_is_not_a_requirement_chain,
        scenario_clause_graph_diamonds_are_parsed_once_per_call,
        scenario_clause_graph_preserves_depth_cycle_and_ref_contexts,
        scenario_validation_source_reverse_selection,
        scenario_transitive_reverse_dependency_selection,
        scenario_formal_trace_table_and_multirefs_compatibility,
        scenario_product_ac_and_professional_chain_preserve_unique_declaration,
        scenario_root_rename_cannot_drop_previous_contract,
        scenario_manual_only_change_rechecks_unchanged_design,
        scenario_product_fragment_must_identify_feature,
        scenario_trace_refs_require_addressed_declaration,
        scenario_engineering_only_needs_bounded_na,
        scenario_validation_na_rejects_placeholders,
        scenario_feature_boundaries_and_explicit_label_mismatch,
        scenario_root_scoped_and_contract_downgrade_fail,
        scenario_reverse_consumer_only_new_breakage_is_strict,
        scenario_tracking_metadata_does_not_hide_design_changes,
        scenario_valid_new_design,
        scenario_committed_range_reads_trusted_head_content,
        scenario_committed_target_symlink_does_not_redirect_selected_head,
        scenario_worktree_symlinks_are_contained,
        scenario_missing_demand_allocation,
        scenario_missing_validation_mapping,
        scenario_unresolved_upstream_fragment,
        scenario_required_row_shape_and_cells,
        scenario_validation_source_must_be_test_or_manual,
        scenario_anchor_and_heading_fragment_collision_is_ambiguous,
        scenario_shared_markdown_adapters_cache_immutable_results_by_exact_text,
        scenario_fenced_code_trace_decoy_is_ignored,
        scenario_complete_na,
        scenario_na_accepts_readable_repository_path_fragment,
        scenario_na_rejects_unreadable_or_noncanonical_evidence_locator,
        scenario_incomplete_na,
        scenario_target_only_design_change_is_excluded,
        scenario_whitespace_and_comment_only_source_change_is_excluded,
        scenario_untracked_worktree_design_is_included,
        scenario_staged_worktree_design_is_included,
        scenario_staged_tracked_worktree_design_uses_index_content,
        scenario_unstaged_worktree_design_is_included,
        scenario_duplicate_validation_rows_fail_cardinality,
        scenario_committed_validation_symlink_sources_fail_closed,
        scenario_untouched_legacy_design_is_excluded,
        scenario_authority_only_prompt_control_change_rechecks_current_consumer,
        scenario_authority_only_root_prd_prompt_control_change_rechecks_consumer,
        scenario_authority_rename_and_delete_fail_closed,
        scenario_worktree_authority_only_change_rechecks_current_consumer,
        scenario_untouched_legacy_consumer_is_excluded_from_authority_change,
        scenario_partial_or_malformed_range_is_rejected,
    )
    for scenario in scenarios:
        scenario()
    print(f"system-design-traceability-check.test: OK ({len(scenarios)} scenarios)")


if __name__ == "__main__":
    main()
