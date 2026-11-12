# `doc/devlog` retired archive summary

更新时间: 2026-05-27

## Current Boundary
- `repository_health_engineer` maintains this retired historical entry and compatibility for old references.
- Reconsider this exception if daily devlogs are proposed again or execution-evidence authority changes. Retire the summary only when a more useful historical entry fully replaces it; it must never become current runtime truth.
- `doc/devlog` is no longer a runtime source of truth.
- The former daily files from 2026-02-03 through 2026-04-01 were summarized here and removed from the active repository surface.
- Current task truth and execution-evidence authority are defined by the [engineering workflow source of truth](../engineering/workflow/source-of-truth.md); this archive does not restate that mutable contract.
- Historical references that previously pointed to a specific daily file should use this summary as the compact archive pointer.

## Retired Corpus
| Window | Former files | Former lines | Summary |
| --- | ---: | ---: | --- |
| 2026-02 | 26 | about 28.1k | High-churn implementation and migration period covering Viewer UI/LLM iteration, runtime numeric correctness phases, P2P reward/storage/network hardening, site/static-doc updates, and early documentation migration. |
| 2026-03 | 30 | about 13.9k | Governance and release-readiness period covering strict doc schema migration, doc tree reorganization, launcher blockchain/explorer/usability work, release-candidate evidence, liveops/readme work, task-worktree governance, and P2P/shared-network follow-through. |
| 2026-04 | 1 | 288 | PM workflow/self-evolution cleanup and transition from daily devlog truth toward GitHub task issue evidences. |

Total retired daily files: 57. Former line count: 42,309.

## February 2026 Summary
- Viewer work dominated the first half of the month: copyable text, `bevy_egui` right panel migration, CJK font handling, observation-oriented panel polish, 2D/3D camera switching, screenshot loops, visual regression setup, and live Viewer/LLM control checks.
- LLM agent work moved from JSON-text decisions toward stricter tool-only and repair-loop contracts, with repeated `llm_bootstrap` online samples and factory/data-production goals used as the proving path.
- Runtime and P2P hardening work covered numeric correctness phases, replication writer and sequencer overflow semantics, snapshot restore validation, reward/runtime production hardening, storage/redeemable-power assets, blockchain/P2PFS hardening, and distfs self-healing.
- Documentation work started the large migration from old free-form documents to PRD/project structure, including topic pairing, project state updates, and repeated `doc-governance-check.sh` validation.
- Site/manual/static-doc work updated GitHub Pages, viewer manual migration, release-download communication, and static documentation mirrors.

## March 2026 Summary
- Early March established the modern documentation structure: module `prd.md/design.md/project.md`, topic directories, PRD indexes, strict schema checks, and role/handoff governance.
- Launcher and world-simulator work focused on blockchain transfer/explorer APIs, public-chain explorer P0/P1, launcher usability, self-guided onboarding, preflight/error cards, request-domain splitting, config dirty-state protection, and main-file modularization.
- Core governance rounds reviewed document responsibility boundaries, design backfill needs, path migrations, source-of-truth alignment, and release-candidate evidence boards.
- Runtime/world-runtime work continued numeric/storage governance, release-candidate evidence, wasm/module observability, and runtime candidate soak handoffs.
- Liveops/readme/game/playability work covered limited preview runbooks, closed-beta evidence templates, release messaging, Moltbook/Xiaohongshu drafts, and gameplay gate evidence.
- Late March shifted process truth away from daily devlogs and toward task UID identity, worktree lifecycle scripts, role-scoped PM evidence, and task-local execution logs; GitHub task issue evidence comments are the current task-local execution sink after the Step 3 migration.

## April 2026 Summary
- The remaining daily entry captured PM/self-evolution cleanup and confirmed that daily devlog status could drift from PM truth.
- This window was the transition point away from daily devlogs; after the GitHub Project Step 3 migration, GitHub task issue evidence comments are the durable execution record.

## Former Hotspots
| Former file | Former lines | Main reason it was noisy |
| --- | ---: | --- |
| `2026-02-16.md` | 3288 | Viewer/LLM and runtime bridge work accumulated many implementation/test entries in one day. |
| `2026-02-17.md` | 2812 | LLM tool-only migration and P2P/blockchain hardening produced repeated task-log blocks. |
| `2026-02-23.md` | 2426 | Runtime numeric correctness phases and distfs hardening generated long repeated closure entries. |
| `2026-02-12.md` | 1776 | Site/viewer/static-doc iteration and validation records were densely appended. |
| `2026-02-27.md` | 1619 | Documentation governance and viewer/live archive cleanup concentrated many migration notes. |
| `2026-03-08.md` | 1480 | Launcher explorer/usability/self-guided work produced many task-by-task implementation entries. |
| `2026-03-10.md` | 1311 | Governance rounds, release evidence, runtime/viewer handoffs, and QA records converged. |

## Usage
- For current status, follow the canonical workflow authority linked above; do not use this page as current truth.
- For historical orientation, use the monthly summaries above, then follow the referenced module/topic documents.
- Do not add new daily files under `doc/devlog`; use the current execution-evidence sink defined by the canonical workflow instead.
