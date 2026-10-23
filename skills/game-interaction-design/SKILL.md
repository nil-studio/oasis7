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

## State Feedback and Recovery

Map feedback to the applicable runtime/API contract, not to invented status names. Distinguish transport acknowledgement, canonical request result, and business outcome. For the affected journey, use these questions to find missing states:

| Observed fact | Required clarity and permitted action | Invalid inference |
| --- | --- | --- |
| Transport acknowledgement / volatile Received | The transport responded; durable outcome is not yet confirmed | Game pending, task created, or business success |
| Contract-confirmed pending / accepted | Which layer accepted the request and what to wait for or query | Committed or delivered |
| Unknown / disconnected before final result | The outcome is unconfirmed; inspect the original request within current read authorization | Failed, or safe to submit a new intent |
| Explicit rejected / failed | The known reason, affected scope, and permitted next step | An invented server cause or automatic permission expansion |
| Read-only recovery | What can be read and what still cannot be done | Service or write readiness |
| Empty / unbound / filtered view | Whether there is no content, no binding, no matching result, or a read/permission problem | A read failure means an empty world or lost progress |

An unknown operation must not be turned into a generic retry that creates a new identity. Follow the original contract for lookup or safe resubmission; recovery does not renew an expired grant. A cognition commit does not prove manufacturing success, and a missing recent-window entry does not prove failure. Use [request-recovery checks](../../testing-manual.md#request-recovery-regression) for the affected boundary, without treating every row as a mandatory matrix for every UI edit.

Write error and blocked-state copy around what is known, what it affects, and the allowed next action. Preserve recoverable player input; do not expose raw exceptions, secrets, or private payloads. Only promise an action that the current authority and implementation allow.

## Check the Affected Journey

- Exercise relevant repeat clicks, interruption, refresh/re-entry, error followed by correction, and Close/Cancel paths. Verify that a late response cannot overwrite a newer selection or result, and that repeated actions do not create unintended duplicate effects.
- Navigate by keyboard through the changed controls; check visible focus, focus after errors/dismissal, an understandable control name, and association between fields and errors. Check state announcements with actual assistive technology before claiming screen-reader support.
- Check long text, narrow viewports, 200% zoom, and target reachability where affected. Distinguish viewport simulation from real touch/device testing; use platform-appropriate input guidance rather than assuming Web behavior validates native controls.
- Compare displayed state with the original request and authoritative business outcome. Record actual version, state and viewport under the existing verification rules for that surface: S6 for Web, and the applicable native/launcher entry for those surfaces. Screenshots alone do not validate input, recovery, or world effects.

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
