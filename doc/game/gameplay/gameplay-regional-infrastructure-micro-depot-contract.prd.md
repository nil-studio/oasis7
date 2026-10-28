# Gameplay 区域基础设施 micro_depot 合同 PRD

- 对应设计文档: `doc/game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.design.md`
- 可变执行状态: 对应 GitHub Project task 与 issue evidence comments

审计轮次: 1

## 目标

- 将 `micro_depot` 从 design-only 补充提升为正式 `PRD-GAME-016` topic truth。
- 定义玩家通过小型、可审计、带 upkeep 的区域设施改变一次 repair / logistics quote 的体验承诺。
- 产品承诺入口：[`受治理的区域能力与扩展`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001)；本合同保留设施生命周期、费用、失败恢复与实现权威。
- 明确 WASM proposal 与 runtime authority 的边界，防止自由建造、任意 WASM 上传或 global governance 权力漂移。

## 范围

- 覆盖 `micro_depot` single-commission consumable lifecycle 的玩家动词、区域专业化阶段边界、install/service/depletion/reclaim/reinstall、状态经济规则、失败恢复、feedback surfaces、balance risks 和验证矩阵。
- 覆盖 topic triplet 与 root/gameplay/index 路由同步。
- 不覆盖具体 Rust/WASM 实现、任意玩家上传 WASM 治理、数值调参最终值、release/public claim 放行或 closed beta 阶段升级。

## 接口 / 数据

- PRD 主入口: `doc/game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.prd.md`
- 设计细节: `doc/game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.design.md`
- 当前执行: 对应 GitHub Project task 与 issue evidence comments
- 根 baseline: `doc/game/prd.md` / `PRD-GAME-016`
- 关键 runtime 数据: `RegionalInfrastructure`, `MicroDepotEvalInput`, `MicroDepotProposal`, `MicroDepotServiceApplied`
- 关键 player-facing 数据: install quote, upkeep state, service radius, before/after preview, blocker, receipt, module evidence

资源 provenance 边界：设施/材料 inventory 与 records 只在本专题授权的生命周期和 receipt 中使用，不自动成为 `Electricity`、`Data`、restricted slot-1 claim/upkeep support 或 liquid starter OC；它们也不得形成持续或无成本设施补给。跨模块来源与 sink 类别见 [`doc/product/world-rules-core-gameplay/prd.md`](../../product/world-rules-core-gameplay/prd.md#resource-model-and-cross-module-provenance)，本专题继续拥有具体 lifecycle、费用与实现合同。

## 里程碑

- M0 (2026-07-05): topic PRD/design/project triplet formalized and rooted in `PRD-GAME-016` routes.
- M1: `micro_depot.eval.v2` WASM ABI + host adapter proves deterministic measured proposal hash while retaining v1 legacy compatibility.
- M2: runtime state/events and quote pipeline support install/service/reclaim with structured blockers.
- M3: Viewer / pure API / agent surfaces expose quote, receipt, module evidence and next useful action.
- M4: QA smoke proves one repair/logistics action becomes cheaper, faster or less risky because of depot, while remaining blocker is visible.

M1–M4 是目标验收顺序，不是当前可用性、mainline 合入、阶段变化或公开 claim。任何当前状态只能由 Git、PR 和实际验证记录、同一候选的新鲜专业证据与根 `README.md` 的统一 claim envelope 共同确认；本 topic PRD 及历史或局部样本不能代签。

## 风险

- 若根文档吸收完整矩阵，会再次使根 PRD 臃肿。
- 如果 `micro_depot` 被写成 first-10-minute action，会破坏 trust/capability gate sequencing。
- 如果 WASM 被赋予 canonical mutation authority，会破坏 determinism、audit 和 replay safety。
- 如果效果过强、可堆叠或绕过 upkeep，会变成唯一正确解或 free-building exploit。
- 如果 receipt/module evidence 不可读，玩家仍无法知道自己造成了什么变化。

## 1. Executive Summary

- Problem Statement: `micro_depot` 已在设计补充中定义了可编程区域设施的玩家循环、WASM-backed 规则边界与 smoke gate，但缺少正式 PRD / project triplet，容易被误读为未承诺候选或直接实现承诺。
- Proposed Solution: 新增 `PRD-GAME-016`，将 `micro_depot` 明确为第一个 bible-grade 可编程区域设施专题：玩家通过小型、可审计、带 upkeep 的区域设施，改变一次 repair / logistics 行动的 quote，并获得可追溯 receipt。
- Success Criteria:
  - SC-1: 玩家能在同一专题内读懂为什么部署 depot、花了什么、改变了哪一次 repair / logistics 行动、还有什么 blocker 没解决。
  - SC-2: 设计明确保持 `regional specialist / limited-scope regional influence` 阶段边界，不进入首 10 分钟新手循环。
  - SC-3: `micro_depot` 的 WASM 权限边界清楚区分 proposal 与 runtime authority：WASM proposes, runtime validates / applies / signs。
  - SC-4: topic PRD / design / project、根 PRD、gameplay README 与 `prd.index.md` 可互相定位，不把详细规则塞回 root PRD。
  - SC-5: 本专题不升级当前阶段或对外 claim envelope；当前仍为 `internal_playable_alpha_late` / `limited playable technical preview`。
  - SC-6: 任何 10 分钟验证样本必须写作 `post-first-capability 10-minute slice` 或 `regional-specialist 10-minute scenario`，并显式列出 first capability、稳定 claim、repair/logistics blocker receipt、regional specialist lane 与玩家可读 surface 前置条件。
  - SC-7: 每次 service preview 明确给出按资源种类计量的整数 debit；apply 原子扣减库存与内部 throughput accounting，receipt 显示两者 before/after。库存耗尽即结束本次 commissioning 的服务生命，upkeep 不补货且在零库存时拒绝、不收费。

## 2. User Experience & Functionality

- User Personas:
  - 区域小玩家 / regional specialist: 已完成 first capability，想用一次有明确耗尽边界的局部设施 commissioning 改善 repair / logistics 行动，而不是立即投靠 major power。
  - 回流玩家: 看到区域 blocker 后，需要一个可解释、可恢复、可审计的下一步，而不是只看 raw log。
  - `gameplay_designer`: 需要把玩家动词、loop、收益、失败和 anti-abuse 边界写成可验证玩法规则。
  - `producer_system_designer`: 需要冻结系统承诺、stage boundary、资源经济和 release claim 边界。
  - `runtime_engineer` / `wasm_platform_engineer`: 需要明确 WASM 只给 proposal，canonical state / accounting / receipt 由 runtime 拥有。
  - `viewer_engineer` / `agent_engineer` / `qa_engineer`: 需要玩家可读 surface、agent action contract 与 smoke matrix。
- User Scenarios & Frequency:
  - 首次区域设施解锁: 玩家完成 first capability、稳定持有 claim、完成 repair / logistics 闭环并看到重复 blocker 后触发。
  - 区域阻塞恢复: supply missing、route blocked、repair 等待过长或 logistics quote 风险过高时重复发生。
  - 维护与回收: upkeep 未付、设施超出有效范围、收益不足或玩家转向新 specialization 时发生。
- User Stories:
  - PRD-GAME-016: As a regional specialist, I want to deploy a small auditable depot that changes one repair/logistics quote, so that I can create local leverage without gaining free building, arbitrary scripting, or global governance power.
  - PRD-GAME-016A: As a player, I want before/after quote and receipt evidence, so that I understand what the depot changed and what remains blocked.
  - PRD-GAME-016B: As an implementation owner, I want WASM proposal authority separated from runtime validation/application/signing, so that programmable infrastructure stays deterministic, auditable, and bounded.
- Critical User Flows:
  1. Flow-MD-001: `regional pressure card -> repair/logistics blocker explanation -> suggested intervention: deploy micro_depot`
  2. Flow-MD-002: `install quote + upkeep + service radius preview -> player confirms -> MicroDepotInstalled receipt`
  3. Flow-MD-003: `repair/logistics quote request -> micro_depot.wasm evaluates proposal -> runtime validates -> before/after preview`
  4. Flow-MD-004: `player confirms measured debit -> runtime atomically revalidates and debits stock + internal throughput accounting -> receipt shows before/after -> remaining blocker / next regional option`
  5. Flow-MD-005: `stock reaches zero -> further service rejects without mutation -> upkeep rejects without charge -> destructive reclaim -> optional fresh full-cost reinstall`; out-of-range/unsupported/duplicate/permission failures remain structured blockers, while refill/reset are future work.
- Functional Specification Matrix:

| Surface | Required fields / signals | Player verb | State transition | Rule / computation boundary | Owner |
| --- | --- | --- | --- | --- | --- |
| Install quote | `install_cost=10 Data`, `deployment_sink=2 Data`, `commissioned_transfer=8 Data`, `upkeep_per_epoch`, `service_radius_cm`, `supported_resource_kinds=["data"]`, `expected_effect`, `remaining_blockers`, `forecast_status`, `forecast_basis`, `expected_future_blockers_covered`, `break_even_uses`, `upkeep_horizon_epochs`, `low_use_warning`, `recommendation_reason` | inspect quote, compare ROI, deploy | `suggested -> quoted -> installed / rejected` | `forecast_status` is `sufficient` only when the runtime can bind the quoted horizon to current regional-pressure/blocker evidence; otherwise it is `insufficient`, `low_use_warning=true`, and `recommendation_reason` must say that the facility is not forecast-recommended rather than inventing coverage. `forecast_basis` identifies the evidence snapshot or insufficiency cause. v2 MVP accepts exactly lowercase Data-only `["data"]`; empty/non-data/mixed/duplicate/case variants reject atomically. Quote must come from runtime, not Viewer guesswork | `runtime_engineer` + `viewer_engineer` + `gameplay_designer` |
| Facility state | `facility_id`, `owner_claim_id`, `location_id`, `status`, `module_id`, `wasm_hash`, `inventory_revision`, `available_units_by_kind`, `throughput_limit_units_per_epoch`, `throughput_epoch`, `throughput_remaining_units`, `last_receipt_id` | inspect depot, pay upkeep while stocked, reclaim | `active(stocked) -> depleted -> reclaimed -> optional fresh install` | v2 is single-commission consumable: install debits `10 Data` as `2` sink + `8` commissioned inventory. Stock zero ends service life; upkeep neither replenishes nor charges at zero. Reclaim destroys remainder/refunds zero; only fresh full-cost reinstall creates new supply | `runtime_engineer` |
| WASM proposal | `schema_version = micro_depot.eval.v2`, `proposal_hash`, `effect_delta`, `explanation_code`, `resource_debits[{resource_kind, units}]`, `evaluated_inventory_revision`, `evaluated_epoch` | service repair/logistics | `not_applicable / blocked / applicable` | v2 proposes exact integer quantities against one runtime snapshot; v1 remains legacy adapter compatibility only; WASM cannot mint/debit resources, mutate ownership, bypass upkeep, or write canonical world state | `wasm_platform_engineer` + `runtime_engineer` |
| Service evaluation | before/after cost, risk, wait, route, repair/logistics outcome, exact resource debit, stock/throughput availability | inspect evaluated effect, execute service | `evaluated -> applied / blocked` within one synchronous action | Evaluation never reserves; proposal revision/epoch must match its evaluator input, then runtime atomically validates and debits stock and throughput with the service effect | `runtime_engineer` |
| Receipt | `accepted_intent_id`, `execution_status`, `blocker_type`, `world_change_summary`, `module_hash`, `proposal_hash`, `evaluated_epoch`, `inventory_revision_before`, `inventory_revision_after`, `resource_debits`, `inventory_units_before_by_kind`, `inventory_units_after_by_kind`, `throughput_units_before`, `throughput_units_after` | inspect receipt, choose next action | `applied -> next_blocker_visible / resolved` | Receipt must prove exact depot contribution; failed apply records blocker but no partial debit or service mutation | `viewer_engineer` + `qa_engineer` |

- Acceptance Criteria:
  - AC-1: `micro_depot` is formally registered as `PRD-GAME-016` and reachable from root/gameplay/index routes.
  - AC-2: The topic explicitly states it is not first-10-minute onboarding, free building, arbitrary player WASM upload, UGC market, Minecraft-style block placement, global governance, or starter-funding subsidy.
  - AC-3: Player verbs include diagnose regional blocker, inspect quote, deploy, pay upkeep only while stocked, service repair/logistics, inspect receipt, reclaim, and fresh full-cost reinstall.
  - AC-4: The reachable core lifecycle covers blocker -> quote -> deploy -> measured service(s) -> stock zero -> service blocked -> upkeep rejected/no charge -> destructive reclaim -> optional fresh full-cost reinstall.
  - AC-5: State/economy rules cover install cost, upkeep, service radius, one depot per claim/location, supported resources, effect caps, degradation/grace, and funding provenance.
  - AC-6: Current-slice failure/recovery covers unpaid upkeep, out of range, unsupported resource, an empty regional blocker causal reference, duplicate facility, insufficient funds, permission denial, and reclaim. The current runtime treats a non-empty `regional_blocker_receipt_id` as an opaque causal reference; it does not yet prove that the referenced repair/logistics receipt exists.
  - AC-7: Every applied service records module id/version/hash, schema version and proposal hash for replay/audit.
  - AC-8: QA smoke proves one repair/logistics action becomes measurably cheaper, faster, or less risky because of depot, and the player can identify remaining blocker.
  - AC-9: Install quote must include a minimal ROI strip: expected future blockers covered, break-even uses, upkeep horizon, low-use warning and recommendation reason; if the player can only see one before/after quote improvement, the depot is not yet a strategic facility choice.
  - AC-10: Canonical state exposes non-negative integer `available_units_by_kind`, `inventory_revision` and internal throughput accounting fields. v2 install atomically debits `10 Data`, sinks `2`, transfers `8` into commissioned inventory and initializes internal throughput `16/16`, epoch `0`; receipt/provenance records that split. Stock zero ends service life, upkeep at zero rejects/no charge, reclaim refunds zero and destroys all remainder, and only a fresh `10 Data` reinstall creates fresh `8/16` with no carry.
  - AC-11: `micro_depot.eval.v2` evaluator output binds exact debits to `proposal_hash + evaluated_inventory_revision + evaluated_epoch`, and those snapshot fields must equal its synchronous evaluator input. Runtime atomically debits stock + throughput with the service effect, or changes none of them. Runtime retains schema-bound v1 legacy compatibility through the exact historical pre-change wire fields, canonical serialization, and input/proposal hash projection; v2 fields never enter the v1 projection. Absent measured fields decode as revision/stock/throughput zero, without retroactive commissioning grant or new class-only producer.
  - AC-12: Applied receipts expose exact debit and inventory/throughput before/after. Reachable depletion is reported as insufficient inventory with reclaim/fresh reinstall or another action; throughput `16` is retained as a defense-in-depth ceiling/replay invariant, not an independently naturally reachable gameplay blocker or recovery loop. Independent preview-confirm concurrency and stale-preview recovery are not current acceptance.
  - AC-13: v2 MVP accepts exactly `supported_resource_kinds == ["data"]`; empty, non-Data, mixed, duplicate, or case-variant lists reject atomically without charging install cost or creating facility state.
  - AC-14: Every install quote distinguishes a forecast sufficient for a strategic recommendation from an insufficient/stale forecast. A sufficient forecast identifies its regional-pressure/blocker evidence and horizon; an insufficient forecast sets `low_use_warning=true`, gives an insufficiency reason, and must not represent `expected_future_blockers_covered` or `break_even_uses` as a recommendation to deploy. The quote remains inspectable and deployable when other rules allow it, preserving player choice without false certainty.
  - Future follow-up, not current acceptance or current playability evidence: canonical repair/logistics receipt production and provenance validation (or an equivalent canonical regional-pressure evidence source), authorized refill, canonical epoch clock, throughput rollover/reset, and configurable commissioning/debit curves require gameplay/runtime joint design and their own acceptance/tests. Until a canonical evidence producer exists, tests and public claims must not describe a non-empty causal reference as an authenticated prior receipt.

当前可用性与 release/public claim guard：上述 AC 与路线图定义目标合同和验收边界，不改变当前 stage 或 claim。只有同一新鲜候选具备适用的 runtime、WASM、Viewer/pure API、QA/playtest 及需要时 recovery/sync 证据，并经产品决策与 LiveOps 同步后，才可由根 `README.md` 评估或表达公开 claim；单项实现、topic 文档、历史证据或局部 green 均不能代签。
- Non-Goals:
  - No block placement, digging, terraforming, free-build construction, or direct embodied control.
  - No arbitrary player-uploaded WASM in MVP; only repo-authored allowlisted module hashes.
  - No bypass of claim scope, upkeep, restricted starter fund transfer guards, or resource accounting.
  - No global governance power, alliance leadership, or closed-beta/release claim upgrade.

## 3. AI / Agent System Requirements

- Tool Requirements:
  - Agent action contract must surface `InstallMicroDepot`, `EvaluateMicroDepotQuote`, `ServiceRepairFromDepot`, `ServiceLogisticsFromDepot`, `PayDepotUpkeep`, `ReclaimDepot`, and `InspectDepot` as bounded, explainable actions.
  - Agent must not silently deploy a depot without player-visible quote, expected effect, upkeep, and remaining blocker.
- Evaluation Strategy:
  - Agent output is acceptable only when it preserves accepted intent, quote readability, receipt causality, and recovery suggestions.
  - If the agent chooses another action, it must explain why depot is out of range, unaffordable, unsupported, duplicate, suspended, or lower value than an alternative.

## 4. Technical Specifications

- Architecture Overview:
  - `micro_depot` is a WASM-backed regional facility. WASM evaluates a proposal against a runtime-provided input snapshot; runtime validates, caps, applies, signs and emits receipt.
  - Viewer and pure API render canonical DTOs only; they do not inspect WASM or infer benefit from events.
- Integration Points:
  - `doc/game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.design.md`
  - `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md`
  - `doc/game/gameplay/gameplay-indirect-control-agency-contract.prd.md`
  - `doc/product/world-rules-core-gameplay/prd.md`（产品承诺）与 `doc/game/gameplay/gameplay-top-level-design.prd.md`（玩法合同）
  - `doc/game/gameplay/gameplay-agent-claim-economy-contract.prd.md`
  - `doc/world-simulator/viewer/viewer-pixel-world-player-leverage-production-readability-2026-05-28.brainstorm.md`
- Edge Cases & Error Handling:
  - Empty `regional_blocker_receipt_id`: install is refused; player receives a blocker explaining that a prior repair/logistics causal reference is required. Unknown/non-empty provenance is not validated in the current slice because no canonical repair/logistics receipt producer or regional-pressure evidence ledger exists; authenticated provenance remains a future fail-closed gate.
  - Duplicate `owner_claim_id + location_id`: install is refused with existing depot pointer and inspect/reclaim option.
  - Unpaid upkeep while stocked: service is blocked or degraded; player can pay upkeep, reclaim, or choose another intervention. At stock zero, upkeep rejects and charges nothing.
  - Out of range: proposal returns not applicable; UI must show distance/radius explanation and next valid target when known.
  - Unsupported resource: proposal is blocked; UI must name unsupported resource class without suggesting arbitrary resource minting.
  - Insufficient inventory: apply is blocked as `DEPOT_INVENTORY_INSUFFICIENT`; UI names the kind, required units and available units without claiming an unimplemented refill path.
  - Throughput defense: runtime may fail closed as `DEPOT_THROUGHPUT_EXHAUSTED` for malformed/corrupt state, but `16 > 8` makes this unreachable before stock depletion in the current commissioning. UI and acceptance must not advertise a distinct throughput recovery path or epoch loop.
  - Insufficient funds: quote remains visible but install/action cannot proceed; restricted starter funding cannot become infinite facility subsidy.
  - Low expected use: if the regional pressure card predicts fewer follow-up blockers than `break_even_uses`, install remains possible only with a low-use warning and an explicit recommendation reason; the UI must not present the depot as an always-correct buff.
  - Forecast missing or stale: if current regional-pressure/blocker evidence cannot support the quoted horizon, runtime returns `forecast_status=insufficient` with a traceable `forecast_basis` insufficiency cause. The quote remains visible, but it must set `low_use_warning=true` and must not turn a one-off measured service improvement into a strategic deployment recommendation.
  - Phase boundary missing: if a sample, smoke test or QA script schedules `micro_depot` deployment before first capability completion, stable claim, repair/logistics blocker receipt or equivalent regional pressure evidence, active/eligible regional specialist lane, and claim/upkeep + repair/logistics blocker literacy, mark `micro_depot_phase_boundary_missing` and do not count the run as onboarding evidence.
  - Unknown module hash or schema mismatch: runtime refuses proposal and records module evidence blocker.
- Non-Functional Requirements:
  - NFR-MD-1: Given identical input snapshot and module hash, proposal hash must be byte-identical.
  - NFR-MD-2: 100% of applied depot services must emit module hash, proposal hash, before/after quote, world change summary and receipt id.
  - NFR-MD-3: 100% of player-facing previews must include remaining blocker or next useful action.
  - NFR-MD-4: Any formal playability or release claim must cite QA/playtest evidence; this PRD alone is not release evidence.
  - NFR-MD-5: 100% of install quotes must expose a coarse break-even judgment; missing ROI fields are a gameplay readability regression, even when before/after service preview works.
  - NFR-MD-6: 100% of applied services must reconcile integer debit quantities with inventory and throughput before/after; no negative balance, partial debit, floating-point quantity or preview-side reservation is permitted.
  - NFR-MD-7: 100% of install quotes must make their forecast sufficiency auditable: a strategic recommendation binds current regional-pressure/blocker evidence and horizon; an insufficient forecast is explicitly non-recommending and cannot be rendered as positive ROI.
- Security & Privacy:
  - WASM has no wall clock, network, filesystem, host escape, nondeterministic RNG, ownership mutation, resource minting or canonical state write authority.
  - Runtime preserves funding provenance and rejects restricted starter fund transfer or scope bypass.

## 5. Risks & Roadmap

- Phased Rollout:
  - R0: Formalize `PRD-GAME-016` triplet and routing without changing stage or claim envelope.
  - R1: Implement `micro_depot.eval.v2` measured WASM ABI + host adapter with deterministic proposal hash and v1 legacy compatibility.
  - R2: Add canonical state/events and replay-safe install/service/reclaim.
  - R3: Integrate repair/logistics quote pipeline and before/after preview.
  - R4: Add Viewer / pure API DTOs, receipt surface, module evidence, and blocker taxonomy.
  - R5: Run gameplay smoke and QA matrix before any release/public claim.
- Technical / Gameplay Risks:
  - If the effect is too weak, the depot feels like a tax; if too strong, it becomes the only correct action.
  - If upkeep appears too early, starter funding and first capability flow become punishing.
  - If module governance opens arbitrary upload too early, MVP becomes a security platform problem.
  - If receipt fields are insufficient, Viewer falls back to guessing causality from recent events.
  - If future facilities expand too fast, oasis7 can be misread as direct construction gameplay rather than indirect-control civilization simulation.

## 6. Validation & Decision Record

- Test Plan & Traceability:

| PRD-ID | Corresponding task / slice | Evidence tier | Validation method | Regression scope |
| --- | --- | --- | --- | --- |
| PRD-GAME-016 | `micro-depot-topic-triplet-formalization` | `test_tier_required` | doc governance, root/gameplay/index triplet reachability, no stage/claim drift | topic authority and design-bible completeness |
| PRD-GAME-016 | `micro-depot-wasm-abi-host-adapter` | `test_tier_required` | same v2 input produces byte-identical measured proposal hash; schema/hash mismatch rejects; golden v1 payload retains exact pre-change wire/action and input/proposal hashes | WASM determinism and adapter boundary |
| PRD-GAME-016 | `micro-depot-runtime-state-events` | `test_tier_required` | install debits `10 Data` as `2` sink + `8` transfer; reclaim refunds zero/destroys remainder; reinstall creates fresh `8/16` with no carry; replay produces identical state | canonical facility lifecycle and anti-refill economics |
| PRD-GAME-016 | `micro-depot-quote-pipeline` | `test_tier_required` | repair/logistics preview shows before/after with capped depot contribution | player quote readability |
| PRD-GAME-016 | `micro-depot-roi-forecast-sufficiency` | `test_tier_required` | current regional-pressure/blocker evidence yields `forecast_status=sufficient` with a bound horizon/basis; missing or stale evidence yields `insufficient`, `low_use_warning=true`, and a non-recommending reason without synthetic coverage | anti-misleading strategic ROI guidance |
| PRD-GAME-016 | `micro-depot-viewer-api-surface` | `test_tier_required` | Viewer and pure API show same quote, blocker, receipt and module evidence | player-facing receipt and parity |
| PRD-GAME-016 | `micro-depot-gameplay-smoke` | `test_tier_required` + `test_tier_full` | reachable lifecycle: install -> measured service(s) -> stock zero -> service blocked -> upkeep rejected/no charge -> destructive reclaim -> optional fresh full-cost reinstall | single-commission consumable lifecycle |
| PRD-GAME-016 | `micro-depot-measured-supply-accounting` | `test_tier_required` + `test_tier_full` | v2 tests prove Data-only install rejection matrix, `10=2 sink+8 transfer`, fresh `8/16`, exact atomic service debit, zero-stock service/upkeep rejection without charge, zero-refund destructive reclaim/reinstall, before/after reconciliation, internal throughput ceiling/replay invariant, and exact historical v1 wire/hash compatibility; refill/reset and preview-confirm races excluded | consumable inventory lifecycle and replay accounting |

- Decision Log:

| Decision ID | Selected option | Rejected alternative | Evidence / rationale |
| --- | --- | --- | --- |
| DEC-MD-001 | Promote `micro_depot` to formal `PRD-GAME-016` topic triplet | Leave as design-only supplement while README routes to it | Design already defines loop, state, receipt and smoke gates; role slices flagged design-only status as the clearest bible-grade gap. |
| DEC-MD-002 | Keep `micro_depot` in regional specialist / limited-scope regional influence | Make it a first-10-minute onboarding action | It requires claim/upkeep/blocker/receipt literacy and would steal focus from trust/capability gates. |
| DEC-MD-003 | WASM proposes, runtime validates/applies/signs | Let WASM mutate canonical state or account balances | Determinism, replay, security and auditability require runtime authority. |
| DEC-MD-004 | MVP only allows repo-authored allowlisted module hash | Allow arbitrary player-uploaded WASM | Arbitrary upload would move the slice from gameplay facility to security/governance platform work. |
| DEC-MD-005 | Depot affects one bounded repair/logistics quote and receipt | Treat depot as global buff or governance power | Bounded quote contribution preserves small-player regional leverage without global power creep. |
| DEC-MD-006 | Install quote includes coarse ROI guidance: expected future blockers, break-even uses, upkeep horizon, low-use warning and recommendation reason | Only show install/upkeep plus one before/after service improvement | 2026-07-08 gameplay slice flagged that without break-even guidance, a depot can feel like a tax or an always-correct buff rather than a regional specialization choice. |
| DEC-MD-007 | Treat 10-minute depot verification as a `post-first-capability` slice | Label the sample as generic `0-10 min` onboarding | The depot loop requires first capability, stable claim, prior blocker evidence and regional specialist eligibility; otherwise QA can mistake it for an early onboarding gate. |
| DEC-MD-008 | Scope v2 as a single-commission consumable depot: exact `data=8` stock is the reachable service-life bound; throughput `16`, epoch `0` remain internal defense-in-depth/replay fields; zero-stock upkeep rejects/no charge and recovery is destructive reclaim plus fresh full-cost reinstall | Ongoing/replenishing facility, upkeep-funded supply, independent throughput recovery loop, free reinstall refill, retroactive v1 grants, or current epoch playability claims | This is the smallest reachable deterministic lifecycle that prevents unlimited use. Refill, epoch/reset and configurable constants remain future work with separate acceptance. |
| DEC-MD-009 | A depot ROI recommendation requires a traceable, current regional-pressure/blocker forecast; missing or stale evidence produces an explicit non-recommending quote, not synthetic coverage | Infer a positive or negative strategic recommendation from one measured service preview or hide the uncertainty | A single quote can prove local effect but cannot establish repeat demand. The distinction preserves player agency while preventing false-precision economic guidance and creates a deterministic cross-surface contract for a future implementation slice. |
