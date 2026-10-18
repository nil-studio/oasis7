# oasis7 Simulator：统一大世界种子与 Fragment Runtime 复用（设计文档）

- 对应需求文档: `doc/world-simulator/scenario/unified-world-seed-fragment-runtime.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

## 1. 设计定位
本专题把“统一大世界”定义为一个 chain-authoritative、seed-derived world。所有环境只允许从同一个世界种子派生 chunk、fragment、资源预算和 starter spawn；正式资源事实必须由链上 genesis/commit 承载或引用。runtime live、viewer、provider bridge 与 testnet 节点是同一链上世界事实的消费者和提交者，不再各自发明地点和资源。

## 文档适用范围

原 §1–§9 保留正式统一世界目标及原决策问题；新增登记把已采用的局部机制与尚未核实的正式统一接线分开。2026-10-15 文档/源码基线为 `a90a1384fb94fc036c929125c84717e5614762ad`，只读核对，不运行生成器、节点、链、模型或业务测试。源码能证明当前机制，不能自行选择产品方案；原“缺口”按本次 profile 登记精确阅读，不泛化为完全没有 resource record 或所有 frag 出生均未选。

## 2. 现状对齐
已有能力：
- `chunk_seed(world_seed, coord)` 已提供稳定 chunk 随机根。
- `generate_chunk_fragments` 已把 chunk 生成落到 `WorldModel.locations`、`fragment_profile`、`fragment_budget` 与 `chunk_resource_budgets`。
- `chunked-fragment-generation` 统一定义 starter core、min fragments、material distribution 与 replenish 语义。

缺口：
- runtime live 的 formal release path 仍用固定 material/electricity bootstrap。
- runtime snapshot 映射层会从 Agent 坐标派生 `runtime:x:y:z` Location。
- first Agent claim 和 starter resource flow 尚未绑定 seed-derived starter chunk。
- chunk/resource 生成目前可以在 simulator/runtime 本地完成；正式链路还缺少 chain commit 级的 resource provenance、remaining budget delta 与 replay 验证合同。

<a id="unified-allocation"></a>
### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [Simulator 生成事实](unified-world-seed-fragment-runtime.prd.md#uws-simulator-facts) | 既有生成输入、双级预算与 snapshot 投影来源，不新增算法 | [Simulator 来源](#simulator-facts-design) | Simulator / chain schema；chunked 原合同 | 产品 N/A；reason=本行仅登记 Simulator 内生成输入/双级预算投影的既有工程接线，正证为原 R2/§2/§3.2 与实际 from_simulator_model；scope=当前离线模型数据来源，不含可采权、跨消费者或正式资源产品验收，后者下列单列 |
| [本地初始化身份复用](unified-world-seed-fragment-runtime.prd.md#uws-local-spawn-adoption) | #547 本地 seed-model Agent 坐标；frag 优先/standoff 沿原 owner | [本地接线](#local-spawn-adoption-design) | Simulator init / Viewer runtime live | 产品 N/A；reason=本行只接收 #547 本地 bootstrap 使用既有初始化模型坐标的内部接线；scope=默认构建入口与 Agent 坐标，不含玩家 claim、location/provenance、资源或可见性；正式消费者下列单列 |
| [artifact 分离](unified-world-seed-fragment-runtime.prd.md#uws-artifact-separation) | #1797/#1801 的 generated sidecar 与 governed world 分离 | [artifact](#artifact-separation-design) | artifact 工具 / governed activation | 产品 N/A；reason=本行是 #1797 已选 artifact 工具目录/输入来源分离，正证为采用记录及 build/validate 双路径；scope=离线工件边界，不含正式世界生成、激活或消费者，后者沿 R1/R6 单列 |
| [status 来源](unified-world-seed-fragment-runtime.prd.md#uws-status-source) | #2762/#2766 最新 CAS 与真实 fallback/reject | [status](#status-source-design) | chain runtime 状态读面 | 产品 N/A；reason=本行只接收 #2762 的既有 status 后端权威材料读取与完整性校验；scope=读取来源和失败回退，不含玩家资源权利、正式业务 record、共识或自然资源接线，后者沿 R6 单列 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r1-统一世界种子) | canonical seed、跨入口同世界引用及可验证来源  原 AC 与其 profile 条件仍适用。 | [对应条款](#31-seed-manifest) | genesis / manifest / runtime / testnet / launcher / Viewer / provider | 当前三个 seed 机制不能代签正式一致；具体 genesis 承载未选 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r2-复用现有-chunkfragmentresource-生成) | 复用生成与 total/remaining，不临时重算可采量  原 AC 与其 profile 条件仍适用。 | [对应条款](#32-seed-派生) | Simulator / runtime resource consumers | 不以 Simulator fixture 证明所有正式 profile 已统一 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r3-runtime-live-消费世界生成事实) | starter 来源、正式地点/附近资源、claim 等待失败及 grant 分离  原 AC 与其 profile 条件仍适用。 | [对应条款](#51-world-bootstrap) | runtime / claim / Viewer；正式 starter 选择和逻辑地点待定 | 当前本地坐标不能覆盖玩家正式接线；原明确 empty-world 状态与错误边界保留 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r4-新用户入口) | starter 来源、正式地点/附近资源、claim 等待失败及 grant 分离  原 AC 与其 profile 条件仍适用。 | [对应条款](#unified-claim-flow) | runtime / claim / Viewer；正式 starter 选择和逻辑地点待定 | 当前本地坐标不能覆盖玩家正式接线；原明确 empty-world 状态与错误边界保留 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r5-viewer-与-provider-观察一致) | 同 snapshot 的 location/resource IDs、seed/version 与摘要  原 AC 与其 profile 条件仍适用。 | [对应条款](#unified-observation) | Viewer / provider；[舞台来源有限依赖](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | 未核实同候选两消费者真实输出，不能用 DTO 或工程身份代签 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r6-资源生成全部链上化) | committed 生成/消费事实、provisional 不消耗、refresh/replay 不复原 remaining  原 AC 与其 profile 条件仍适用。 | [对应条款](#43-资源变更权威) | chain / runtime / Viewer / provider；[无效果待决有限依赖](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002) | 业务 record 未选、正式端到端未验证；不是整个 DWE-002 或 finality 验收 |

## 3. Canonical World Seed Contract
### 3.1 Seed Manifest

Trace refs: O7-REQ-f8497929-34dc-4193-a435-7dbe60bf0e4c；本条有限承接[既有产品消费者](../../product/agents-world-simulation/prd.md#canonical-seed-consumer)，只沿原 R1 与对应 AC 范围；正式 profile 未定接线不由本条或本地实现代选。

每个统一大世界需要一个 seed manifest，至少包含：
- `world_id`
- `world_seed`
- `chunk_generation_schema_version`
- `asteroid_fragment_seed_offset`
- `world_config_hash`
- `genesis_ref` / `chain_id`（testnet/public 环境）

该 manifest 是 runtime live、chain runtime、viewer snapshot 和 provider observation 的共同输入或可验证引用。

### 3.2 Seed 派生

Trace refs: O7-REQ-112440b1-da55-4443-b513-58dbf9056511；本条有限承接[既有产品消费者](../../product/world-rules-core-gameplay/prd.md#generated-resource-source)，只沿原 R2 与对应 AC 范围；正式 profile 未定接线不由本条或本地实现代选。

统一派生链：

```text
world_seed
  -> asteroid_fragment_seed = world_seed + seed_offset
  -> chunk_seed(asteroid_fragment_seed, chunk_coord)
  -> generate_fragments(seed, chunk_bounds, AsteroidFragmentConfig)
  -> synthesize_fragment_profile(profile_seed, radius, material)
  -> synthesize_fragment_budget(fragment_profile)
```

任何 runtime-only 随机源、系统时钟随机、进程启动随机都不得参与正式资源生成。

## 4. Chain-Authoritative Resource Contract
### 4.1 链上资源事实
正式资源生成分为两个阶段：

1. Deterministic derivation：由 `world_seed + chunk_coord + chunk_generation_schema_version` 派生 fragment/profile/budget。
2. Chain commitment：把该 derivation 的结果摘要写入 genesis、chain commit 或可复放 execution record。

进入正式玩法的 chunk 必须有链上资源事实记录，至少包含：
- `world_id`
- `world_seed` 或 seed manifest hash
- `chunk_coord`
- `chunk_generation_schema_version`
- `asteroid_fragment_seed_offset`
- `fragment_ids`
- per-fragment `profile_hash`
- per-fragment `budget_total_hash`
- per-fragment `budget_remaining_hash`
- `chunk_resource_budget_hash`
- `commit_height` / `commit_hash` / `execution_record_ref`

### 4.2 Provenance 状态
资源地点必须显式标记 provenance：

- `chain_committed`: 已进入 genesis/commit，可用于正式采矿、生产、交易、provider observation。
- `chain_pending`: 已提交生成或变更请求，但尚未 committed；UI 可以展示等待态，provider 不得把它当作可消耗资源。
- `provisional`: runtime 本地预览或测试 fixture；只能用于调试/视觉预览，不得进入正式玩法账本。
- `compat_anchor`: `runtime:x:y:z` 坐标锚点；不是正式资源地点。

### 4.3 资源变更权威

Trace refs: O7-REQ-2b3769ae-078a-4123-a0de-e20405b42c7c；本条有限承接[既有产品消费者](../../product/world-rules-core-gameplay/prd.md#committed-resource-continuity)，只沿原 R6 与对应 AC 范围；正式 profile 未定接线不由本条或本地实现代选。

`remaining` budget 的变化必须来自链上 action / commit：
- 采矿、回收、补种、生产消耗和 material ledger 增量都必须产生可重放的 resource delta。
- runtime 可以先做 optimistic/pending 显示，但 committed snapshot 必须以链上 delta 为准。
- viewer 刷新、provider observation 和 testnet replay 不得通过重新运行 generation 还原 `remaining`，只能从 committed delta 或 checkpoint 恢复。

## 5. Runtime Live 接入设计
### 5.1 World Bootstrap

Trace refs: O7-REQ-212aa59b-6d97-43a7-8fd7-35d441489281；本条有限承接[既有产品消费者](../../product/agents-world-simulation/prd.md#formal-generated-world-entry)，只沿原 R3 与对应 AC 范围；正式 profile 未定接线不由本条或本地实现代选。

formal release 默认世界启动时：
1. 加载或构造 seed manifest。
2. 从 chain/genesis/manifest 获取 starter chunk provenance；如果缺失，只能生成并提交 pending chunk，不得直接进入正式资源账本。
3. 初始化 `WorldConfig` 与 `WorldInitConfig`，启用 asteroid fragment。
4. 生成或验证 starter-safe chunk：
   - 默认选择世界中心 chunk；
   - 或从 manifest 指定 `starter_chunk_coord`；
   - 使用现有 `generate_chunk_fragments` 生成资源；
   - 比对链上 `profile_hash`、`budget_total_hash` 与 `chunk_resource_budget_hash`。
5. 从 starter chunk 内选择 spawn anchor：
   - 优先选择具备可采 budget 的 frag location；
   - 若无可用 frag，则按确定性 fallback 选择 chunk 中心并标记 `starter_chunk_resource_missing`。
6. 注册 starter Agent 到该 spawn anchor，Agent `location_id` 必须指向 seed-derived、chain-committed Location 或明确 fallback reason。

### 5.2 Location 语义
- `frag-*` / seed-derived / chain-committed Location：正式资源地点，可被 viewer、provider、采矿/生产链路引用。
- `runtime:x:y:z`：兼容坐标锚点，只能用于过渡、调试和无资源动作，不应作为正式可采地点。
- Viewer 需要把两者视觉区分：正式资源地点显示资源摘要；坐标锚点显示 fallback/compat badge。

### 5.3 Resource Ledger
正式资源读取顺序：
1. chain-committed resource delta / checkpoint
2. `Location.fragment_budget.remaining_by_element_g` with matching chain provenance
3. `WorldModel.chunk_resource_budgets[coord].remaining_by_element_g` with matching chain provenance
4. 已采矿/加工后的 runtime material ledger

固定 bootstrap material 只能作为 migration / test fixture / grant，不得被描述为世界自然资源。

<a id="unified-claim-flow"></a>
## 6. Claim / Onboarding Flow

Trace refs: O7-REQ-7853375e-94b6-4c6c-8d6b-c486a4277dd4；本条有限承接[既有产品消费者](../../product/agents-world-simulation/prd.md#first-claim-generated-context)，只沿原 R4 与对应 AC 范围；正式 profile 未定接线不由本条或本地实现代选。

`claim_first_agent` 改为：
1. 确认世界 seed manifest 已加载。
2. 确认 starter chunk 已 chain-committed；若未 committed，则提交生成请求并返回等待态。
3. 选择 starter-safe spawn location。
4. 注册/绑定 Agent。
5. 发放 OC/LLM starter grant。
6. 将玩家引导到可见 starter resource pocket 的下一步操作。

如果任一步失败，返回明确错误：
- `world_seed_missing`
- `starter_chunk_generation_failed`
- `starter_chunk_not_committed`
- `starter_resource_provenance_missing`
- `starter_resource_pocket_missing`
- `starter_spawn_location_missing`

<a id="unified-observation"></a>
## 7. Viewer / Provider Observation

Trace refs: O7-REQ-fd8b7e31-afa8-4361-81f0-849c07099dfd；本条有限承接[既有产品消费者](../../product/agents-world-simulation/prd.md#shared-snapshot-resource-observation)，只沿原 R5 与对应 AC 范围；正式 profile 未定接线不由本条或本地实现代选。

Snapshot 新增或确保暴露：
- `world_seed` 或 `world_seed_ref`
- `chunk_generation_schema_version`
- per-location `position_source`: `seed_fragment` / `runtime_coord_anchor` / `fallback`
- per-location `resource_provenance`: `chain_committed` / `chain_pending` / `provisional` / `compat_anchor`
- per-location `resource_commit_ref`
- nearby resource summary: dominant material, remaining key elements, chunk coord

Provider observation 使用同一 snapshot 派生，禁止单独查本地缓存生成不同资源摘要。

## 8. Migration Plan
### M1 文档与 contract
- 增加本专题 PRD/design/project。
- 在 runtime live/formal release 文档中引用 seed-authoritative contract。
- 明确 chain-authoritative resource contract。

### M2 Snapshot contract
- 为 runtime live snapshot 增加 seed/version/resource provenance 字段。
- Viewer 显示正式资源地点与 compat anchor 区分。

### M3 Runtime bootstrap
- formal release 默认世界改为 seed manifest bootstrap。
- `claim_first_agent` 改为 starter chunk spawn。

### M4 Chain resource commitment
- genesis 预提交 starter chunk resource manifest。
- chunk 按需生成时提交 `generate_chunk` action/result。
- 采矿/生产/补种提交 resource delta，committed snapshot 从 chain delta 恢复 remaining budget。

### M5 Testnet evidence
- public/local testnet readiness 增加 world seed 与 chunk generation schema 验证。
- provider bridge smoke 增加 observation seed/resource provenance 校验。
- 增加 refresh/replay 后 remaining budget 不被本地重算复原的验证。

## 9. Open Decisions
- `world_seed` 在现有 public_testnet genesis 中的字段名与存储位置。
- starter chunk 默认是否固定为世界中心，还是由 manifest 显式指定。
- first Agent 是否直接出生在 frag location，还是出生在附近 logical base location 并把 frag 作为 nearby resource。
- resource commitment 是写入现有 chain action/event schema，还是新增专用 `ChunkResourceManifest` / `ResourceDelta` execution record。


<a id="unified-profile-register"></a>
## 10. 已采用 profile 与正式剩余范围

<a id="simulator-facts-design"></a>
### 10.1 Simulator 生成与预算来源

Trace refs: O7-REQ-9a16e402-a0e7-45eb-8122-4741155843c2；唯一工程声明与范围见下方对应 PRD 身份。

[既有生成事实身份](unified-world-seed-fragment-runtime.prd.md#uws-simulator-facts)只接收当前 Simulator 子范围。[init.rs](../../../crates/oasis7_simulator/src/init.rs)、[chunking.rs](../../../crates/oasis7_simulator/src/chunking.rs)和[chain_resource_schema.rs](../../../crates/oasis7/src/chain_resource_schema.rs)分别定位真实初始化、确定性 chunk 派生和 from_simulator_model 投影。fragment/chunk total/remaining 仍沿[chunked 原专业需求](chunked-fragment-generation.prd.md)，不从 snapshot 字段重新发明资源账本。当前 runtime_agent_chunk_v1 路径从 world/chain/config hash 派生 seed，构造 runtime-starter refs 与预算；它与 Simulator 路径共用 DTO 不证明两者统一成同一自然资源生成事实。

产品消费者仍适用 R1/R2/R5/R6 及 AC1/AC4/AC5/AC7；工程投影来源不是它们的端到端验收。CG6/CG10 的 ChunkGenerated 重建校验与 CG7 的泛称生成/补种 delta 义务存在未裁决冲突；本专题 §4.3 禁止重生成还原 remaining，不自动决定初始几何事件应重建还是携带完整布局。本登记不替任一旧义务宣布过时。

<a id="local-spawn-adoption-design"></a>
### 10.2 本地 formal-release 默认初始化

[已有 #547](https://github.com/nil-studio/oasis7/pull/547)接收本地 Simulator seed/fragment spawn，chain-authoritative resource 保留后续范围。[gameplay_actions.rs](../../../crates/oasis7/src/viewer/gameplay_actions.rs)的默认 seed、bootstrap `(0,0,0)` 与 build_world_model 接线，以及[support.rs](../../../crates/oasis7/src/viewer/runtime_live/support.rs)的本地 starter Agent，属于已采用子范围。frag 优先与上方 standoff 的唯一身份沿[原出生要求](unified-world-seed-fragment-runtime.prd.md#uws-local-spawn-adoption)，不重复声明。

support 仍有固定 material/electricity/Data 与 factory bootstrap；grant 不变为自然资源。当前取模型 Agent 坐标不证明正式 claim 的 location_id、nearby resource、commit provenance 与 fail-closed 已绑定。坐标零不等于“世界中心”的产品定义，不能把本地常量升级成所有正式 profile 的 starter 默认选择。

<a id="artifact-separation-design"></a>
### 10.3 Generated sidecar 与 governed runtime world

Trace refs: O7-REQ-41fc7ce2-9979-43b2-be4f-1c7f8c80e58b；唯一工程声明与范围见下方对应 PRD 身份。

[分离身份](unified-world-seed-fragment-runtime.prd.md#uws-artifact-separation)由[#1797 采用记录](https://github.com/nil-studio/oasis7/issues/1797#issuecomment-4867727910)及[#1801](https://github.com/nil-studio/oasis7/pull/1801)支持。[build/validate 脚本](../../../scripts/p2p-build-governed-bootstrap-world.sh)保留 audited governed runtime world，将生成场景写入 generated-scenario-world；[generator](../../../crates/oasis7_ops_tools/src/bin/oasis7_generate_world_artifact.rs)保存 scenario/seed/config/public manifest provenance。artifact 生成成功或 sidecar 存在不代替 governed world 激活，也不证明 public genesis 已选择 canonical seed 字段、starter 或自然资源承载。

[world_initialization.rs](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/world_initialization.rs)接收 externally prepared world/snapshot/journal/issuer/approval，并不创建 World 或制造身份；这是真实正式激活信任边界，不是业务生成方案的选择记录。

<a id="status-source-design"></a>
### 10.4 当前 resource status 的权威来源接收

Trace refs: O7-REQ-88cc07e6-ae98-4ecf-a173-39b235e39fe6；唯一工程声明与范围见下方对应 PRD 身份。

[status 来源身份](unified-world-seed-fragment-runtime.prd.md#uws-status-source)由[#2762](https://github.com/nil-studio/oasis7/issues/2762)与[#2766](https://github.com/nil-studio/oasis7/pull/2766)接收。[status_payload_world_resource.rs](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/status_payload_world_resource.rs)优先最新 execution record/CAS；其损坏、无效或高度不一致路径拒绝用旧 ready 材料掩盖，合法 genesis/recovery fallback 仍保留。这里接收的是 status 读取决定，不是新 resource execution record schema、正式生成动作或最终性协议选择。

原 R6/AC6/AC7 要求的生成、消费、提交、恢复 remaining 与跨消费者 provenance 仍需单独验证。latest manifest/delta 有字段、有 hash 或 status ready，都不能据此宣称自然资源 chain-authoritative 全部落地。

<a id="unified-decision-profile"></a>
### 10.5 四个 Open Decisions 的 profile 边界

§9 四句原文与答案保持未定；以下只收窄尚未核实选定的正式 unified 范围，不重新待选已采用局部合同。

| 原问题 | 已采用子范围 | 正式 unified profile 尚未核实的最小剩余 |
| --- | --- | --- |
| public_testnet genesis 的 world_seed 位置 | Simulator seed、sidecar provenance 与 runtime derived seed 均存在；canonical 可验证引用仍是原目标 | 谁提供 canonical seed，放 genesis 哪个字段或被其承诺的 manifest，以及正式消费者如何绑定；字段存在不是位置选择 |
| starter 中心还是 manifest 指定 | 本地默认 bootstrap `(0,0,0)` 与 Simulator bootstrap_chunks 已采用 | 正式 profile 的选择权、优先级与绑定；status 的 map 首项不是中心选择批准 |
| first Agent frag 或 logical base | build_world_model/本地默认入口已采用 frag 优先与 standoff | 正式 chain-authoritative Agent 逻辑地点、nearby resource、commit provenance 与 fail-closed 是否沿用同模型；RegisterAgent 坐标不够证明 |
| existing action/event 或专用 record | manifest/delta DTO、snapshot、Simulator event 派生以及最新 CAS/status 接收已有采用 | 正式生成/消费事实用何业务 record、结果如何 committed、各消费者/replay 如何引用；不由共用 DTO 或 CAS 自行选择 |

<a id="unified-verification-plan"></a>
## 11. 验证设计与可追溯性

### 11.1 验证映射表

全部为 planned，本次没有业务执行结果。执行时固定 candidate commit、真实 profile、world/chain/genesis、有效 seed/config/version、触发顺序和 snapshot/record 引用，记录命令、环境及原输出；源码断言不是运行通过。下面具名命令只定位现有离线测试，不创建新的 test 或补造端到端结果。

| 上游 requirement（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与条件 | 准确验证方法、test/manual source、scenario/layer | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [生成事实](unified-world-seed-fragment-runtime.prd.md#uws-simulator-facts) | [Simulator](#simulator-facts-design) | 生成 chunk 与最新 delta 的 snapshot 引用 | [persist.rs](../../../crates/oasis7/src/simulator/tests/persist.rs)：`cargo test -p oasis7 --lib chain_resource_snapshot_records_generated_chunk_and_latest_delta`；固定离线 fixture，核 manifest/delta 与模型引用 | 命令、candidate、输入与原断言输出 | 不证明 formal seed 接线、全部守恒或实际执行通过 |
| [本地初始化](unified-world-seed-fragment-runtime.prd.md#uws-local-spawn-adoption) | [本地接线](#local-spawn-adoption-design) | 默认入口 starter 坐标与 seed model 一致 | [具名本地初始化计划](../../../testing-manual.md#uws-v-local-spawn)；原 support 内测试的实际 fixture 与坐标断言，先核 discovery，未运行 | candidate/default config、测试名、坐标与输出；0 tests 不算通过 | 不证明正式 location_id、chain 资源、玩家可见性或 claim 完整流程 |
| [artifact 分离](unified-world-seed-fragment-runtime.prd.md#uws-artifact-separation) | [artifact](#artifact-separation-design) | generated sidecar 与 governed 材料分离 | [UWS-V-ARTIFACT](../../../testing-manual.md#uws-v-artifact)；原 build/validate 输入与目录/provenance/audit 对账，未运行 | 两目录、provenance、audited manifest、命令与退出码 | 不证明 public genesis seed 位置或正式 runtime 已消费 sidecar |
| [status 来源](unified-world-seed-fragment-runtime.prd.md#uws-status-source) | [status](#status-source-design) | 最新 CAS 优先，损坏/高度不一致不能退旧 ready | [UWS-V-STATUS](../../../testing-manual.md#uws-v-status)；真实 bin/features/filter、CAS 正负例与原断言，未运行 | 正/负例 test discovery 与原输出、CAS refs 与 height | 不证明正式 record 选择或自然资源业务全通过 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r1-统一世界种子) | [对应条款](#31-seed-manifest) | AC1/AC5 同世界 seed 与生成摘要  原 AC 与其 profile 条件仍适用。 | [独立逐项计划](../../../testing-manual.md#uws-v-seed-reference)；按该计划固定输入、正反例、原合同 oracle 与输出，未执行 | 同候选来源链、三输出与差异 | 当前只具备局部 fixture，正式选择及验证未完成 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r2-复用现有-chunkfragmentresource-生成) | [对应条款](#32-seed-派生) | 原 R2 当前 total/remaining 与生成来源；非零消费、缺预算及错 chunk 负例，不以临时重算代当前可采量。 | [独立逐项计划](../../../testing-manual.md#uws-v-resource-budget)；按该计划固定输入、正反例、原合同 oracle 与输出，未执行 | 同候选来源链、三输出与差异 | 当前只具备局部 fixture，正式选择及验证未完成 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r3-runtime-live-消费世界生成事实) | [对应条款](#51-world-bootstrap) | AC2/AC3 正常、empty-world 及错误/等待不假成功  原 AC 与其 profile 条件仍适用。 | [UWS-V-CONSUMERS](../../../testing-manual.md#uws-v-consumers)正常及缺 manifest、未 committed、无 starter resource 三组输入；逐步核 §6 输出与 location 可解析性 | Agent/location/context/commit、错误/等待、grant 来源 | 正式 starter/location 选择及真实测试接线未核实；本地坐标断言不代签 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r4-新用户入口) | [对应条款](#unified-claim-flow) | AC2/AC3 正常、empty-world 及错误/等待不假成功  原 AC 与其 profile 条件仍适用。 | [UWS-V-CONSUMERS](../../../testing-manual.md#uws-v-consumers)正常及缺 manifest、未 committed、无 starter resource 三组输入；逐步核 §6 输出与 location 可解析性 | Agent/location/context/commit、错误/等待、grant 来源 | 正式 starter/location 选择及真实测试接线未核实；本地坐标断言不代签 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r5-viewer-与-provider-观察一致) | [对应条款](#unified-observation) | AC4 两消费者同 snapshot 同 IDs/摘要  原 AC 与其 profile 条件仍适用。 | [独立逐项计划](../../../testing-manual.md#uws-v-observation-parity)；按该计划固定输入、正反例、原合同 oracle 与输出，未执行 | 两输出、snapshot/Agent/权限引用与差异 | 待接真实消费者 harness；不伪造已存在测试或玩家结果 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r6-资源生成全部链上化) | [对应条款](#43-资源变更权威) | AC6/AC7/AC8 committed provenance、remaining 恢复和 provisional 拒绝  原 AC 与其 profile 条件仍适用。 | [UWS-V-CONSUMERS](../../../testing-manual.md#uws-v-consumers)记录生成→提交→消费→刷新→replay；provisional 同动作负例；与原 §4.3、§8 M4/M5 比较 | record、delta、前后 remaining、拒绝与跨消费者引用 | status tests 不代签；正式业务 record 未选不得伪造执行结果 |

### 11.2 计划执行入口

[artifact 计划](../../../testing-manual.md#uws-v-artifact)、[status 计划](../../../testing-manual.md#uws-v-status)、[本地初始化计划](../../../testing-manual.md#uws-v-local-spawn)与[正式消费者计划](../../../testing-manual.md#uws-v-consumers)明确候选、命令/输入、断言、输出和阻塞；计划存在不等于执行完成。


## 12. 风险与未决边界

四个正式 profile 接线问题继续由 §9/§10.5 保留，已采用子范围不重新待选。原消费者、资源链上生成/消费与跨入口验证仍适用；缺精确产品接收身份或实施验证时记录真实关系债务，不用 N/A 抹掉，不从局部源码或文档检查得出完整规范闭合。
