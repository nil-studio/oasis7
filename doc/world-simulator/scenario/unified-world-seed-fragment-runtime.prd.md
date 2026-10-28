# oasis7 Simulator：统一大世界种子与 Fragment Runtime 复用（需求文档）

- 对应设计文档: `doc/world-simulator/scenario/unified-world-seed-fragment-runtime.design.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history；本文与 design 保留 chain-authoritative seed、resource manifest、delta 与 replay 合同。

## 1. Executive Summary
- 统一大世界、local testnet、public testnet 与 runtime live 必须共享同一套 `world_seed -> chunk -> fragment -> resource budget` 生成语义。
- 当前 simulator 已有 deterministic chunk/fragment/resource 生成能力；runtime live / formal release 默认世界仍主要依赖固定 bootstrap 库存与 `runtime:x:y:z` 坐标锚点，需要收敛。
- 新用户第一轮体验应从统一 seed 派生的 starter-safe chunk 开始，而不是单独硬编码一个与世界资源无关的 Agent/Location。
- 资源、出生点、可采碎片、Location 展示、viewer pixel-world 地形与链上/共识快照应来自同一份世界生成事实。
- 资源生成必须链上化：正式世界的 chunk/resource provenance、remaining budget 与后续采集变更必须能由 chain/genesis/commit 证明，runtime live 只能消费或提交这些事实，不能在链下生成一套正式资源账本。

## 目标
- 将统一大世界的 starter spawn、fragment location、resource budget 与后续 resource delta 收敛为同一份 chain-authoritative seed/chunk/fragment 事实。
- 让 local testnet、public testnet、viewer、runtime live 与 provider observation 对同一个 `world_seed` 和 resource provenance 给出一致解释。

## 范围
- In scope: seed manifest、chunk/fragment/resource provenance、starter chunk/onboarding 接线、viewer/provider observation 一致性、resource delta replay 合同。
- Out of scope: 新经济数值平衡、新资源品类、完整删除兼容坐标锚点、非 testnet/正式世界的调试 fixture 迁移。

## 接口 / 数据
- `SeedManifest` / `oasis7.world_resource_manifest.v1`: `world_id`、`chain_id`、`genesis_ref`、`world_seed`、`chunk_generation_schema_version`、`world_config_hash`、`generation_algorithm_id/hash`、`created_at_height/block_hash`。
- `ChunkResourceManifest` / `oasis7.world_resource_manifest.v1.generated_chunks`: chunk coord、`chunk_seed`、`chunk_status`（`committed` / `chain_pending` / `provisional` / `exhausted`）、fragment ids、profile hash、budget total hash、budget remaining hash、commit ref。
- `ResourceDelta` / `oasis7.world_resource_delta.v1`: delta id、action/event id、ordering key、fragment/location/chunk ref、material/resource key、delta amount、commit height/hash、base/result manifest hash、actor/source/replay status。
- `/v1/chain/status.world_resource`: testnet readiness 必须暴露 `schema_version`、`delta_schema_version`、`world_id`、`chain_id`、`world_seed`、`chunk_generation_schema_version`、seed/starter/latest commit hash、committed/provisional/pending delta counters、`readiness_status` 与 `failed_gates[]`。

## 里程碑
- M1: 文档合同冻结，明确资源生成必须链上化。
- M2: runtime live snapshot 暴露 seed/chunk/resource provenance。
- M3: claim/onboarding 绑定 starter chunk spawn 与 starter resource context。
- M4: chain commit/replay 支持 resource manifest 与 delta。
- M5: testnet readiness 证明 viewer/runtime/provider resource provenance 一致。

## 风险
- 既有 runtime live 与测试仍依赖固定 bootstrap 库存，需要阶段迁移。
- 链上承载完整 fragment/profile/budget 明细可能过重，需要 hash/manifest/ref 折中。
- optimistic UI 与 committed resource delta 之间需要清晰 pending/rollback 状态。

## 2. Problem Statement
当前存在两条语义：

- simulator/scenario 侧：已有基于 seed 的 chunk、fragment、材质、元素预算、补种和可采资源账本。
- runtime live/formal release 侧：默认启动会注册 starter Agent，Location 多数由 Agent 坐标派生；资源主要是固定启动库存。

这会导致本地测试入口看起来像大世界，但地点和资源并不是同一个世界生成系统的产物，后续接入 testnet / bridge / LLM 时容易出现“世界事实不一致”。

## 3. Requirements
### R1 统一世界种子

Trace refs: O7-REQ-f8497929-34dc-4193-a435-7dbe60bf0e4c；产品唯一声明沿下方接收链接，原专业条款保持。

- 每个大世界必须有唯一 canonical `world_seed`。
- `world_seed` 必须写入 genesis / manifest / runtime snapshot 可验证字段。
- local testnet、public testnet、launcher、viewer 与 provider observation 必须引用同一世界种子或其派生证明。

产品承诺接收：[既有统一世界种子的跨入口引用](../../product/agents-world-simulation/prd.md#canonical-seed-consumer)；仅上述 R1 原消费者结果，工程字段/算法仍归本专题，不改变适用 profile 或正式未决答案。

### R2 复用现有 chunk/fragment/resource 生成

Trace refs: O7-REQ-112440b1-da55-4443-b513-58dbf9056511；产品唯一声明沿下方接收链接，原专业条款保持。

- 可玩大世界的地点与资源必须优先来自现有 simulator chunk/fragment 生成链路。
- `chunk_seed(world_seed, coord)` 是 chunk 内 fragment/resource 的唯一随机根。
- 资源可采量必须使用 `fragment_budget` / `chunk_resource_budgets` 的 total/remaining 账本，不允许运行时临时重算。

产品承诺接收：[既有生成地点与资源预算的同一来源](../../product/world-rules-core-gameplay/prd.md#generated-resource-source)；仅上述 R2 原消费者结果，工程字段/算法仍归本专题，不改变适用 profile 或正式未决答案。

### R3 Runtime live 消费世界生成事实

Trace refs: O7-REQ-212aa59b-6d97-43a7-8fd7-35d441489281；产品唯一声明沿下方接收链接，原专业条款保持。

- runtime live 不再自造一套独立资源世界。
- `runtime:x:y:z` Location 只能作为短期兼容锚点或调试 fallback；可玩入口不得把它当作正式资源地点。
- formal release 默认世界应加载或生成 starter-safe chunk，并从该 chunk 派生 starter spawn、nearby resource pocket 与初始可见目标。

产品承诺接收：[既有正式入口消费生成世界事实](../../product/agents-world-simulation/prd.md#formal-generated-world-entry)；仅上述 R3 原消费者结果，工程字段/算法仍归本专题，不改变适用 profile 或正式未决答案。

### R4 新用户入口

Trace refs: O7-REQ-7853375e-94b6-4c6c-8d6b-c486a4277dd4；产品唯一声明沿下方接收链接，原专业条款保持。

- `claim_first_agent` 应将 Agent 绑定到统一大世界中的 starter-safe spawn，而不是固定 `(0,0,0)` 或另一个与资源生成无关的位置。
- 初始 OC / LLM 预算仍可以是 onboarding grant，但世界物料与可采资源应来自 starter chunk。
- 如果 starter chunk 生成失败，页面必须阻塞并显示明确世界生成错误，而不是展示空快照或假地点。

产品承诺接收：[既有首次认领的生成来源与失败边界](../../product/agents-world-simulation/prd.md#first-claim-generated-context)；仅上述 R4 原消费者结果，工程字段/算法仍归本专题，不改变适用 profile 或正式未决答案。

### R5 Viewer 与 Provider 观察一致

Trace refs: O7-REQ-fd8b7e31-afa8-4361-81f0-849c07099dfd；产品唯一声明沿下方接收链接，原专业条款保持。

- Viewer 的 `model.locations`、pixel-world fragment terrain、resource summaries 与 provider observation 必须来自同一 snapshot。
- Provider 不能看到与玩家 viewer 不一致的 location/resource ids。
- UI 可以隐藏低层 frag 细节，但不能把派生锚点误标成正式资源地点。

产品承诺接收：[既有Viewer与Provider的同快照资源观察](../../product/agents-world-simulation/prd.md#shared-snapshot-resource-observation)；仅上述 R5 原消费者结果，工程字段/算法仍归本专题，不改变适用 profile 或正式未决答案。

### R6 资源生成全部链上化

Trace refs: O7-REQ-2b3769ae-078a-4123-a0de-e20405b42c7c；产品唯一声明沿下方接收链接，原专业条款保持。

- 正式资源生成事实必须由链上可验证记录承载，包括 `world_seed`、`chunk_generation_schema_version`、chunk coord、fragment ids、fragment profile hash、fragment budget hash、chunk resource budget hash。
- genesis 可以预提交 starter chunk；后续 chunk 可以按需生成，但生成请求、生成结果摘要和 schema/version 必须进入 chain commit 或等价可复放日志。
- runtime live 允许在本地预览未提交 chunk，但未提交资源只能标记为 `provisional`，不得进入正式采矿、生产、provider observation 或玩家可交易资源账本。
- `fragment_budget.remaining_*` 的减少必须由链上 action / commit 驱动；runtime/viewer 不得把本地重算结果当成 authoritative remaining balance。
- testnet readiness 必须能证明 viewer/runtime/provider 读取的是同一个链上 resource provenance，而不是各自本地重新生成后碰巧一致。

产品承诺接收：[既有正式资源的提交来源与余额连续性](../../product/world-rules-core-gameplay/prd.md#committed-resource-continuity)；仅上述 R6 原消费者结果，工程字段/算法仍归本专题，不改变适用 profile 或正式未决答案。

## 4. Non-Goals
- 本专题不新增复杂分布 DSL；继续复用现有 `AsteroidFragmentConfig`、material distribution 和 replenish 策略。
- 不在本专题定义完整经济平衡数值，只定义统一来源与接线合同。
- 不要求一次性删除所有 `runtime:x:y:z` 兼容代码；但正式可玩入口必须迁移到 unified seed path。

## 5. Acceptance Criteria
- AC1: 同一个 `world_seed + chunk coord` 在 simulator、runtime live snapshot 和 testnet replay 中生成一致 chunk/resource summary。
- AC2: formal release 默认入口首个可玩 snapshot 至少包含一个 seed-derived starter-safe location/fragment 或明确 empty-world onboarding state，不再只依赖坐标锚点。
- AC3: `claim_first_agent` 后 Agent 的 `location_id` 能解析到 seed-derived Location，且该 Location 具备可解释的 fragment/resource context。
- AC4: Viewer 与 provider observation 对同一 Agent 返回相同 `location_id`、附近可见资源摘要和世界 seed/version。
- AC5: 本地 testnet / public testnet readiness 证据能同时证明 `world_id`、`world_seed`、chain/genesis 与 viewer runtime snapshot 的一致性。
- AC6: starter chunk 的 seed/chunk/resource manifest 已在 genesis 或首个 chain commit 中出现，viewer snapshot 与 provider observation 都能引用该链上 provenance。
- AC7: 对同一 fragment 的采集会产生链上 resource delta；刷新或重放后 `remaining` budget 与链上 commit 一致，不会被 runtime 本地重算复原。
- AC8: 未提交的本地 preview chunk 在 UI/provider 中显示为 provisional 或不可用，不能被正式 gameplay action 消耗。


## 6. 既有采用子范围的身份与适用性

本节只登记 R1–R6 的现有采用子机制及其真实边界，不改变原要求、AC1–AC8、schema 或里程碑。正式统一世界的消费者义务不因局部工程身份而成为不适用；完整 chain-authoritative 接线仍按配对设计的四项 Open Decisions 与原验收逐 profile 核实。这里的“采用”表示已有合同/采用记录，不表示本次执行业务验证或整个正式目标完成。

<a id="uws-simulator-facts"></a>
### 既有专业条款身份：Simulator 生成事实与双级预算

Trace ID: O7-REQ-9a16e402-a0e7-45eb-8122-4741155843c2

R2 与 §接口/数据中的既有 Simulator 生成、fragment/chunk total/remaining 来源；配对设计 §2 已有能力、§3.2 派生链的实际 Simulator 子范围。这里只登记生成事实的输入与账本来源，不重订 chunk_seed 的参数命名、seed offset、公式或预算算法。

工程 owner 为 Simulator / chain runtime / artifact 维护者，具体分配见[对应设计](unified-world-seed-fragment-runtime.design.md#simulator-facts-design)；[验证计划](unified-world-seed-fragment-runtime.design.md#unified-verification-plan)分别保留 source 断言、待执行结果及未覆盖消费者。

<a id="uws-artifact-separation"></a>
### 既有专业条款身份：生成 artifact 与 governed world 的分离

Trace ID: O7-REQ-41fc7ce2-9979-43b2-be4f-1c7f8c80e58b

R1/R6 的 provenance 接入中已采用的 artifact 分离：generated-scenario-world 是生成场景 sidecar，audited governed runtime world 仍保留为独立权威材料；此范围由 #1797/#1801 采用记录限定。sidecar 的存在不证明正式 runtime 已消费其自然资源。

工程 owner 为 Simulator / chain runtime / artifact 维护者，具体分配见[对应设计](unified-world-seed-fragment-runtime.design.md#artifact-separation-design)；[验证计划](unified-world-seed-fragment-runtime.design.md#unified-verification-plan)分别保留 source 断言、待执行结果及未覆盖消费者。

<a id="uws-status-source"></a>
### 既有专业条款身份：最新 execution record / CAS 的 status 接收

Trace ID: O7-REQ-88cc07e6-ae98-4ecf-a173-39b235e39fe6

R6 与 /v1/chain/status.world_resource 的既有 status 子范围：优先最新 execution record/CAS 的资源快照，保留 genesis/recovery fallback；损坏或高度不一致的最新权威材料不得被旧 ready JSON 掩盖。#2762/#2766 为已有采用来源，不能据此选定正式生成/消费的业务 record。

工程 owner 为 Simulator / chain runtime / artifact 维护者，具体分配见[对应设计](unified-world-seed-fragment-runtime.design.md#status-source-design)；[验证计划](unified-world-seed-fragment-runtime.design.md#unified-verification-plan)分别保留 source 断言、待执行结果及未覆盖消费者。

<a id="uws-local-spawn-adoption"></a>
### 本地 frag 初始化的身份复用

Trace refs: O7-REQ-4d98352e-f146-4672-b159-b99545708c78, O7-REQ-3bef4fa6-57cd-41f6-baa3-74272ea8adcc；唯一工程声明沿[确定性出生选择](agent-frag-initial-spawn-position.prd.md#fsp-selection-contract)与[初始化 standoff/边界](agent-frag-initial-spawn-position.prd.md#fsp-standoff-contract)。#547 已让本地 formal-release 默认启动复用 seed-model 的 Agent 坐标，不能再把该局部 frag 优先选择写成未选。R3/R4 的正式 Agent location/provenance、附近资源、claim 等待/失败以及 chain commitment 仍由本专题原文拥有，不由本地坐标证明；[本地接线设计](unified-world-seed-fragment-runtime.design.md#local-spawn-adoption-design)只接收这一子范围。

<a id="uws-consumer-boundaries"></a>
### 原消费者义务与剩余范围

R1 的 canonical seed 跨入口可验证引用、R3/R4 的正式 starter/claim 接线、R5 的 Viewer/provider 同 snapshot、R6 的资源 committed/provisional 与恢复 remaining 都仍为适用要求，不能用上面三个工程身份替代。原 AC1–AC8 分别保留，具体目标分配与未选接线见[设计分配表](unified-world-seed-fragment-runtime.design.md#unified-allocation)。固定场景初始化只有限关联[SC-1](../../product/agents-world-simulation/prd.md#sc-agent-001)；[舞台事实来源](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)只提供 Viewer 读取依赖；[无效果待决](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002)只提供 committed 前消费者边界。它们均不替代全套统一 seed 产品接收，不把 SC-1 当正式链资源、claim 或 provider 的万能身份。
