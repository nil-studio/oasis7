---
name: particle-systems
version: "2.0.0"
description: Use when creating, tuning, optimizing, pooling, or troubleshooting particle effects, visual effects recipes, GPU particles, or engine-specific VFX tooling.
sasmp_version: "1.3.0"
bonded_agent: 03-graphics-rendering
bond_type: PRIMARY_BOND

parameters:
  - name: effect_type
    type: string
    required: false
    validation:
      enum: [explosion, fire, smoke, magic, weather, impact]
  - name: platform
    type: string
    required: false
    validation:
      enum: [pc, console, mobile, vr]

retry_policy:
  enabled: true
  max_attempts: 3
  backoff: exponential

observability:
  log_events: [start, complete, error]
  metrics: [particle_count, draw_calls, gpu_time_ms]
---

# Particle Systems

## Oasis7 Workflow Binding

In oasis7, this is a domain-triggered, non-default VFX aid. It supports
`game_visual_interaction_designer` and the relevant implementation slice when a
task actually changes player-facing effects, readability, or visual-performance
behavior. It does not replace visual direction, implementation ownership, QA
verification, or real screenshot/playtest evidence.

## When to Use

Use this skill when:

- a task creates or changes particle effects, VFX recipes, pooling, GPU particles, or visual performance
- visual feedback needs particle timing, readability, or performance guidance
- a game_visual_interaction_designer or implementation slice needs reusable VFX reference material

Do not use this skill when:

- the visual change is static UI styling with no particle/VFX behavior
- the task is pure gameplay logic without player-facing visual feedback
- the task only needs generic VFX education without an oasis7 visual or runtime
  decision

## Core Workflow

1. Start from the player-facing effect purpose: feedback, readability, mood, or performance.
2. Read `references/full-guidance.md` for architecture, recipes, Unity setup, GPU particles, pooling, troubleshooting, and tools.
3. Verify effects visually and with performance constraints on the target scene/viewport.

## Supporting Files

- `references/full-guidance.md`: detailed original guidance, examples, patterns, and command/reference material.

## Oasis7-Specific Surfaces

- VFX assets and effect code touched by the task
- visual/playtest evidence
- `references/full-guidance.md`

## Known Failure Modes

- A beautiful effect can still fail if it obscures gameplay state or target readability.
- Particle counts that pass on desktop may fail on target hardware; verify budget where the effect ships.
- Pooling effects requires full reset of lifetime, color, transforms, emitters, and callbacks.

## Guardrails

- Keep this entrypoint concise; move heavy examples or catalog material to supporting files.
- Follow `doc/engineering/workflow/source-of-truth.md` for authority docs, user authorization, and verification evidence; do not add separate task or role gates.
- Do not present reference material as verified project behavior without checking the current repo state.
- Do not treat a particle recipe as accepted visual direction without checking applicable visual design authorities and real rendering/playtest evidence.

## Verification

- Run the visual/performance check tied to the effect, including screenshot or playtest evidence when relevant.
- Run `./scripts/lint-skills.sh` after skill edits.
