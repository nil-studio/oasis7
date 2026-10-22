---
name: game-interaction-design
description: Use when an oasis7 change creates or modifies player-visible interaction behavior, including player-facing flows, input/control behavior, UI state feedback, error recovery, accessibility implications, or game UX heuristics for viewer, web, launcher, or gameplay screens.
---

# Game Interaction Design

Use this skill to turn a player-facing flow into concrete interaction findings, implementation briefs, or review criteria.

## When to Use

Use this skill when the task changes player-visible interaction content, especially when:

- a task touches player actions, click/drag/select/command flows, controls, or input affordances
- UI states need feedback rules for hover, focus, pressed, selected, disabled, loading, success, failure, or blocked actions
- a viewer/web/gameplay screen needs first-time usability, error recovery, or accessibility review
- `game_visual_interaction_designer` is reviewing a visual/player-facing interaction change before handing a brief to `viewer_engineer` or residual risk to `qa_engineer`

Do not use this skill when:

- the task is a non-visual workflow, repository, runtime, or prose-only judgment with no player-visible interaction change
- the question is visual style only; use `game-visual-design`
- the question is gameplay balance, economy, progression, or player-verb semantics; involve `gameplay_designer`
- the question is implementation feasibility or browser automation; involve `viewer_engineer` and use `agent-browser` as needed

## Core Workflow

1. State the player flow under review:
   - player intent
   - entry point and exit point
   - input mode and device assumptions
   - runtime/viewer state the UI is allowed to reveal
2. Check the interaction contract:
   - the player can tell what is interactive, what is selected, what is blocked, and why
   - every command has a timely acknowledgement, result, and recovery path
   - destructive, irreversible, or expensive actions have an appropriate confirm/undo/cancel story
   - states are recognizable without relying only on memory, tiny text, color, or hidden hover behavior
3. Check accessibility and platform fit:
   - keyboard, pointer, touch, and reduced-motion expectations are explicit when relevant
   - text size, contrast, target size, and timeout assumptions are not hostile to common player constraints
   - platform-specific guidance is treated as input, not universal law
4. Produce an interaction brief:
   - required states and feedback timing
   - blocked/error copy intent, not final marketing text
   - acceptance evidence: real interaction smoke, screenshot/video, DOM/browser evidence, or explicit unverified risk

## Recovery-State Review

For the affected journey, distinguish transport acknowledgement, canonical request result, and business outcome. An unknown result needs a way to inspect the original request, not a generic retry that creates a new intent. A cognition commit is not proof that a manufacturing action succeeded; a result missing from a recent window is not proof of failure. Use the applicable contract rather than inventing status names or promises.

Exercise the relevant empty, unbound, pending, unknown, rejected and read-only recovery states. Check that copy explains what is known, whether an effect is confirmed, and the permitted next step. Include keyboard focus after errors/dismissal, status/error association, long text, narrow viewports and zoom where affected. A screenshot does not prove keyboard or screen-reader behavior, and Web evidence does not validate native controls. See the [bounded scenario reference](../../.agents/skills/systematic-debugging/references/skills-research-checks.md) for examples and source boundaries; it adds no mandatory full-matrix run.

## Source Map

Read `references/source-map.md` when adapting this skill, citing external rationale, or deciding whether a new checklist item belongs here.

Borrowed as inputs:

- game usability heuristics
- game accessibility checklists
- platform/game-control guidance
- UI interaction state patterns
- cognitive load and game UX framing

Rejected as defaults:

- treating accessibility references as compliance certification
- importing a non-game design system's visual language
- replacing runtime/viewer smoke with a heuristic review

## Oasis7-Specific Surfaces

- role owner for professional conclusions: `.agents/roles/game_visual_interaction_designer.md`
- implementation handoff owner: `.agents/roles/viewer_engineer.md`
- verification handoff owner: `.agents/roles/qa_engineer.md`
- browser loop skill: `skills/agent-browser/SKILL.md`
- visual companion skill: `skills/gpt-image-2/SKILL.md`
- workflow evidence sink: the actual PR or existing Task, following `doc/engineering/workflow/source-of-truth.md`; Issue and Project are optional

## Known Failure Modes

- Reviewing a static screenshot and claiming the interaction is validated.
- Applying Material, Apple, or Xbox patterns as style law instead of extracting interaction principles.
- Letting accessibility guidance become a huge generic checklist that hides the current player flow.
- Defining UI feedback that cannot be backed by real runtime/viewer state.

## Guardrails

- Treat this as mandatory only for visual or player-visible interaction changes; for other `game_visual_interaction_designer` participation, use it only when the slice contract names an interaction risk.
- Keep interaction findings separate from gameplay-rule findings.
- Do not promise keyboard, touch, controller, or screen-reader support unless the implementation and QA evidence cover it.
- Do not let target images, reference videos, or design-system examples replace real player-facing smoke evidence.
- If implementation follows from the brief, name the minimum states that `viewer_engineer` must implement and `qa_engineer` can verify.

## Verification

- Minimum skill-surface checks after editing this skill:
  - `./scripts/lint-skills.sh`
  - `./scripts/doc-governance-check.sh`
  - `git diff --check`
- Expected result:
  - the skill stays concise, links only existing supporting files, and preserves the role/verification boundary.
