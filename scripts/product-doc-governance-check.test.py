#!/usr/bin/env python3
"""Regression fixtures for product-doc-governance-check.py."""

from __future__ import annotations

from pathlib import Path
import re
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parent.parent
CHECKER = ROOT / "scripts/product-doc-governance-check.py"
FIXTURE_RETIRED_HISTORY = Path(
    "doc/product/world-infrastructure/fixture-retired-history.prd.md"
)
COPY_PATHS = (
    "README.md",
    "doc/product",
    "doc/game/prd.md",
    "doc/game/gameplay/gameplay-top-level-design.prd.md",
    "doc/game/gameplay/gameplay-industrial-externality-contract.prd.md",
    "doc/game/gameplay/gameplay-industrial-factory-capability-lifecycle-contract.prd.md",
    "doc/game/gameplay/gameplay-industrial-recipe-lifecycle-decision-contract.prd.md",
    "doc/game/gameplay/gameplay-industrial-pipeline-service-window-contract.prd.md",
    "doc/core/prd.md",
    "doc/engineering/doc-governance/doc-structure-standard.design.md",
    "doc/site/prd.md",
    "doc/world-runtime/prd.md",
    "doc/world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md",
    "doc/p2p/prd.md",
    "doc/testing/prd.md",
    "doc/world-simulator/prd.md",
    "doc/world-simulator/m4/industrial-resource-flow-contract.prd.md",
    "doc/world-simulator/llm/decision-provider-contract.prd.md",
    "doc/world-simulator/llm/provider-agent-experience-parity.prd.md",
    "doc/world-simulator/llm/decision-provider-contract.prd.md",
    "doc/world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md",
    "doc/game/gameplay/gameplay-agent-claim-economy-contract.prd.md",
)

# The slug and Product PRD-ID are the stable identity.  The tuple keeps both
# the retired and final display labels so C1 can prove that only the final
# label is accepted in active identity declarations while retired prose stays
# recognizable.
MODULE_NAME_COMPATIBILITY = (
    (
        "world-rules-core-gameplay",
        "PRD-PRODUCT-001",
        "世界规则与核心玩法",
        "世界规则与玩法系统",
    ),
    (
        "world-infrastructure",
        "PRD-PRODUCT-002",
        "大世界基础设施",
        "权威世界基础设施",
    ),
    (
        "agents-world-simulation",
        "PRD-PRODUCT-003",
        "智能体与世界模拟",
        "智能体、世界模拟与交互",
    ),
    (
        "player-entry-distribution",
        "PRD-PRODUCT-004",
        "玩家入口与发行",
        "玩家接入与发行",
    ),
)


def make_fixture() -> Path:
    root = Path(tempfile.mkdtemp(prefix="oasis7-product-doc-fixture-"))
    for relative in COPY_PATHS:
        source = ROOT / relative
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_dir():
            shutil.copytree(source, target)
        else:
            shutil.copy2(source, target)
    module_root = root / "doc/product/world-infrastructure/prd.md"
    module_text = module_root.read_text(encoding="utf-8")
    migration_heading = "### 非权威迁移索引\n"
    assert migration_heading in module_text, "fixture module root missing migration index"
    module_root.write_text(
        module_text.replace(
            migration_heading,
            migration_heading
            + f"- [Fixture retired history]({FIXTURE_RETIRED_HISTORY.name}): test-only provenance.\n",
            1,
        ),
        encoding="utf-8",
    )
    (root / FIXTURE_RETIRED_HISTORY).write_text(
        """# Fixture retired history

## Document identity

- 生命周期：`retired`

## Historical navigation

This synthetic page preserves non-authoritative migration provenance for tests.
It is not an active product identity, authority, roadmap, or acceptance source.
""",
        encoding="utf-8",
    )
    assert (root / FIXTURE_RETIRED_HISTORY).is_file()
    return root


def run(root: Path, expected: str | None = None) -> None:
    result = subprocess.run(
        ["python3", str(CHECKER), "--repo-root", str(root)],
        check=False,
        capture_output=True,
        text=True,
    )
    output = result.stdout + result.stderr
    if expected is None:
        assert result.returncode == 0, output
        assert "product-doc-governance: OK" in output, output
    else:
        assert result.returncode != 0, output
        assert expected in output, output


def replace(path: Path, before: str, after: str) -> None:
    text = path.read_text(encoding="utf-8")
    assert before in text, f"fixture source missing {before!r} in {path}"
    path.write_text(text.replace(before, after, 1), encoding="utf-8")


def replace_all(path: Path, before: str, after: str) -> None:
    text = path.read_text(encoding="utf-8")
    assert before in text, f"fixture source missing {before!r} in {path}"
    path.write_text(text.replace(before, after), encoding="utf-8")


def replace_from_current_name(path: Path, candidates: tuple[str, str], before_suffix: str, after: str) -> None:
    text = path.read_text(encoding="utf-8")
    for current_name in candidates:
        before = f"{before_suffix}{current_name}"
        if before in text:
            path.write_text(text.replace(before, f"{before_suffix}{after}", 1), encoding="utf-8")
            return
    raise AssertionError(f"fixture source missing any current name {candidates!r} in {path}")


def replace_manifest_module_name(
    path: Path, slug: str, candidates: tuple[str, str], name: str
) -> None:
    text = path.read_text(encoding="utf-8")
    for current_name in candidates:
        before = f"| {current_name} | [`doc/product/{slug}/prd.md`]("
        if before in text:
            after = f"| {name} | [`doc/product/{slug}/prd.md`]("
            path.write_text(text.replace(before, after, 1), encoding="utf-8")
            return
    raise AssertionError(f"fixture manifest missing any current name {candidates!r} for {slug}")


def set_module_display_name(root: Path, module: tuple[str, str, str, str], name: str) -> None:
    """Set one module's manifest and canonical-root display name from either P1/C0 source spelling."""
    set_landing_display_name(root, module, name)
    set_root_display_name(root, module, name)


def set_landing_display_name(root: Path, module: tuple[str, str, str, str], name: str) -> None:
    """Change only one active landing-row display name."""
    slug, _prd_id, legacy_name, final_name = module
    current_names = (legacy_name, final_name)
    manifest = root / "doc/product/README.md"
    replace_manifest_module_name(manifest, slug, current_names, name)


def set_root_display_name(root: Path, module: tuple[str, str, str, str], name: str) -> None:
    """Change the canonical root heading and active product-module metadata."""
    slug, _prd_id, legacy_name, final_name = module
    current_names = (legacy_name, final_name)
    replace_from_current_name(
        root / f"doc/product/{slug}/prd.md",
        current_names,
        "# ",
        f"{name} PRD",
    )
    replace_from_current_name(
        root / f"doc/product/{slug}/prd.md",
        current_names,
        "- 产品模块：",
        name,
    )


def set_all_module_display_names(root: Path) -> None:
    """Switch every landing row, root heading, and metadata label to P1 names."""
    for module in MODULE_NAME_COMPATIBILITY:
        set_module_display_name(root, module, module[3])


def add_historical_legacy_prose(root: Path) -> None:
    """Historical migration prose may retain old labels without active identity."""
    path = root / FIXTURE_RETIRED_HISTORY
    path.write_text(
        path.read_text(encoding="utf-8")
        + (
            "\n历史迁移 provenance 仍可提及旧标签：世界规则与核心玩法、大世界基础设施、"
            "智能体与世界模拟、玩家入口与发行；这些文字不是当前产品入口或 canonical root identity。\n"
        ),
        encoding="utf-8",
    )


def add_invalid_active_topic_fragment(root: Path) -> None:
    """An active module inventory link must resolve its real target fragment."""
    replace(
        root / "doc/product/world-rules-core-gameplay/prd.md",
        "(first-session-and-continuation.prd.md)",
        "(first-session-and-continuation.prd.md#missing-active-topic-fragment)",
    )


def add_invalid_paired_design_fragment(root: Path) -> None:
    """A paired design mapping must resolve the referenced product anchor."""
    replace(
        root / "doc/product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.design.md",
        "industrial-demand-goals-and-settlement.prd.md#req-sc31-001",
        "industrial-demand-goals-and-settlement.prd.md#missing-paired-design-fragment",
    )


def add_invalid_retired_migration_fragment(root: Path) -> None:
    """A retired migration mapping must preserve navigable fragment targets."""
    replace(
        root / "doc/product/world-infrastructure/prd.md",
        f"({FIXTURE_RETIRED_HISTORY.name})",
        f"({FIXTURE_RETIRED_HISTORY.name}#missing-retired-migration-fragment)",
    )


def add_fenced_identity_pseudo_content(root: Path, module: tuple[str, str, str, str]) -> None:
    """Add name/identity-looking prose outside canonical roots; it must be ignored."""
    slug, prd_id, _legacy_name, final_name = module
    path = root / "doc/core/prd.md"
    path.write_text(
        path.read_text(encoding="utf-8")
        + (
            "\n```markdown\n"
            f"- 产品模块：{final_name}\n"
            f"- 产品模块 slug：`{slug}`\n"
            f"- Product PRD-ID：`{prd_id}`\n"
            "```\n"
        ),
        encoding="utf-8",
    )


def remove_paired_design_links(root: Path, *, fence: str | None = None) -> None:
    """Remove every live paired-PRD target while preserving a fenced negative case."""
    path = root / "doc/product/agents-world-simulation/agent-conversation-and-prompt-control.design.md"
    text = path.read_text(encoding="utf-8")
    text = re.sub(
        r"\[([^\]]+)\]\(agent-conversation-and-prompt-control\.prd\.md(?:#[^)]+)?\)",
        r"`\1`",
        text,
    )
    if fence:
        text += (
            f"\n{fence}markdown\n"
            "[paired PRD](agent-conversation-and-prompt-control.prd.md)\n"
            f"{fence}\n"
        )
    path.write_text(text, encoding="utf-8")


def pseudo_paired_design_link(root: Path, form: str) -> None:
    path = root / "doc/product/agents-world-simulation/agent-conversation-and-prompt-control.design.md"
    remove_paired_design_links(root)
    target = "[paired PRD](agent-conversation-and-prompt-control.prd.md)"
    if form == "inline":
        target = f"`{target}`"
    elif form == "indented":
        target = f"    {target}"
    elif form == "comment":
        target = f"<!-- {target} -->"
    elif form == "escaped":
        target = f"\\{target}"
    elif form == "even-escaped":
        target = f"\\\\{target}"
    elif form == "image":
        target = f"!{target}"
    elif form == "quote-fence":
        target = "> ```markdown\n> " + target + "\n> ```"
    elif form == "list-fence":
        target = "- ```markdown\n  " + target + "\n  ```"
    elif form == "quote-indent":
        target = ">     " + target
    elif form == "list-indent":
        target = "-     " + target
    else:
        raise AssertionError(form)
    path.write_text(path.read_text(encoding="utf-8") + f"\n{target}\n", encoding="utf-8")


def scenario(expected: str | None, mutation) -> None:
    root = make_fixture()
    try:
        mutation(root)
        run(root, expected)
    finally:
        shutil.rmtree(root)


def assert_access_mode_consolidated(root: Path) -> None:
    module = root / "doc/product/player-entry-distribution"
    dated_basename = "player-access-mode-contract-2026-03-19"
    assert not list(module.glob(f"{dated_basename}.*.md")), "dated access-mode companion set must remain absent"
    assert dated_basename not in (module / "prd.md").read_text(encoding="utf-8"), (
        "module PRD must not restore a dated active-topic link"
    )


def scenario_equivalent_navigation_and_nonmatching_pair_names() -> None:
    root = make_fixture()
    try:
        module = root / 'doc/product/agents-world-simulation/prd.md'
        text = module.read_text().replace('### 活跃产品专题', '### 相关产品主题').replace('### 非权威迁移索引', '### 历史参考')
        module.write_text(text)
        old = root / 'doc/product/agents-world-simulation/agent-conversation-and-prompt-control.design.md'
        renamed = old.with_name('shared-conversation-experience.design.md')
        old.rename(renamed)
        for path in (root / 'doc/product').rglob('*.md'):
            path.write_text(path.read_text().replace(old.name, renamed.name))
        run(root)
    finally:
        shutil.rmtree(root)


def scenario_missing_status_metadata_does_not_become_integrity_failure() -> None:
    root = make_fixture()
    try:
        path = root / 'doc/product/world-infrastructure/prd.md'
        original = path.read_text()
        path.write_text(original.replace('- 生命周期：`active`\n', '').replace('- 后继文档：`无`\n', ''))
        # Ordinary history navigation is not an undeclared product pairing.
        (path.parent / 'concept.design.md').write_text('# Concept\n历史背景：[旧材料](fixture-retired-history.prd.md)\n')
        result = subprocess.run(['python3', str(CHECKER), '--repo-root', str(root)], capture_output=True, text=True)
        assert result.returncode == 0 and '::warning::' in result.stdout, result.stdout + result.stderr
        path.write_text(original.replace('- 生命周期：`active`', '- 生命周期：`unknown`'))
        run(root, 'lifecycle-contract')
        path.write_text(original + '\n- SC-1：Duplicate actual criterion declaration.\n')
        run(root, 'traceability-duplicate-id')
    finally:
        shutil.rmtree(root)


def main() -> None:
    scenario_equivalent_navigation_and_nonmatching_pair_names()
    scenario_missing_status_metadata_does_not_become_integrity_failure()
    root = make_fixture()
    try:
        run(root)
        assert_access_mode_consolidated(root)
    finally:
        shutil.rmtree(root)

    # C1 keeps old labels recognizable in historical prose but no longer accepts
    # them as active landing/root identities. These two scenarios are RED until
    # the production checker stops treating C0 names as active aliases.
    module = MODULE_NAME_COMPATIBILITY[0]
    scenario(
        "entry-contract",
        lambda root: set_landing_display_name(root, module, module[2]),
    )
    scenario(
        "metadata-contract",
        lambda root: set_root_display_name(root, module, module[2]),
    )
    scenario(None, add_historical_legacy_prose)

    # C1-02 must reject broken anchors in active inventories, paired designs,
    # and retired migration mappings while leaving existing path checks intact.
    # These are RED until the checker validates fragments in product scope.
    for invalid_fragment_fixture in (
        add_invalid_active_topic_fragment,
        add_invalid_paired_design_fragment,
        add_invalid_retired_migration_fragment,
    ):
        scenario("invalid-fragment", invalid_fragment_fixture)

    # The explicit all-final identity case remains a positive control.
    scenario(None, set_all_module_display_names)

    # Arbitrary names and another module's name are not compatibility aliases.
    scenario(
        "entry-contract",
        lambda root: set_module_display_name(root, MODULE_NAME_COMPATIBILITY[0], "任意产品模块名称"),
    )
    scenario(
        "entry-contract",
        lambda root: (
            set_module_display_name(root, MODULE_NAME_COMPATIBILITY[0], MODULE_NAME_COMPATIBILITY[1][3]),
            set_module_display_name(root, MODULE_NAME_COMPATIBILITY[1], MODULE_NAME_COMPATIBILITY[0][3]),
        ),
    )

    # A display-name match never overrides the stable slug or Product PRD-ID.
    module = MODULE_NAME_COMPATIBILITY[0]
    slug, prd_id, _legacy_name, final_name = module
    scenario(
        "metadata-contract",
        lambda root: (
            set_module_display_name(root, module, final_name),
            replace(
                root / f"doc/product/{slug}/prd.md",
                f"- 产品模块 slug：`{slug}`",
                f"- 产品模块 slug：`{MODULE_NAME_COMPATIBILITY[1][0]}`",
            ),
        ),
    )
    scenario(
        "metadata-contract",
        lambda root: (
            set_module_display_name(root, module, final_name),
            replace(
                root / f"doc/product/{slug}/prd.md",
                f"- Product PRD-ID：`{prd_id}`",
                f"- Product PRD-ID：`{MODULE_NAME_COMPATIBILITY[1][1]}`",
            ),
        ),
    )

    # Name/identity-looking fenced prose in a non-canonical document remains
    # pseudo-content and must not satisfy or violate the module contract.
    scenario(None, lambda root: add_fenced_identity_pseudo_content(root, MODULE_NAME_COMPATIBILITY[0]))

    scenario(
        "retired-project-ledger",
        lambda root: (
            root.joinpath("doc/product/world-infrastructure/reintroduced.project.md").write_text(
                "# forbidden ledger\n", encoding="utf-8"
            )
        ),
    )

    scenario(
        "entry-contract",
        lambda root: replace(
            root / "doc/product/README.md",
            "| 玩家接入与发行 |",
            "| 第五个产品 | [`doc/product/fifth/prd.md`](fifth/prd.md) | 非法。 |\n| 玩家接入与发行 |",
        ),
    )
    scenario(
        "topic-index-contract",
        lambda root: (
            (root / "doc/product/untracked/nested").mkdir(parents=True),
            shutil.copy2(
                root / "doc/product/world-rules-core-gameplay/prd.md",
                root / "doc/product/untracked/nested/untracked.prd.md",
            ),
        ),
    )
    scenario(
        "topic-lifecycle",
        lambda root: replace(
            root / "doc/product/world-infrastructure/deterministic-world-execution.prd.md",
            "- 生命周期：`active`",
            "- 生命周期：`superseded`",
        ),
    )
    scenario(
        "topic-lifecycle",
        lambda root: replace(
            root / FIXTURE_RETIRED_HISTORY,
            "- 生命周期：`retired`",
            "- 生命周期：`active`",
        ),
    )
    scenario(
        "entry-contract",
        lambda root: replace(
            root / "doc/product/README.md",
            "(world-rules-core-gameplay/prd.md)",
            "(../game/prd.md)",
        ),
    )
    scenario(
        "metadata-placeholder-id",
        lambda root: replace(
            root / "doc/product/world-infrastructure/prd.md",
            "PRD-PRODUCT-002",
            "PRD-PRODUCT-xxx",
        ),
    )
    scenario(
        "lifecycle-contract",
        lambda root: replace(
            root / "doc/product/agents-world-simulation/prd.md",
            "- 生命周期：`active`",
            "- 生命周期：`unknown`",
        ),
    )
    scenario(
        "authority-backlink",
        lambda root: replace(
            root / "doc/world-simulator/prd.md",
            "../product/agents-world-simulation/prd.md",
            "../product/missing/prd.md",
        ),
    )
    scenario(
        "authority-backlink",
        lambda root: replace(
            root / "doc/world-simulator/prd.md",
            "[`doc/product/agents-world-simulation/prd.md`](../product/agents-world-simulation/prd.md)",
            "`doc/product/agents-world-simulation/prd.md`",
        ),
    )
    for authority in (
        "doc/world-runtime/prd.md",
        "doc/world-simulator/prd.md",
        "doc/p2p/prd.md",
    ):
        scenario(
            "authority-backlink",
            lambda root, authority=authority: replace(
                root / authority,
                "[`doc/product/world-rules-core-gameplay/prd.md`](../product/world-rules-core-gameplay/prd.md)",
                "`doc/product/world-rules-core-gameplay/prd.md`",
            ),
        )
    scenario(
        "authority-contract",
        lambda root: replace(
            root / "doc/product/world-infrastructure/prd.md",
            "[`doc/game/prd.md`](../../game/prd.md)",
            "`doc/game/prd.md`",
        ),
    )
    scenario(
        "traceability-contract",
        lambda root: replace(
            root / "doc/product/player-entry-distribution/prd.md",
            "| SC-4 | qa_engineer |",
            "| SC-9 | qa_engineer |",
        ),
    )
    scenario(
        "traceability-prd-resolution",
        lambda root: replace(
            root / "doc/product/world-infrastructure/prd.md",
            "PRD-P2P-001",
            "PRD-P2P-999",
        ),
    )
    scenario(
        "identity-metadata-location",
        lambda root: replace(
            root / "doc/core/prd.md",
            "# core PRD",
            "# core PRD\n\n- 产品模块：`不允许在此声明`",
        ),
    )
    scenario(
        "identity-metadata-location",
        lambda root: replace(
            root / "doc/product/agents-world-simulation/provider-agent-experience-continuity.prd.md",
            "# Agent/provider 体验连续性",
            "# Agent/provider 体验连续性\n\n- Product PRD-ID：`PRD-PRODUCT-003`",
        ),
    )
    scenario(
        "topic-professional-authority",
        lambda root: replace_all(
            root / "doc/product/agents-world-simulation/provider-agent-experience-continuity.prd.md",
            "../../world-simulator/llm/provider-agent-experience-parity.prd.md",
            "../../world-simulator/llm/"
            + "llm-provider-agent-experience-parity"
            + "-2026-03-12.prd.md",
        ),
    )
    scenario(
        "topic-pair-backlink",
        lambda root: remove_paired_design_links(root),
    )
    scenario(
        "topic-pair-backlink",
        lambda root: remove_paired_design_links(root, fence="```"),
    )
    scenario(
        "topic-pair-backlink",
        lambda root: remove_paired_design_links(root, fence="~~~"),
    )
    for pseudo_form in (
        "inline",
        "indented",
        "comment",
        "escaped",
        "image",
        "quote-fence",
        "list-fence",
        "quote-indent",
        "list-indent",
    ):
        scenario(
            "topic-pair-backlink",
            lambda root, pseudo_form=pseudo_form: pseudo_paired_design_link(root, pseudo_form),
        )
    scenario(
        None,
        lambda root: pseudo_paired_design_link(root, "even-escaped"),
    )
    scenario(
        "topic-missing",
        lambda root: replace(
            root / "doc/product/world-rules-core-gameplay/prd.md",
            "(first-session-and-continuation.prd.md)",
            "(missing-first-session.prd.md)",
        ),
    )
    scenario(
        "topic-missing",
        lambda root: replace(
            root / "doc/product/world-infrastructure/prd.md",
            f"({FIXTURE_RETIRED_HISTORY.name})",
            "(missing-world-continuity.prd.md)",
        ),
    )
    scenario(
        None,
        lambda root: replace(
            root / "doc/core/prd.md",
            "# core PRD",
            "# core PRD\n\nProduct PRD-ID is reserved for module PRDs; this is a reference, not metadata.",
        ),
    )
    scenario(
        None,
        lambda root: replace(
            root / "doc/core/prd.md",
            "# core PRD",
            "# core PRD\n\n```markdown\n- 产品模块：`示例，不是声明`\n```\n\n~~~markdown\n- Product PRD-ID：`PRD-PRODUCT-999`\n~~~",
        ),
    )
    scenario(
        None,
        lambda root: replace(
            root / "doc/engineering/doc-governance/doc-structure-standard.design.md",
            "### 3.4 产品组合层的薄覆盖例外",
            "### 3.4 产品组合层的薄覆盖例外\n\n- Product PRD-ID：`PRD-PRODUCT-001`",
        ),
    )
    print("product-doc-governance-check.test: OK")


if __name__ == "__main__":
    main()
