---
name: memory-management
version: "2.0.0"
description: Use when optimizing game memory usage, object pooling, allocation behavior, garbage collection, memory profiling, asset streaming, or platform memory budgets.
sasmp_version: "1.3.0"
bonded_agent: 02-game-programmer
bond_type: PRIMARY_BOND

parameters:
  - name: platform
    type: string
    required: false
    validation:
      enum: [pc, console, mobile, vr, web]
  - name: issue_type
    type: string
    required: false
    validation:
      enum: [leak, fragmentation, gc_spikes, budget_exceeded]

retry_policy:
  enabled: true
  max_attempts: 3
  backoff: exponential

observability:
  log_events: [start, complete, error, gc_collect]
  metrics: [heap_size_mb, gc_time_ms, allocation_rate, pool_usage]
---

# Memory Management

## When to Use

Use this skill when:

- a game/runtime task touches allocations, pooling, GC pressure, asset streaming, or platform memory budgets
- performance work needs memory profiling or memory-budget guidance
- a slice must reason about memory regressions or memory-related frame stutter

Do not use this skill when:

- the task is CPU/GPU performance without meaningful memory behavior
- the task is general Rust ownership cleanup without a runtime memory goal

## Core Workflow

1. Define the target platform, budget, and symptom before changing memory behavior.
2. Read `references/full-guidance.md` for pooling, GC, profiling, streaming, and checklist details.
3. Prefer measured baselines and post-change measurements over intuition.

## Supporting Files

- `references/full-guidance.md`: detailed original guidance, examples, patterns, and command/reference material.

## Oasis7-Specific Surfaces

- runtime/performance profiling evidence
- asset loading or pooling code touched by the task
- `references/full-guidance.md` detailed patterns

## Known Failure Modes

- Pooling can hide leaks or stale state if reset semantics are incomplete.
- Lower allocation count is not automatically lower memory footprint; measure peak and steady-state separately.
- Streaming fixes can move stalls to IO or decode boundaries if prefetch timing is not verified.

## Guardrails

- Keep this entrypoint concise; move heavy examples or catalog material to supporting files.
- Follow `doc/engineering/workflow/source-of-truth.md` for authority docs, user authorization, and verification evidence; do not add separate task or role gates.
- Do not present reference material as verified project behavior without checking the current repo state.

## Verification

- Run the relevant memory/performance profile or focused regression command.
- Run `./scripts/lint-skills.sh` after skill edits.
