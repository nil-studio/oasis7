# Viewer Visual Design Specification (2026-06-05)

- Professional owner: `game_visual_interaction_designer`; integration owner: `tpm`.
- Historical source: `task_a25bf76359be45719edfcda1759626d1` / #1385; original execution evidence remains in Git/Task history.
- Contract revision reviewed against `a2389f437e6cedc3b870a91d33a1cdf40a00a8da`, 2026-10-16. This document does not assert that independent visual review or current browser acceptance has run.
- Companions: [brand system](viewer-brand-system-2026-06-05.design.md), [page/module design](viewer-page-module-design-2026-06-18.design.md), [terminal shell](viewer-gameplay-release-experience-overhaul.prd.md), [player-readable rendering](viewer-pixel-world-player-readable-rendering.prd.md).

## 1. Purpose

This is the canonical visual specification for Viewer/player-facing world-simulator surfaces: first-read hierarchy, world/command/context/diagnostics, truthful states and visual evidence. The visual owner retains judgment; documentation integration does not replace that role or imply a review was performed.

It consolidates durable directions without turning historical modes, screenshots or implementation slices into current capabilities. Product obligations come from [the world-stage PRD](../../product/agents-world-simulation/player-readable-world-stage.prd.md); this specification owns their visual realization and the existing review gate, not runtime truth.

## 2. Scope

In scope: canonical Viewer Web and its software_safe alias, the pixel-world stage, explicitly supported Rust-derived semantic/auxiliary DOM surfaces, contextual commands, chat/Prompt, receipts, blockers, empty/loading/error/recovery states and diagnostic disclosure. Site/document screenshots must follow this specification when they claim to represent the current Viewer.

Out of scope: runtime/economy/auth/chain rules, WASM ABI, large art packs, unrelated launcher/explorer/marketing redesigns, retention/public-support claims or release/required-review approval. The world-view system design owns technical platform/capacity and recovery contracts.

## 3. Source Documents Consolidated

Durable sources are the terminal-shell PRD (Player/Director, command discovery, current-versus-target), page/module design, brand system (tokens/assets/status vocabulary), semantic positioning (independent coordinate and relation authority), player-readable rendering (HUD/causality), [world-stage product design](../../product/agents-world-simulation/player-readable-world-stage.design.md) and [visual-review SOP](../../testing/manual/model-visual-review-sop-2026-05-29.manual.md).

Retired Viewer redesign and 2D/EGUI topics contribute historical intent only. They do not restore a second Viewer, independent JS world renderer, 3D mode or old map controls. Runtime, QA, security and release authority remains with those domains.

## 4. Product Visual North Star

The Viewer should feel like an industrial world command table, not a generic dark diagnostics dashboard. The player should identify the world, the relevant objective/actor/blocker/route, the next decision and the original action's outcome before opening diagnostics.

Preserve reading order before decoration or density. A busy world is not evidence of player progress. Viewing an object is not controlling it, and the currently viewed object need not be the subject of an outstanding action.

## 5. Terminal shell visual summary (2026-08-14)

The [terminal-shell contract](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary) owns Player/Director, dock, Search/Quote, contextual console and current implementation. Keep one world-first stage, command/receipt within one explicit step, and diagnostics secondary.

World Feed v1 remains ambient and separate from Action Receipt: a compact collapsed top-right edge overlay/chip in fullscreen Player, opened on demand. receipt_ref appears only for explicit Runtime identity. Focus/right-panel hooks and entity-search are compatibility references, not new product modes. No permanent three-column Player layout or three peer HUD cards is introduced here.

## 6. Non-Negotiable Principles

### 6.1 World First

The stage is the primary Player surface. Raw JSON, renderer/provider/governance badges and test controls never compete on first load. Unavailable graphics must have an honest cause and a safe route; they must not silently construct a second JS world.

### 6.2 Player Leverage Before Ambient Activity

Distinguish original player-caused progress, Agent autonomous activity and ambient simulation movement. Receipts answer what was attempted, what changed or did not change, why and what comes next. Selecting B while A's operation is pending cannot relabel A's result as B's progress, pull focus automatically or create another effect.

### 6.3 Diagnostics Are Available, Not Dominant

Director/QA tools remain discoverable and scriptable through secondary drawers/tabs/mode boundaries. Hiding them is acceptable only with a clear route back. Explicit Director use does not grant gameplay permissions or relax data honesty.

### 6.4 Data Honesty

Geometry provenance and freshness are distinct. A recently refreshed relationship layout is not an exact coordinate; an old authoritative coordinate is not current observation. Derived, last-known, unknown, conflict, partial and unavailable states need legible text/shape/context, not only a hidden diagnostic field.

Marker/halo enlargement improves readability but is not physical geometry. Visual emphasis, adjacency, labels, inferred routes and default framing do not imply ownership, capability, causality or health unsupported by authority. Revoked private information must disappear, not remain under a last-known label.

### 6.5 One Obvious Command Path

Keep primary command/chat one explicit step from the stage. A disabled or unbound action explains the reason and recovery in context; never make it executable by borrowing the first visible Agent. Observation may continue while unsupported action is blocked. Authorized stop/revocation/original-result lookup follow their own server contracts rather than a blanket renderer/freshness disable.

### 6.6 Sparse Worlds Still Communicate Relationships

Sparse snapshots must remain understandable through readable identities, location provenance, valid context and truthful text. Relationship/route lines appear only when their independent current authority is complete. A snapshot with only location_id may derive a display position but must not manufacture an assignment/route line.

“No confirmed relationship” is a legitimate outcome. Absence of a line is not a visual-review failure when the input lacks that relationship. Renderer failure uses the explicit text/Targets/receipt recovery contract, not a second DOM world; existing Rust-derived DOM fixtures prove only their declared semantic/auxiliary scope.

### 6.7 Readability Beats Decorative Density

Remove secondary labels, texture and decoration before shrinking the main decision surface. Terrain/Location anchors support, not overpower, Agent/goal/blocker/receipt. Dense/overlapping objects need a reliable accessible candidate-selection route; deterministic spatial scattering alone is not acceptance.

## 7. Visual Language

Use the [brand-system companion](viewer-brand-system-2026-06-05.design.md) for executable tokens, icon/status vocabulary, asset admission and screenshot matrix. This revision does not replace those values or introduce a parallel brand book.

### 7.1 Tone

Industrial, precise, command-oriented and playable. Technical content can exist without making the default surface a log viewer. Avoid equal-weight dark cards everywhere.

### 7.2 Surface Hierarchy

| Level | Purpose | Examples | Prominence |
| --- | --- | --- | --- |
| Stage | World comprehension | World canvas, selected-object context | Highest |
| Command | Decision and original feedback | Next Move, goal, command, receipt | High |
| Context | Explanation and orientation | Targets, details, relevant activity | Medium |
| Diagnostics | Engineering/QA | Raw DTO, renderer/provider state, tests | Low/collapsed |

### 7.3 Color Semantics

Retain world-background, terrain-subdued, agent-primary, route-active, objective-accent, blocker-danger, receipt-success, diagnostic-muted, panel-surface and focus-ring token roles. Color is semantic, not ornament. Critical meaning also has text, shape, icon or placement; receipt-success must never be attached to merely accepted/queued/ambient outcomes.

### 7.4 Typography and Copy Density

Retain world-title, objective-title, action-label, receipt-body and diagnostic-caption roles. Player headings are short and action-oriented; reserve large type for genuine primary subjects. Explain blockers and receipts without requiring raw state names. CJK glyphs must render; long labels use an accessible detail route rather than unreadably small text.

### 7.5 Shape and Layout

Strong alignment, consistent spacing and grouping are preferable to nested card walls. Reserve stable stage/toolbar/panel dimensions so loading, hover and errors do not move primary actions. Tooltips/callouts avoid the main command and receipt; when space is exhausted use adjacent accessible details instead of clipping the only explanation.

### 7.6 Status Vocabulary

Provide stable visual/text equivalents for selected, target, blocker, active route, accepted intent, executing, completed, completed-without-progress, no receipt, result awaiting recovery, derived position, last-known/conflict/unknown, partial coverage and unavailable/degraded renderer. These are presentation meanings, not a new runtime enum or permission system.

## 8. Information Architecture

### 8.1 Player Shell Default

Show world/stage, applicable objective/selected context, next action or blocker, original receipt/current operation feedback and command/chat discovery. Demote raw runtime data, renderer internals, provider/auth/governance, tests and long event lists. Empty/unbound contexts remain explicit rather than populated with another Agent's data.

### 8.2 Director Mode

Director is an explicit server-capability-bound operator/debug choice. Dense three-column tools may appear only there; no new permission, player progress or alternate world authority follows from the layout.

### 8.3 HUD, Dock, and Contextual Console Hierarchy

Follow the shell's single primary Next Move region and subordinate Objective/Player Leverage. Narrow screens place Next Move first and context second; blocker/receipt remain legible. World/Targets/Command are primary dock routes; Search stays inside Targets, Diagnostics secondary. Console order is command/chat, selected context, gameplay details, then raw diagnostics. The former three-peer-card HUD is not a current requirement.

### 8.4 Empty, Loading, Error, and Fallback Hierarchy

First show state and whether the relevant action is possible; then cause/missing dependency; then recovery/next observable signal; finally optional diagnostics. Distinguish no snapshot, no authorized objects, filter-empty, partially loaded and genuinely empty scope. Unknown is not zero inventory or nonexistent world.

### 8.5 Mobile and Narrow Screens

Preserve World/Targets/Command and optional Diagnostics without forcing the player to scroll past the whole world to find commands. Low-height/landscape, CJK, zoomed text and touch tooltips need explicit coverage. Layer/filter/selection changes do not alter task or original receipt identity.

## 9. Pixel-World Layering

Priority is decision context and original receipt/blocker, active/selected Agent, valid routes/relationships/hotspots/goal callouts, logical Location anchors, terrain/background, then diagnostics. This is reading priority, not a requirement that every item occupy a permanent panel.

Derived/missing/stale positions need visible scope and accessible explanation. Scale/LOD hides secondary detail first. If an old renderer cannot distinguish a stale coordinate from a current one, suppress that misleading dynamic visual and keep authorized text rather than silently upgrading it.

## 10. 2D Map Rules

These are target spatial-readability rules, not restoration of retired 2D/3D modes. The long-term top-down/orthographic industrial stage uses distinguishable entity symbols, progressive labels, valid flow direction and stable selected/goal/blocker context across scales.

Do not assume a current 2D/3D toggle, introduce cross-mode leakage requirements for nonexistent surfaces, or claim historical semantic zoom is shipped. Overview/local transitions, manual pan exiting follow, overlap selection and return-to-goal follow the [world-stage experience](../../product/agents-world-simulation/player-readable-world-stage.design.md#exploration-transitions). Flow arrows require supported authority, not adjacency or guessed motion.

## 11. Interaction and Feedback

### 11.1 Selection

Selection yields visible focus and readable context, preserves objective/next step, and never executes. Decorative terrain must not look actionable. Offscreen, filtered, not-loaded, stale, revoked and deleted objects have different explanations. Revoked details disappear; an unavailable target is not replaced with another Agent.

### 11.2 Command

Command/chat is discoverable without diagnostics. Unavailable commands explain why; original submitted subject stays fixed despite selection/recommendation changes. Browsing/following/zooming/terrain clicks remain read-only, while legal high-level guidance follows the same owner/Runtime contract.

### 11.3 Receipt

Each receipt explains action, result, reason and next. Preserve original operation/subject association; an authorized player-scoped result may have no Agent. Accepted/submitted/queued is pending, completed-no-progress is not progress, and unknown outcome requires original-operation lookup, not a new request. No current selection or ambient Feed can fill missing identity/effect fields.

### 11.4 Loading, Empty, Error, and Fallback States

Reserve layout space and separate query/network/task/renderer failures. Startup GPU absence and loss after genuine ready are distinct tests. After graphical failure stop stale hit targets/animation, identify stale imagery, retain authorized text/Targets/receipt/safe control and provide bounded retry. Restore the current authorized view, not old private canvas contents or world actions.

Production world graphics remain WASM-owned. Rust-derived DOM tests/auxiliary views are explicitly scoped; renderer unavailable is not permission to implement independent JS terrain/routes. Detailed behavior follows [recovery](external-agent-world-view.design.md#view-recovery).

## 12. Accessibility and Robustness

Critical text remains readable on supported desktop/tablet/narrow/low-height surfaces. Keyboard focus is visible and restored to the invoker; non-drag single-pointer alternatives reach the same authorized observation targets. Touch does not depend on hover. Stage shortcuts do not consume text input/chat/IME.

Escape closes the innermost drawer before Focus; during composition the IME consumes it. Follow exit has an explicit accessible control, and manual exploration must not be continuously overridden. Reduced motion retains all essential meaning; refresh must not steal focus or announce every entity update as a new success.

Contrast, no horizontal overflow, no clipped primary buttons, no obscured feedback and correct CJK are mandatory. Treat world labels as text, not executable markup. Specific combinations/methods are in [PW-ACCESSIBILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility); this is not a blanket certification claim.

## 13. Visual Review Gate

Visible output changes require screenshot-based review under the existing SOP. Minimum evidence is a desktop capture, mobile when first-screen/responsive/navigation/layout/text density is affected, automated state/console/UI evidence or explicit unavailable reason, and the expected contract. Large visual-system changes also use the brand-system expanded matrix.

Must pass: correct first subject and hierarchy, no blocking clipping/overlap/illegibility, no unsupported causality, equivalent mobile route, and secondary diagnostics. A sparse fixture without relation authority must not fail merely for lacking a route; it must fail if the UI invents one or hides provenance.

<a id="visual-review-escalation-design"></a>
Apply the [canonical verdict/confidence table](../../testing/manual/model-visual-review-sop-2026-05-29.manual.md#visual-review-decision-contract). Only pass + high with no independent escalation condition can replace routine human visual review. Escalate disputed direction/interaction/flow, medium/low confidence, missing key viewport, public/release promises, watch (including the first watch), block or human_escalation. Preserve prior cards and owner decisions. This does not satisfy required review, L5, release or public approval.

Pixel-world blockers include terrain overpowering Agent/goal, diagnostics overpowering command/receipt, misleading coordinate/freshness, invented relationships or causality, unselectable overlapping targets, and missing authorized recovery. Historical #1385 screenshots/cards only prove their original candidates; later visible changes need current evidence. Five-second comprehension requires the distinct [PW-READABILITY method](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-readability), not a screenshot verdict alone.

## 14. Implementation Brief for Viewer Work

State Player/Director scope, first-read subject, affected stage/command/context/diagnostics, subject/provenance/receipt/empty/recovery behavior, desktop/mobile/input matrix, stable hooks and non-goals. Structural/visual changes with material layout or interaction alternatives still require two or three bounded comparison artifacts and inherited decisions under the existing workflow; routine fixes must not become an excuse for a new Shell redesign.

A documentation-only contract revision does not supply screenshots or pretend those comparisons/reviews have run. Implementation and visible changes obtain the corresponding actual evidence.

Detailed journeys, independent observation/draft/original-operation identity, viewport/input states and versioned references are specified in the [page/module handoff](viewer-page-module-design-2026-06-18.design.md#detailed-uiux-handoff). These refine this specification, not establish another visual authority. Static reference review proves only the reference's presentation; it does not replace the current-candidate visual, input, runtime or comprehension evidence above.

## 15. Acceptance Criteria for Future Visual Changes

World-first hierarchy, player/ambient separation, fixed original causality, discoverable command, accessible diagnosis/recovery, truthful source/partial/unknown and progressive density remain mandatory. Supported Web/native combinations are verified independently. Build/DOM/probe, software raster, real GPU, real interaction and real-player understanding are different evidence levels; missing levels stay unverified.

## 16. Governance

This file is the canonical visual direction; brand tokens and shell contracts remain with their owners. Topic PRDs may refine but not weaken source honesty, hierarchy or the review gate. Direction changes update the Viewer landing navigation and affected product/system contracts; current capabilities still require implementation and evidence.

## 17. Residual Risks

Final asset language and token values stay in the brand companion; they do not establish runtime features, marketing identity or release readiness. Upstream original-operation binding, dense-world data and geometry/freshness support can block truthful visuals. Text degradation is preferable to fabricated world state; real user comprehension, native compatibility and quality budgets remain separately measured.

## 目标、范围与设计边界

本修订统一已有视觉方向，并细化世界舞台的主体/来源/探索/恢复义务；不改变Shell布局、世界规则、权限或WASM ABI。产品要求与专业机制按下表明确分配，设计不自报实现或运行验收。

## 决策、风险与未决范围

选择复用现有品牌与Shell、按权威信息绘制、失败时保留合法文本，而非新增第二渲染器或从装饰补事实。代价是某些数据不足时不能提供完整空间画面。原操作关联、来源时效支持、密集世界可用性和原生兼容仍待对应实现与实际验证；视觉规范本身不能关闭这些风险。

## Visual review traceability

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | 当前目的/blocker/下一步首读、单一主区和窄屏从属。 | [HUD层级](#83-hud-dock-and-contextual-console-hierarchy) | Viewer/visual及Shell authority。 | 局部层级不证明完整探索或真人理解。 |
| [QA visual review contract](../../testing/prd.md#visual-review-product-contract) | verdict/confidence和升级条件一致。 | [评审升级](#visual-review-escalation-design) | QA/visual；产品N/A：理由=本行仅规定专业取证与评审机制；范围=不新增玩家产品义务，不排除其他行。 | 不代签required review/L5/发行。 |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | 玩家因果与ambient分开，选择不改原结果。 | [玩家影响](#62-player-leverage-before-ambient-activity) | Runtime因果、Viewer/visual。 | renderer健康/世界繁忙不是玩家成功。 |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | 原行动/结果/原因/下一步可解释。 | [回执](#113-receipt) | 原operation/receipt authority。 | 文字不证明原世界效果。 |

### 11.1 验证映射表

| 上游 requirement / acceptance | 本设计条款 | 独立 obligation 与条件 | 验证方法 / source / scenario / candidate | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | [HUD层级](#83-hud-dock-and-contextual-console-hierarchy) | 主区/从属/窄屏无退化。 | [PW-ACCESSIBILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility)，当前release/S6、desktop/narrow、键盘/CJK。 | 状态、截图、焦点及实际步骤。 | 静态CSS/JSX不代签浏览器。 |
| [QA visual review contract](../../testing/prd.md#visual-review-product-contract) | [评审升级](#visual-review-escalation-design) | 同证据下升级条件一致。 | [SOP](../../testing/manual/model-visual-review-sop-2026-05-29.manual.md#visual-review-decision-contract)；无截图/缺视口/watch/medium/low/public/dispute判据，同候选。 | 实际card及owner处置，未执行标明。 | 不制造截图/发行通过。 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | [玩家影响](#62-player-leverage-before-ambient-activity) | ambient/他人结果/选中变化不冒充玩家进展。 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)和PW-READABILITY，同候选及原结果。 | 原主体/操作、UI、答案与误归因。 | 单张图/模型不证明真人理解。 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | [回执](#113-receipt) | pending/no-progress/unknown/无Agent结果解释正确。 | [PW-FRESHNESS](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-freshness)与现有S6，实际受权原操作。 | 状态、原因、原结果及恢复路径。 | 页面标签不证明服务器授权/世界成功。 |
