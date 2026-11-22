---
name: gameplay-mechanics
version: "2.0.0"
description: Use when implementing or refining concrete gameplay mechanics, feedback loops, combat, economy, progression, movement, event interactions, or balance iteration.
sasmp_version: "1.3.0"
bonded_agent: 01-game-designer
bond_type: PRIMARY_BOND

parameters:
  - name: mechanic_type
    type: string
    required: false
    validation:
      enum: [combat, movement, puzzle, progression, economy]
  - name: complexity
    type: string
    required: false
    validation:
      enum: [simple, moderate, complex]

retry_policy:
  enabled: true
  max_attempts: 3
  backoff: exponential

observability:
  log_events: [start, complete, error]
  metrics: [engagement_time, action_frequency, balance_score]
---

# Gameplay Mechanics

## When to Use

Use this skill when:

- a task changes player verbs, mechanics, resource loops, combat, movement, progression, or balance behavior
- mechanic implementation needs feedback-loop, event, or tuning guidance
- a gameplay_designer or gameplay implementation slice needs reusable mechanics reference material

Do not use this skill when:

- the request is only high-level product strategy or architecture documentation
- the task is visual/UI presentation without changing gameplay rules or player verbs

## Core Workflow

1. Route through the appropriate professional gameplay slice before presenting gameplay conclusions.
2. Read `references/full-guidance.md` for detailed mechanics patterns, code examples, balance loops, and troubleshooting.
3. Tie each mechanic change to an observable player-facing behavior and a verification command or playtest note.

## Supporting Files

- `references/full-guidance.md`: detailed original guidance, examples, patterns, and command/reference material.

## Oasis7-Specific Surfaces

- `.agents/roles/gameplay_designer.md`
- Git, PR, actual CI and review records; an existing Task/Project may be used when needed under `doc/engineering/workflow/source-of-truth.md`
- gameplay/runtime tests or playtest evidence for changed mechanics

## Known Failure Modes

- A mechanic that compiles can still feel wrong; include feedback timing and player-readable state in acceptance.
- Balance changes without instrumentation or replayable scenarios are hard to verify later.
- Avoid changing economy/progression constants without documenting the expected player impact.

## Guardrails

- Keep this entrypoint concise; move heavy examples or catalog material to supporting files.
- Follow `doc/engineering/workflow/source-of-truth.md` for authority docs, user authorization, and verification evidence; do not add separate task or role gates.
- Do not present reference material as verified project behavior without checking the current repo state.

## Verification

- Run the focused gameplay/runtime test or playtest harness tied to the mechanic.
- Run `./scripts/lint-skills.sh` after skill edits.
