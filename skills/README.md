# Non-Default Specialist Skill Library

Root `skills/` stores specialist guidance that should remain referenceable
but not be default-loaded as an active oasis7 skill entrypoint.

Use this surface for:

- professional method skills referenced by role cards
- retired or conditional specialist skills
- channel-specific playbooks and analysis guides
- upstream or long-form reference material that should not trigger by default

Default-loadable repo-owned skill entrypoints live under `.agents/skills/`.
Promoting material from `skills/` back into the default surface requires a
source-of-truth-first workflow change, README/role-card sync, and
`./scripts/lint-skills.sh`.

Current library entries:

- `agent-browser`: browser automation reference for Web/viewer evidence.
- `content-creation`: LiveOps, community, channel, release, and campaign copy.
- `epic-story-orchestrator-zh`: Chinese long-form story and lore-bible workflow.
- `game-architect`: game architecture planning and technical design.
- `game-design-theory`: theory-backed player motivation, loop, progression,
  and balance framing.
- `game-interaction-design`: player-facing flow, feedback, input, and UX review.
- `game-visual-design`: player-visible screen hierarchy, readability, and
  screenshot review.
- `gameplay-mechanics`: mechanics, combat, economy, progression, movement, and
  balance iteration.
- `gpt-image-2`: GPT Image 2 visual companion workflow.
- `humanizer-zh`: Chinese text naturalization and AI-pattern reduction.
- `level-design`: level pacing, spatial flow, encounters, and traversal.
- `memory-management`: memory, pooling, allocation, and asset streaming.
- [`oasis7-command-center`](oasis7-command-center/SKILL.md): project goals,
  current rules and decisions, Git/PR/CI delivery facts, and Codex development
  sessions for the oasis7 command dashboard.
- `optimization-performance`: profiling, frame rate, CPU/GPU, loading, and
  scalability optimization.
- `particle-systems`: particle effects and VFX tuning reference.
- `prd`: Product Requirements Document generation and improvement.
- `synchronization-algorithms`: authority, prediction, interpolation, and
  multiplayer state consistency.
- `xiaohongshu-note-analyzer`: XiaoHongShu / RedNote note analysis reference
  for explicit LiveOps or readme-governance opt-in work.
