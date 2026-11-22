---
name: game-architect
description: Use when planning a new game architecture, major game feature, refactor, or technical design document that needs structured requirement, technical design, and implementation-plan outputs.
---

# Game Architect

## Oasis7 Workflow Binding

In oasis7, this skill is a specialist architecture-planning aid, not a second project workflow. Follow `doc/engineering/workflow/source-of-truth.md`: the responsible person or main Agent may analyze, design and implement directly, using professional roles when needed. Keep architecture conclusions within the relevant professional authority; this skill adds no task, worktree, TPM or handoff prerequisite.

Maintain durable requirements and design in the existing product or professional authority. Record execution and review in the actual PR or an existing Task; Issue and Project are optional.

## When to Use

Use this skill when:

- a game architecture or major feature needs requirement, technical design, and implementation-planning documents
- the task needs explicit selection among architecture paradigms or system-specific references
- the output is documentation that will later feed implementation through the oasis7 workflow chain

Do not use this skill when:

- the user is asking for direct implementation with already-sufficient repo truth
- the question is a narrow gameplay, visual, runtime, viewer, or QA judgment owned by another role slice

## Core Workflow

1. Identify the problem, scope and relevant existing requirements/design. Read-only analysis does not require a task, branch, worktree or remote evidence comment.
2. Use this `SKILL.md` as the entrypoint; read `references/full-guidance.md` for document templates, phase details, paradigm tables, and examples.
3. Maintain architecture conclusions in the relevant existing PRD/design; use the templates only where they fit the requested deliverable.
4. Keep execution plans, remaining work and review results in the actual PR or existing Task, without copying mutable status into long-term documents.
5. If implementation is authorized, continue under the development workflow and its applicable verification and independent-review rules; a TPM handoff or separate router skill is not required.

## Supporting Files

- `references/full-guidance.md`: detailed original guidance, examples, patterns, and command/reference material.

## Oasis7-Specific Surfaces

- `architect/requirement.md`
- `architect/technical_design.md`
- `architect/implementation.md`
- `doc/engineering/workflow/source-of-truth.md`

## Known Failure Modes

- Architecture docs can drift into a second workflow; keep long-term contracts in their existing authority and delivery facts in Git, PR, actual CI and review records.
- Do not let this skill own runtime, gameplay, viewer, QA, or visual conclusions without the matching professional slice.
- Reference files are conditional; load only the system references that match the requested architecture problem.

## Guardrails

- Keep this entrypoint concise; move heavy examples or catalog material to supporting files.
- Follow `doc/engineering/workflow/source-of-truth.md` for authority docs, user authorization, and verification evidence; do not add separate task or role gates.
- Do not present reference material as verified project behavior without checking the current repo state.

## Verification

- Confirm generated doc paths and referenced files exist.
- Run `./scripts/lint-skills.sh` after skill-surface edits.
