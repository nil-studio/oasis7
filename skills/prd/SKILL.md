---
name: prd
description: Use when generating or improving Product Requirements Documents with executive summaries, user stories, technical specifications, acceptance criteria, and risk analysis.
license: MIT
---

# Product Requirements Document (PRD)

## Overview

Design comprehensive, production-grade Product Requirements Documents (PRDs) that bridge the gap between business vision and technical execution. This skill works for modern software systems, ensuring that requirements are clearly defined.

## Oasis7 Workflow Binding

In oasis7, this skill is a specialist planning surface governed by [the development workflow](../../doc/engineering/workflow/source-of-truth.md). The responsible owner may perform the work directly and use TPM or professional roles as needed; a fixed role chain, task Issue, or worktree registration is not a planning prerequisite.

- Write PRD outputs to repo-owned docs such as `doc/<module>/prd.md` or `doc/**/**.prd.md`.
- Record execution scope, handoff and actual evidence in the existing Task/PR; an Issue is optional. A read-only analysis or draft does not require a remote evidence comment.
- PRD-only output is not proof of implementation readiness or delivery. Check the actual acceptance gaps, applicable system-design obligations and verification entry.
- Check existing requirements, design, PR/Issue evidence and handoff before asking the user for more information. Ask only about unresolved product decisions or acceptance gaps that would materially change the PRD.
- Trace refs: O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6. Follow [the global requirements-trace contract](../../doc/engineering/doc-governance/cross-layer-requirements-traceability.design.md#global-requirement-trace): for new or substantively changed stable requirements, reuse one globally declared Trace ID across the requirement, applicable system design and Task (which may be the PR). Preserve local PRD/REQ/AC IDs and exact clause links; explain genuinely inapplicable or not-yet-formed layers. Do not bulk-backfill untouched content or add a workflow gate. The [read-only trace CLI](../../scripts/requirement-trace.README.md) provides bounded queries and rebuildable indexes; full-text search and manual link following remain useful. Its diagnostics are not completeness or completion proofs. For each new or substantively changed product feature, follow the [independent design/implementation status contract](../../doc/engineering/doc-governance/product-documentation-standard.design.md#feature-design-implementation-status): preserve exact design and Task/evidence links, remaining scope, and review baseline/environment/acceptance scope. Recheck statuses when requirements change; keep detailed execution history in Task and preserve existing trace-table columns.

## When to Use

Use this skill when:

- Starting a new product or feature development cycle
- Translating a vague idea into a concrete technical specification
- Defining requirements for AI-powered features
- Stakeholders need a unified "source of truth" for project scope
- User asks to "write a PRD", "document requirements", or "plan a feature"

---

## Operational Workflow

### Phase 1: Discovery (Repo Truth First)

Before writing or changing PRD content, inspect the existing work context and relevant repo
truth to fill known context. Do not assume missing product intent, but do not
re-ask questions already answered by GitHub-backed task truth, PRD, project, handoff, or
source-of-truth evidence.

**Check, then ask only if still unresolved:**

- **The Core Problem**: Why are we building this now?
- **Success Metrics**: How do we know it worked?
- **Constraints**: Budget, tech stack, or deadline?

### Phase 2: Analysis & Scoping

Synthesize the user's input. Identify dependencies and hidden complexities.

- Map out the **User Flow**.
- Define **Non-Goals** to protect the timeline.

### Phase 3: Technical Drafting

Generate the document using the **Strict PRD Schema** below.

---

## PRD Quality Standards

### Requirements Quality

Use concrete, measurable criteria. Avoid "fast", "easy", or "intuitive".

```diff
# Vague (BAD)
- The search should be fast and return relevant results.
- The UI must look modern and be easy to use.

# Concrete (GOOD)
+ The search must return results within 200ms for a 10k record dataset.
+ The search algorithm must achieve >= 85% Precision@10 in benchmark evals.
+ The UI must follow the 'Vercel/Next.js' design system and achieve 100% Lighthouse Accessibility score.
```

### Developer-Ready Completeness (Mandatory)

A PRD is not "ready for implementation" unless it is detailed enough that engineering and QA can execute without oral补充.

You MUST explicitly cover:

- **Functional Spec Depth**: interaction flow, field definitions, button behavior, state transitions, sorting/computation rules, permission logic.
- **Edge Cases**: network failure, empty data, permission denied, timeout, concurrency conflict, data corruption fallback.
- **NFR with Targets**: performance, compatibility, security, expected data scale, extensibility constraints.
- **Testability**: acceptance criteria, done definition, validation method, regression impact scope, and requirement -> applicable system design -> Task traceability with actual test evidence; retain local PRD/REQ/AC IDs alongside the shared Trace ID.
- **Decision Record**: chosen approach, rejected alternatives, and evidence/rationale.

### Game Design Bible Completeness (Oasis7)

When the PRD work targets `doc/game` or gameplay topics and the goal is to
express all game details, use
`doc/engineering/workflow/source-of-truth.md`.

Do not turn the root PRD into a catch-all manual. Keep root `doc/game/prd.md`
as the active gameplay baseline and route details into topic PRD/design documents.

Each detailed gameplay PRD or promoted topic must cover:

- **Player Promise**: what experience the player is being promised, and which
  release or preview claim boundary it may affect.
- **Player Verbs**: what the player can do, what the system may refuse or
  redirect, and what feedback is required.
- **Loops and Timing**: micro-loop, first 10/30 minutes, first capability,
  mid-loop, and long-loop expectations where relevant.
- **State and Economy Rules**: resources, costs, ownership, cooldowns, upkeep,
  failure states, recovery paths, and anti-abuse / anti-grind constraints.
- **Feedback Surfaces**: which viewer/API/agent/runtime surfaces must show
  acceptance, progress, blockers, consequences, next steps, and replay/resume
  anchors.
- **Balance Risks**: exploit paths, dominance risks, forced dependency risks,
  world-activity-only false positives, and player-leverage checks.
- **Validation Matrix**: PRD-GAME id, owner role, required/full evidence tier,
  sample scenario, acceptance signal, and what remains unproven.

### Document Boundary (PRD vs Execution Records)

Use the following split and avoid duplicate definitions:

| Doc | Focus | Must Not Include |
| --- | --- | --- |
| PRD (`doc/<module>/prd.md`, `doc/**/**.prd.md`) | `Why/What/Done` (scope, behavior, acceptance, NFR, risks, decisions) | task breakdown, daily progress |
| Existing Task/PR (optional Issue or Project view) | `How/When/Who` (tasks, dependency, owner, status, execution plan) | rewriting target-state requirements |
| Git, PR, actual CI, and review records | implementation and verification evidence; link the relevant Task/Issue when used | a parallel progress ledger or a source of new requirements |

The [retired devlog summary](../../doc/devlog/README.md) is for historical lookup only. Do not create daily devlog files.

Guardrails:
- Product feature design/implementation status summaries are allowed next to their requirements; retain evidence links and baseline, and keep detailed execution history in Task/PR. They are not a second progress ledger.
- oasis7 content-check findings follow the [incremental CI boundary](../../doc/engineering/doc-governance/product-documentation-standard.design.md#advisory-content-checks); findings in new or substantively changed PR documents block CI, while untouched legacy content remains advisory. Input/execution errors and existing integrity checks remain blocking. Do not treat warnings as semantic acceptance.
- `Critical User Flows` describe system/user/operator runtime behavior, not "rewrite/migrate docs" steps.
- `Acceptance Criteria` describe verifiable outcomes (behavior/invariants/metrics/tests), not file existence.
- `Roadmap/Milestones` describe shipped capability phases (MVP/v1.1/v2.0), not daily to-do updates.
- Detailed game design belongs in topic PRD/design documents and indexed
  supplements; execution stays in the existing Task/PR. Root docs should route and summarize current truth, not absorb
  all topic matrices.

Quick contrast:
- Bad: `Critical User Flows: read old doc -> rewrite -> update project doc -> verify commit`
- Good: `Critical User Flows: user triggers X -> system validates -> state transitions -> user sees Y`

For applicable quality questions and risk assessment, use `check.md`; it does not define a separate workflow gate.

---

## Strict PRD Schema

You **MUST** follow this exact structure for the output:

### 1. Executive Summary

- **Problem Statement**: 1-2 sentences on the pain point.
- **Proposed Solution**: 1-2 sentences on the fix.
- **Success Criteria**: 3-5 measurable KPIs.

### 2. User Experience & Functionality

- **User Personas**: Who is this for?
- **User Scenarios & Frequency**: When and how often each persona uses this feature.
- **User Stories**: `As a [user], I want to [action] so that [benefit].`
- **Critical User Flows**: Step-by-step flow for key paths.
- **Functional Specification Matrix**: fields, button actions, state transitions, sorting/computation rules, and permission behavior. In oasis7 product documents, keep each touched feature’s independent design/implementation status card adjacent to its feature/requirement; do not add columns to existing fixed trace tables.
- **Acceptance Criteria**: Bulleted list of "Done" definitions for each story.
- **Non-Goals**: What are we NOT building?

### 3. AI System Requirements (If Applicable)

- **Tool Requirements**: What tools and APIs are needed?
- **Evaluation Strategy**: How to measure output quality and accuracy.

### 4. Technical Specifications

- **Architecture Overview**: Data flow and component interaction.
- **Integration Points**: APIs, DBs, and Auth.
- **Edge Cases & Error Handling**: network, timeout, empty/invalid data, concurrency, fallback behavior.
- **Non-Functional Requirements**: measurable performance, compatibility, security/privacy, scale, extensibility targets.
- **Security & Privacy**: Data handling and compliance.

### 5. Risks & Roadmap

- **Phased Rollout**: MVP -> v1.1 -> v2.0.
- **Technical Risks**: Latency, cost, or dependency failures.

### 6. Validation & Decision Record

- **Test Plan & Traceability**: requirement Trace ID -> applicable system-design clause -> existing Task/PR, with actual `test_tier_required` / `test_tier_full` evidence where applicable. Explain missing or inapplicable layers; a shared ID alone does not prove completion.
- **Decision Log**: selected option, rejected options, and rationale/evidence.

---

## Implementation Guidelines

### DO (Always)

- **Define Testing**: For AI systems, specify how to test and validate output quality.
- **Iterate**: Present a draft and ask for feedback on specific sections.
- **Specify Edge Conditions**: Include failure and boundary handling, not only happy path.
- **Quantify NFRs**: Every non-functional constraint should be measurable.
- **Record Decisions**: Capture tradeoffs and why alternatives were rejected.

### DON'T (Avoid)

- **Skip Discovery**: Never write a PRD without checking repo truth first; ask concise clarifying questions only when material gaps remain.
- **Hallucinate Constraints**: If the user didn't specify a tech stack, ask or label it as `TBD`.
- **Ship Ambiguous Specs**: If implementation still needs oral clarification, PRD is incomplete.

---

## Example: Intelligent Search System

### 1. Executive Summary

**Problem**: Users struggle to find specific documentation snippets in massive repositories.
**Solution**: An intelligent search system that provides direct answers with source citations.
**Success**:

- Reduce search time by 50%.
- Citation accuracy >= 95%.

### 2. User Stories

- **Story**: As a developer, I want to ask natural language questions so I don't have to guess keywords.
- **AC**:
  - Supports multi-turn clarification.
  - Returns code blocks with "Copy" button.

### 3. AI System Architecture

- **Tools Required**: `codesearch`, `grep`, `webfetch`.

### 4. Evaluation

- **Benchmark**: Test with 50 common developer questions.
- **Pass Rate**: 90% must match expected citations.

# Check written PRD
Use the following checklist to evaluate the quality of a written PRD. 
`check.md` is a professional quality checklist for business goals, users, scope, functional specifications, edge cases and testability. Apply relevant questions to the changed scope; it does not create a separate development gate.

## Review and Risk Assessment

Self-review the applicable requirements and risks after drafting. Record the reviewed version, conclusion, genuine blockers and their disposition in the actual PR/existing Task as appropriate. Item-by-item marks, scores and color summaries are optional aids, not mandatory deliverables or readiness proofs.

Missing requirements, undefined boundaries, untestable acceptance criteria, correctness and safety gaps must be resolved or factually shown inapplicable before dependent implementation proceeds. Assess their actual impact under the development workflow and applicable product/system contracts; do not use a checklist color as a blanket stop for unrelated authorized work.

Explain genuine N/A scope and important uncovered obligations. This checklist adds no N/A approval step. Non-blocking suggestions may be left unchanged with reasons; they do not require automatic follow-up tasks, owners/deadlines or deferral forms. Applicable authority and independent-review requirements remain in force.

## Guardrails

- Keep PRDs aligned with repo truth and acceptance evidence.
- Do not let generated PRD content replace GitHub-backed task truth in oasis7.
