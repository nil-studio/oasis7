---
name: agent-browser
description: Use when automating browser interaction with the local agent-browser CLI, including navigation, forms, screenshots, extraction, authentication, responsive checks, or visual diff workflows.
allowed-tools: Bash(agent-browser:*)
---

# Browser Automation

## When to Use

Use this skill when:

- a task needs scripted browser navigation, clicking, typing, screenshots, extraction, or visual comparison
- a local or external web app needs repeatable UI verification outside the in-app Browser plugin path
- authentication, session persistence, viewport emulation, or downloaded artifacts are part of the browser task

Do not use this skill when:

- the Codex in-app Browser plugin is explicitly requested or is the better local target surface
- a direct API, fixture, or unit test can verify the behavior more cheaply than browser automation

## Core Workflow

1. Verify the repository baseline with `agent-browser --version` (`0.37.1` at this audit) and load the version-matched core guide before guessing flags:
   `agent-browser skills get core --full`.
2. Run `agent-browser doctor --offline --quick --json` before a browser loop. Resolve install or daemon failures before UI actions.
3. Create one owned, worktree-scoped session per run and pass it explicitly to every command:
   `AB_SESSION="$(agent-browser session id --scope worktree --prefix <purpose>)"`.
   Record `agent-browser --session "$AB_SESSION" session info --json`; inspect parallel sessions with `agent-browser session list --json`.
4. Use `--headed open` when the task is visual or player-facing, then wait with `wait --load <state>` and an app signal such as `wait --fn` or `wait --text` before taking refs or acting.
5. Install an EXIT/INT/TERM trap that closes only `$AB_SESSION`. Never use the default session or the close command's global all-sessions option; a headed session is not covered by idle cleanup.
6. Read `references/full-guidance.md` only for the current command family you need, and record browser commands and observable results in the actual PR/existing Task when the browser check is task evidence. Read-only analysis does not require a remote comment; retain applicable original evidence and do not infer permission for external writes.

## Supporting Files

- `references/full-guidance.md`: concise `agent-browser 0.37.1` command patterns; start with the CLI's version-matched `skills get core --full` output when a flag is not listed.
- `references/session-management.md`: owned sessions, diagnostics, restore persistence, and scoped cleanup.
- `templates/`: reusable named-session scripts with trap cleanup; use them as starting points instead of copying unnamed examples.

## Oasis7-Specific Surfaces

- `agent-browser 0.37.1` CLI; version-matched authority is `agent-browser skills get core --full`
- named session lifecycle in `references/session-management.md` and `doc/testing/manual/web-ui-agent-browser-closure-manual.manual.md`
- `AGENTS.md` UI Web closed-loop constraints
- `testing-manual.md` S6 when working on oasis7 UI flows
- Execution-record placement and lifecycle ownership remain defined by `doc/engineering/workflow/source-of-truth.md`; this skill does not replace that workflow.

## Known Failure Modes

- Refs from a snapshot are not durable after navigation or DOM changes; re-snapshot before using stale refs.
- Saved auth or browser state can hide first-run bugs; use a clean session when validating onboarding behavior.
- Screenshots alone are weak evidence for data/state changes; pair them with text extraction, diffing, or app-visible state.
- A CLI/skill version mismatch can make a copied flag stale; verify `--version` and reload the bundled core skill.
- `Resource temporarily unavailable` or another daemon/IPC failure is a diagnostic blocker: retain `session info`, rebuild only the owned session, and do not replay non-idempotent actions.

## Guardrails

- Keep this entrypoint concise; move heavy examples or catalog material to supporting files.
- Follow `doc/engineering/workflow/source-of-truth.md` for branch/worktree isolation, user authorization, and verification evidence; do not add separate task or role gates.
- Do not present reference material as verified project behavior without checking the current repo state.
- Do not use unversioned `npx agent-browser`, the unnamed/default session, the legacy session-name alias, or the close command's global all-sessions option.
- Persistence uses `--restore --restore-save auto` with an explicit restore check when possible; legacy JSON state-file snippets are compatibility-only and must not be the main workflow.

## Verification

- Run the exact browser command used for the claim and inspect the returned snapshot/screenshot/diff.
- For skill edits, run `./scripts/lint-skills.sh`, `./scripts/doc-governance-check.sh`, `git diff --check`.
