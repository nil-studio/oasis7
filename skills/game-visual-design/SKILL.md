---
name: game-visual-design
description: Use when an oasis7 change creates or modifies visual/player-visible game UI content, including screen hierarchy, readability, contrast, composition, density, motion emphasis, screenshot review criteria, or visual reference workflows for viewer, web, launcher, or gameplay screens.
---

# Game Visual Design

Use this skill to turn a player-facing screen into concrete visual hierarchy findings, visual acceptance criteria, or a design brief.

## When to Use

Use this skill when the task changes visual or player-visible content, especially when:

- a task changes visible UI/game presentation, HUD-like surfaces, overlays, menus, maps, boards, or status panels
- screenshot review needs criteria for hierarchy, contrast, density, scale, typography, color, spacing, or motion emphasis
- visual references or target images are useful but must stay bounded by oasis7 runtime and originality constraints
- `game_visual_interaction_designer` is reviewing a visual/player-facing change before handing a brief to `viewer_engineer` or residual visual risk to `qa_engineer`

Do not use this skill when:

- the task is a non-visual workflow, repository, runtime, or prose-only judgment with no visual/player-visible content change
- the question is primarily input flow, controls, micro-interaction states, or error recovery; use `game-interaction-design`
- the question is gameplay rule clarity or balance; involve `gameplay_designer`
- the question is frontend implementation detail, browser automation, or renderer feasibility; involve `viewer_engineer`

## Core Workflow

1. Name the visual job:
   - what the player must notice first
   - what is actionable, ambient, historical, dangerous, or blocked
   - which runtime/viewer data drives the visible state
2. Review hierarchy and readability:
   - scale and position make the primary action/state obvious
   - contrast separates interactive, selected, disabled, warning, and background layers
   - typography and density support scanning without oversized hero treatment inside tool surfaces
   - color, icons, and motion do not carry critical meaning alone
3. Review composition and game fit:
   - screen layout supports the current play loop rather than generic SaaS or marketing composition
   - visual emphasis follows gameplay/state priority, not decoration
   - target images or references are translated into oasis7-specific constraints, not copied
4. Produce visual acceptance criteria:
   - required screenshots or videos, viewport sizes, and states
   - comparison notes against target/reference material when used
   - motion intensity or reduced-motion risk for animated emphasis when relevant
   - residual risk if no real viewer/browser/native screenshot is available

## Source Map

Read `references/source-map.md` when adapting this skill, citing external rationale, or deciding whether a new visual rubric item belongs here.

Borrowed as inputs:

- visual hierarchy, scale, balance, contrast, and Gestalt principles
- game accessibility readability criteria
- platform game-screen guidance
- game UI reference taxonomy

Rejected as defaults:

- copying reference UI assets or style treatments
- using generated target images as proof of implementation
- treating non-game design systems as oasis7 visual direction

## Oasis7-Specific Surfaces

- role owner for professional conclusions: `.agents/roles/game_visual_interaction_designer.md`
- implementation handoff owner: `.agents/roles/viewer_engineer.md`
- verification handoff owner: `.agents/roles/qa_engineer.md`
- browser evidence skill: `skills/agent-browser/SKILL.md`
- workflow evidence sink: the actual PR or existing Task, following `doc/engineering/workflow/source-of-truth.md`; Issue and Project are optional

## Known Failure Modes

- Letting the screen become a pretty composition that hides the next player action.
- Treating a reference board, HUD, or menu as a license to copy another game's assets or style.
- Claiming visual completion from Image2/target images without real rendered screenshots.
- Using contrast or color rules without checking actual game state, viewport, and density.

## Guardrails

- Treat this as mandatory only for visual or player-visible content changes; for other `game_visual_interaction_designer` participation, use it only when the slice contract names a visual risk.
- Keep visual findings separate from gameplay-rule findings and implementation feasibility.
- Do not create fake affordances or decorative state that runtime/viewer data cannot support.
- Do not let visual references replace original oasis7 direction, screenshot evidence, or QA review.
- If implementation follows from the brief, name the minimum view states and screenshot set that can verify it.

## Verification

- Minimum skill-surface checks after editing this skill:
  - `./scripts/lint-skills.sh`
  - `./scripts/doc-governance-check.sh`
  - `git diff --check`
- Expected result:
  - the skill stays concise, links only existing supporting files, and preserves the role/verification boundary.
