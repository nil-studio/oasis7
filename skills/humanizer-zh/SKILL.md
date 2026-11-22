---
name: humanizer-zh
description: Use when editing Chinese text to reduce AI-written patterns, improve natural voice, remove promotional phrasing, and preserve the author intent without adding unsupported meaning.
allowed-tools:
  - Read
  - Write
  - Edit
  - AskUserQuestion
metadata:
  trigger: 编辑或审阅文本，去除 AI 写作痕迹
  source: 翻译自 blader/humanizer，参考 hardikpandya/stop-slop
---

# Humanizer ZH

## When to Use

Use this skill when:

- Chinese copy needs to sound more natural, less formulaic, and less AI-generated
- the task asks to remove AI traces, improve tone, or review Chinese text before publication
- a content or liveops slice needs concrete Chinese style anti-pattern checks

Do not use this skill when:

- the user asks for factual research, translation accuracy, or legal/medical/financial review rather than style editing
- the text must preserve a deliberately formal or institutional voice exactly as written

## Core Workflow

1. Preserve factual meaning, named entities, and author intent first.
2. Use the quick checklist below for routine edits; read `references/full-guidance.md` for the full pattern catalog and examples.
3. Return concise edited text plus only the highest-signal rationale when the user asks for explanation.

## Supporting Files

- `references/full-guidance.md`: detailed original guidance, examples, patterns, and command/reference material.

## Oasis7-Specific Surfaces

- Chinese source text supplied by the user or repo docs
- `references/full-guidance.md` full pattern catalog

## Known Failure Modes

- Removing AI smell by adding slang can damage trust; choose voice that matches the channel and author.
- Do not invent specificity or emotional stakes to make copy livelier.
- Keep punctuation and formatting compatible with the target channel.

## Guardrails

- Keep this entrypoint concise; move heavy examples or catalog material to supporting files.
- Follow `doc/engineering/workflow/source-of-truth.md` for language/writing standards, user authorization, and verification evidence; do not add separate task or role gates.
- Do not present reference material as verified project behavior without checking the current repo state.

## Verification

- Re-read the edited text against the original for meaning preservation.
- Run `./scripts/lint-skills.sh` after skill edits.
