# oasis7 Simulator：统一大世界种子与 Fragment Runtime 复用（设计文档）

- 对应需求文档: `doc/world-simulator/scenario/unified-world-seed-fragment-runtime.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

## 1. 设计定位
本专题把“统一大世界”定义为一个 chain-authoritative、seed-derived world。所有环境只允许从同一个世界种子派生 chunk、fragment、资源预算和 starter spawn；正式资源事实必须由链上 genesis/commit 承载或引用。runtime live、viewer、provider bridge 与 testnet 节点是同一链上世界事实的消费者和提交者，不再各自发明地点和资源。

## 文档适用范围

本设计保留 R1–R6、AC1–AC8 与既有采用范围，现于 [§9](#unified-decisions)明确正式统一世界的四项接线选择，并与[初始生成及后续增量恢复](chunked-fragment-generation.design.md#chunk-replay-conflict-register)一起解释。选择是待实施的设计目标，不是正式工件、代码或验收已经完成的声明。

2026-11-05 静态审读固定源码 `cca8cda97cf121ebc1e25e96d6b0204a421323ea`，未运行生成器、节点、链、模型或业务测试。§10 保留原 `a90a1384fb94fc036c929125c84717e5614762ad` 审读登记及采用记录的范围，当前源码定位按本次基线更新。设计选择不让本地生成、sidecar 或 DTO 自动获得正式世界权威，不新建部署 profile、资源权限或另一份持久账本。

## 2. 现状对齐
已有能力：
- `chunk_seed(world_seed, coord)` 已提供稳定 chunk 随机根。
- `generate_chunk_fragments` 已把 chunk 生成落到 `WorldModel.locations`、`fragment_profile`、`fragment_budget` 与 `chunk_resource_budgets`。
- `chunked-fragment-generation` 统一定义 starter core、min fragments、material distribution 与 replenish 语义。

缺口：
- runtime live 的 formal release path 仍用固定 material/electricity bootstrap。
- runtime snapshot 映射层会从 Agent 坐标派生 `runtime:x:y:z` Location。
- first Agent claim 和 starter resource flow 尚未绑定 seed-derived starter chunk。
- chunk/resource 生成及 manifest/delta DTO 已有局部实现；正式自然资源写端、既有正式提交链的接入与跨消费者验收仍未核实。下文选择承载与恢复规则，不把这些缺口重新解释为完全没有 resource record。

<a id="unified-allocation"></a>
### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [Simulator 生成事实](unified-world-seed-fragment-runtime.prd.md#uws-simulator-facts) | 既有生成输入、双级预算与 snapshot 投影来源，不新增算法 | [Simulator 来源](#simulator-facts-design) | Simulator / chain schema；chunked 原合同 | 产品 N/A；reason=本行仅登记 Simulator 内生成输入/双级预算投影的既有工程接线，正证为原 R2/§2/§3.2 与实际 from_simulator_state；scope=当前离线模型数据来源，不含可采权、跨消费者或正式资源产品验收，后者下列单列 |
| [本地初始化身份复用](unified-world-seed-fragment-runtime.prd.md#uws-local-spawn-adoption) | #547 本地 seed-model Agent 坐标；frag 优先/standoff 沿原 owner | [本地接线](#local-spawn-adoption-design) | Simulator init / Viewer runtime live | 产品 N/A；reason=本行只接收 #547 本地 bootstrap 使用既有初始化模型坐标的内部接线；scope=默认构建入口与 Agent 坐标，不含玩家 claim、location/provenance、资源或可见性；正式消费者下列单列 |
| [artifact 分离](unified-world-seed-fragment-runtime.prd.md#uws-artifact-separation) | #1797/#1801 的 generated sidecar 与 governed world 分离 | [artifact](#artifact-separation-design) | artifact 工具 / governed activation | 产品 N/A；reason=本行是 #1797 已选 artifact 工具目录/输入来源分离，正证为采用记录及 build/validate 双路径；scope=离线工件边界，不含正式世界生成、激活或消费者，后者沿 R1/R6 单列 |
| [status 来源](unified-world-seed-fragment-runtime.prd.md#uws-status-source) | #2762/#2766 最新 CAS 与真实 fallback/reject | [status](#status-source-design) | chain runtime 状态读面 | 产品 N/A；reason=本行只接收 #2762 的既有 status 后端权威材料读取与完整性校验；scope=读取来源和失败回退，不含玩家资源权利、正式业务 record、共识或自然资源接线，后者沿 R6 单列 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r1-统一世界种子) | canonical seed、跨入口同世界引用及可验证来源；原 AC 与适用条件保留 | [seed 绑定](#unified-seed-binding) | 外部初始工件制作方 / 原正式初始化与授权 / runtime / launcher / Viewer / provider | 已选既有 manifest 随初始 checkpoint 承诺；当前三个 seed 机制不能代签正式一致 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r2-复用现有-chunkfragmentresource-生成) | 复用生成与 total/remaining，不临时重算可采量  原 AC 与其 profile 条件仍适用。 | [对应条款](#32-seed-派生) | Simulator / runtime resource consumers | 不以 Simulator fixture 证明所有正式 profile 已统一 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r2-复用现有-chunkfragmentresource-生成) | 复用生成与 total/remaining，不临时重算可采量  原 AC 与其 profile 条件仍适用。 | [双级预算核对](#53-resource-ledger) | Simulator / runtime resource consumers | 不以 Simulator fixture 证明所有正式 profile 已统一 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r3-runtime-live-消费世界生成事实) | starter 来源、正式地点/附近资源、claim 等待失败及 grant 分离；原 AC 与适用条件保留 | [正式启动](#51-world-bootstrap) | runtime / claim / Viewer；固定中心 chunk 与可采 frag 逻辑地点 | 已选规则不证明正式接线；原明确 empty-world 状态与错误边界保留 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r4-新用户入口) | starter 来源、正式地点/附近资源、claim 等待失败及 grant 分离；原 AC 与适用条件保留 | [首次认领](#unified-claim-flow) | 原 Agent 注册/归属与独立 starter grant 消费者 | 不改变身份与 grant 经济合同；本地坐标不能代签正式 claim |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r5-viewer-与-provider-观察一致) | 同 snapshot 的 location/resource IDs、seed/version 与摘要  原 AC 与其 profile 条件仍适用。 | [对应条款](#unified-observation) | Viewer / provider；[舞台来源有限依赖](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | 未核实同候选两消费者真实输出，不能用 DTO 或工程身份代签 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r6-资源生成全部链上化) | committed 生成/消费事实、provisional 不消耗、refresh/replay 不复原 remaining；原 AC 与适用条件保留 | [资源事实承载](#unified-resource-record) | chain / runtime / Viewer / provider；[无效果待决有限依赖](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002) | 已选现有正式 action/result/commit；真实写端与端到端未验证，不是整个 DWE-002 或 finality 验收 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r6-资源生成全部链上化) | committed 生成/消费事实、provisional 不消耗、refresh/replay 不复原 remaining；原 AC 与适用条件保留 | [余额恢复](#43-资源变更权威) | chain / runtime / Viewer / provider；[无效果待决有限依赖](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002) | 已选现有正式 action/result/commit；真实写端与端到端未验证，不是整个 DWE-002 或 finality 验收 |

## 3. Canonical World Seed Contract
### 3.1 Seed Manifest

Trace refs: O7-REQ-f8497929-34dc-4193-a435-7dbe60bf0e4c；承接[既有产品消费者](../../product/agents-world-simulation/prd.md#canonical-seed-consumer)，只沿原 R1 与对应 AC 范围。以下选定正式来源，实施与跨入口验收分别保留。

每个统一大世界需要一个 seed manifest，至少包含：
- `world_id`
- `world_seed`
- `chunk_generation_schema_version`
- `asteroid_fragment_seed_offset`
- `world_config_hash`
- `genesis_ref` / `chain_id`（testnet/public 环境）

选用现有 `oasis7.world_resource_manifest.v1` 的 `world_seed` 作为 canonical 值；正式初始 snapshot 的 `chain_resource_manifest` 承载它，genesis/初始激活记录以已经承诺的 checkpoint/state root 引用该 snapshot。不再在 public-testnet 示例 JSON 或客户端配置放置另一个可覆盖的 canonical seed。上列 seed offset 是固定生成上下文的一部分，不宣称当前 manifest 有同名顶层字段。

该 manifest 是 runtime live、chain runtime、Viewer snapshot 和 provider observation 的共同输入或可验证引用。seed 为 `0` 并不等于缺失；是否可用由完整来源和绑定校验决定，不能用默认值或字段存在代替证明。

<a id="unified-seed-binding"></a>
#### 固定输入、制作与接收

1. 原外部初始工件制作方提供 world/chain/genesis 身份、明确的 `world_seed`、有效 `WorldConfig`、`ChunkRuntimeConfig`、生成版本及所选生成实现。原初始化/授权方按已有 issuer、approval、activation 规则接收。客户端、Launcher、status handler 和首次 claim 不选择或重置 seed，也不从 world ID/hash 派生一个替代值。
2. `world_config_hash` 继续校验有效 `WorldConfig`；当前 Simulator 的 `generation_algorithm_hash` 是有效 config 与 chunk runtime 的内容摘要，不是生成器二进制 hash。生成实现仍由已绑定的执行材料/版本定位。seed offset、启用位与 spacing 来自同一 `ChunkRuntimeConfig`，不能沿重启后的默认配置补齐。实际配置字节和恢复所需上下文必须进入既有初始工件/执行材料的已声明闭包；仅有 hash、文件名或旁置 sidecar 不足以采用。
3. 接收前核对 manifest 的 world/chain/genesis、seed、生成版本、配置摘要及 generated chunks；`world_seed` 与固定 chunk runtime 必须一致。正式初始 snapshot 的 typed 恢复不能改变这些值。缺输入、未知版本、摘要不匹配或其他世界材料均阻塞，不能回退到 `runtime_agent_chunk_v1`、本地固定 seed 或生成一个“相同外观”的世界。
4. 后续同世界记录继承上述生成上下文；允许变化的是已提交的资源状态。替换 seed、空间尺寸或生成上下文不是普通刷新/claim 的恢复方式。旧世界沿自己的已承诺历史恢复，本选择不把它就地重建为新开局；确需改变时沿原世界迁移/治理合同另作明确变更。

当前 [manifest DTO](../../../crates/oasis7_agent_api/src/world_resource/manifest.rs)已有 world/chain/genesis、seed、配置与生成摘要字段；[正式初始检查点](../../../crates/oasis7_chain_runtime/src/host/execution_bridge/formal_initial.rs)核身份并把整个 snapshot 纳入 state root，[初始化入口](../../../crates/oasis7_chain_runtime/src/host/world_initialization.rs)只恢复外部准备的 snapshot/journal，不创建 World 或授权。它们提供可复用的承载与信任边界；固定生成上下文、真实自然资源内容及逐消费者引用仍须实际接入，不能声称这些文件已经验证完整 unified seed。

### 3.2 Seed 派生

Trace refs: O7-REQ-112440b1-da55-4443-b513-58dbf9056511；承接[既有产品消费者](../../product/world-rules-core-gameplay/prd.md#generated-resource-source)，只沿原 R2 与对应 AC 范围；统一来源的实现仍须核实。

统一派生链：

```text
world_seed
  -> asteroid_fragment_seed = world_seed.wrapping_add(seed_offset)
  -> chunk_seed(asteroid_fragment_seed, chunk_coord)
  -> generate_fragments(seed, chunk_bounds, AsteroidFragmentConfig)
  -> synthesize_fragment_profile(profile_seed, radius, material)
  -> synthesize_fragment_budget(fragment_profile)
```

任何 runtime-only 随机源、系统时钟随机、进程启动随机都不得参与正式资源生成。

这里复用现有 Simulator 的派生、spacing、边界 reservation、预算与有界裁剪，不重新定义随机算法。生成顺序也是固定输入：同 seed/config 但不同已生成邻块或 trigger order 不能当作同一重放输入。生成给出初始 total/remaining；运行后的 remaining 只能读 §4.3 的已提交状态。

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

<a id="unified-resource-record"></a>
#### 正式业务事实的承载选择

使用现有正式 action → execution result → commit/checkpoint 链承载生成与消费，不另建专用 `ChunkResourceManifest` / `ResourceDelta` execution-record 类型、独立排序或第二种提交许可。已有 manifest/delta DTO 是同一结果中的资源内容与索引；它们自身的 `committed` 标签或 hash 不构成正式提交证明。

| 事实 | 生产者与同次结果 | 消费者与拒绝边界 |
| --- | --- | --- |
| 初始 starter 生成 | 外部初始工件制作方按固定上下文生成，随原正式初始 checkpoint 承诺完整地点/预算与 manifest；授权仍由原初始激活承担 | 正式启动先验证同一初始记录和实际数据；示例 genesis、sidecar 或独立本地 journal 不代签 |
| 后续 chunk 首次生成 | 已授权的原正式动作执行路径在确定的前状态上调用同一生成规则，产生本次地点、profile、双级预算、事件与结果 manifest；原 action/result 保留业务关联 | 原正式提交完成前为 pending/provisional；重复请求沿原幂等与原结果查询，不二次生成或增加预算 |
| 采集/消耗/回收/补种等变更 | 原相应业务执行者在同一候选中更新地点、fragment/chunk 预算及实际受影响库存，写明原 action/event、前后 manifest、顺序与精确 delta；无此后端能力时保持不可用 | 只接受当前世界的合法后继结果；检查预算非负、作用域、前后摘要及一次应用；请求失败或结果未知不产生推定资源效果 |

原条款要求的 fragment/location/chunk 身份、profile/total/remaining 摘要和实际变化必须能从该记录及其已声明内容中取得。当前通用 `ChainResourceDeltaEntry::ChunkResource` 只提供 chunk 聚合，不能单独代替 fragment 明细；需关联同次权威事件及前后状态，不能从相同数量猜受影响 frag。`latest_chain_resource_delta` 是最新结果索引，不是完整历史；中间记录缺失时不能凭最后一条补出历史。

构建候选时先固定前状态、动作与结果，再由原提交协议给出记录身份。不可把正在计算的 commit hash 回写到参与同一 hash 的 snapshot 而形成自引用；原可选 hash 字段及外部正式 proof/ref 按既有编码使用，消费者必须验证 ref 与该 state root 的关联，不能只看字符串非空。失败的候选不发布为新世界状态；恢复从已验证 checkpoint 和完整后继记录继续，不应用一半地点、一半余额。

本次选择要求的是自然资源业务接入既有正式执行链。[formal_package.rs](../../../crates/oasis7_chain_runtime/src/host/execution_bridge/formal_package.rs)当前显式拒绝 Simulator/bootstrap mutation；该拒绝继续有效。后续须在原 typed action/result 及权威状态消费者中实现资源逻辑并完成现有执行策略验证，不能把放开该判断、上传 Simulator journal 或生成 DTO 当作完成。具体实现补丁不在本次文档范围，正式记录类型与权限体系没有在此新增。

### 4.2 Provenance 状态
资源地点必须显式标记 provenance：

- `chain_committed`: 已进入 genesis/commit，可用于正式采矿、生产、交易、provider observation。
- `chain_pending`: 已提交生成或变更请求，但尚未 committed；UI 可以展示等待态，provider 不得把它当作可消耗资源。
- `provisional`: runtime 本地预览或测试 fixture；只能用于调试/视觉预览，不得进入正式玩法账本。
- `compat_anchor`: `runtime:x:y:z` 坐标锚点；不是正式资源地点。

### 4.3 资源变更权威

Trace refs: O7-REQ-2b3769ae-078a-4123-a0de-e20405b42c7c；承接[既有产品消费者](../../product/world-rules-core-gameplay/prd.md#committed-resource-continuity)，只沿原 R6 与对应 AC 范围；正式写端与同候选恢复仍须验证。

`remaining` budget 的变化必须来自链上 action / commit：
- 采矿、回收、补种、生产消耗和 material ledger 增量都必须产生可重放的 resource delta。
- runtime 可以先做 optimistic/pending 显示，但 committed snapshot 必须以链上 delta 为准。
- viewer 刷新、provider observation 和 testnet replay 不得通过重新运行 generation 还原 `remaining`，只能从 committed delta 或 checkpoint 恢复。

这条禁止的是把既有资源重新生成成未消费余额。初始 `ChunkGenerated` 的布局重建仍按 [CG6/CG10 的固定上下文校验](chunked-fragment-generation.prd.md#chunk-cg6-replay-contract)执行；到达该生成事件之后，必须继续按原顺序应用已经提交的补种/消费，不能停在初始预算上。若 checkpoint 已含该 chunk 及其消费结果，就从该切点后的事件继续，不能重复应用生成事件。后续 `FragmentsReplenished` 只应用已成 entries，不重跑补种算法。

例如初始 chunk 只有 fragment A、剩余 100 g；A 已提交消费 30 g 后，A 与 chunk 的 remaining 都是 70 g。后续补种新增 fragment B、预算 5 g 时，A 仍为 70 g、B 为 5 g、chunk 总剩余为 75 g；不能把新 frag 的资源加回 A。checkpoint 或完整历史恢复须分别重现这三项，只验证初始生成 100 g 不通过余额连续性验收。重复事件或错前状态不能再增加一次。该算例仅说明既有补种创建新 frag、双级 delta 顺序与恢复判据，不新增资源产率。

## 5. Runtime Live 接入设计
### 5.1 World Bootstrap

Trace refs: O7-REQ-212aa59b-6d97-43a7-8fd7-35d441489281；承接[既有产品消费者](../../product/agents-world-simulation/prd.md#formal-generated-world-entry)，只沿原 R3 与对应 AC 范围。

formal release 默认世界启动时：
1. 加载并验证 §3.1 的正式初始/当前 checkpoint、seed manifest 和固定生成上下文。普通 Viewer/claim 不能在缺少它们时自行构造正式 manifest。
2. 用有效 `WorldConfig.space` 的整数厘米尺寸计算几何中心 `(width_cm / 2, depth_cm / 2, height_cm / 2)`，每轴向下取整；通过现有 `chunk_coord_of` 得到唯一 starter chunk。三轴必须为正且坐标有效。中心在内部边界上时沿原半开块归属规则，不另作负向偏移。
3. 本次选择**固定中心规则**，不开放 manifest 中的另一个 starter 坐标覆盖，也不以 map 首项、`bootstrap_chunks` 顺序或客户端参数选择。`bootstrap_chunks` 仍可预生成其他块，但不改变正式 starter 身份。默认 100 km × 100 km × 10 km 空间按当前 20 km × 20 km × 10 km chunk 得到 `(2,2,0)`；本地 `(0,0,0)` 是另一已采用局部初始化，不等于几何中心。
4. 初始工件应预提交该 chunk；确有原正式按需生成能力时，缺少已提交 chunk 才沿该能力请求生成并等待原结果。能力未接入时显示当前无法准备 starter，不能显示虚假“已提交”或在客户端重试生成。验证 profile、total、chunk budget 及当前 remaining 都来自同一记录，不用本地预览覆盖。
5. 先核该 chunk 的地点/profile 与双级预算完整性；缺预算、来源不明或对账失败属于不可确认，不能静默丢掉坏条目后称整个 chunk 有效。完整有效后，从其中当前有正可采 remaining 的 `frag-` 地点按稳定 Location ID 排序取首个；已确认零余额的 frag 不进入候选。这只确定有资源来源的初始地点，不承诺全部首个工业配方投入、免费采集、路径必达或永久独占。预览与提交之间仍需重核资源和权限。
6. 无可用候选时保留明确 empty-world/onboarding 阻塞及 `starter_resource_pocket_missing`。可以把 chunk 中心用于无资源的只读定位并显示既有 `starter_chunk_resource_missing` 原因，但不得因此成功注册 starter、发资源或把中心标为正式可采地点。不得换 seed、换 chunk、填满预算或无限重新生成。

§9 明确替代旧“默认中心或 manifest 指定”的二选一，以及“中心 fallback 仍可注册”的歧义。保留 AC2 的明确 empty-world 路径和 AC3 的成功 claim 必须有真实资源上下文；失败不被降为成功的兼容锚点。

### 5.2 Location 语义
- `frag-*` / seed-derived / chain-committed Location：正式资源地点，可被 viewer、provider、采矿/生产链路引用。
- `runtime:x:y:z`：兼容坐标锚点，只能用于过渡、调试和无资源动作，不应作为正式可采地点。
- Viewer 需要把两者视觉区分：正式资源地点显示资源摘要；坐标锚点显示 fallback/compat badge。

正式新 starter 直接以选定 frag 的 ID 作为逻辑 `location_id`，不另建 logical base 代理它的资源。初始几何位置复用[既有 standoff/边界规则](agent-frag-initial-spawn-position.prd.md#fsp-standoff-contract)：优先 frag 上方约 50 m，按原确定性边界降级，逻辑地点始终是同一 frag。坐标等于或偏离 frag 中心都不能单独证明地点身份、所有权或采集资格。nearby resource 摘要至少能说明这个实际来源，并按原可见范围列出其他获准来源；不把全部 starter chunk 当作该 Agent 的库存。

### 5.3 Resource Ledger
正式资源的核对顺序：
1. chain-committed resource delta / checkpoint
2. `Location.fragment_budget.remaining_by_element_g` with matching chain provenance
3. `WorldModel.chunk_resource_budgets[coord].remaining_by_element_g` with matching chain provenance
4. 已采矿/加工后的 runtime material ledger

固定 bootstrap material 只能作为 migration / test fixture / grant，不得被描述为世界自然资源。

这四项不是四个可互相顶替的权威来源。前 3 项必须在同一已验证 checkpoint/record 切点上相互一致；缺 fragment 预算或 chunk 对账失败时返回未知/不可用，不跳到总表猜该 frag 的可采量。第 4 项是已经取得的库存，按原消费账本使用，不能补作地下 remaining。恢复不得将原料库存、grant 和自然资源各算一遍来掩盖缺账。

<a id="unified-claim-flow"></a>
## 6. Claim / Onboarding Flow

Trace refs: O7-REQ-7853375e-94b6-4c6c-8d6b-c486a4277dd4；承接[既有产品消费者](../../product/agents-world-simulation/prd.md#first-claim-generated-context)，只沿原 R4 与对应 AC 范围。

首次 Agent 引导按以下顺序承接原注册/绑定操作，不把这六步包装成一个已经存在的原子 API：
1. 在当前合法玩家/owner、world、branch/reorg 与原请求上下文中读取 seed manifest，验证 §3.1 的正式来源。
2. 核中心 starter chunk 是否已提交。准备请求只沿实际存在且获准的正式能力发出；未完成时返回等待，尚无该能力时返回明确不可用。等待不消费资源、不注册占位 Agent 或伪造一次 claim 成功。
3. 新建 starter 按 §5 的候选和逻辑地点规则选取；原操作已有结果或已有可复用 Agent 时按原 claim/身份合同恢复，核其当前 location 和资源上下文，不能为了符合新开局规则自动移动、重建或重发它。
4. 原正式注册/绑定执行方在自己的当前前状态重新验证归属、生成上下文、候选与预算，再提交该操作。只有原权威结果才能显示已取得 Agent；请求接受不等于完成。当前 owner 或世界隔离已变时停止旧上下文，原结果未知先查询原请求。
5. 后续 OC/LLM starter grant 沿其**既有独立操作**、资格、额度和幂等规则处理。当前 [starter admission](../../../crates/oasis7_runtime/src/service_admission/starter.rs)核当前 owner/world/genesis/branch，[apply_game_starter_claim](../../../crates/oasis7_runtime/src/world/game_starter.rs)单独处理实际领取；注册不自动授权或执行领取。grant 失败保留已完成注册及其真实结果，不自动撤销 Agent、重复领取或补自然资源。
6. 同一获准 snapshot 中显示实际 frag、当前资源、原操作阶段与下一步。推荐采集时沿[既有首 frag 理由](agent-frag-initial-spawn-position.prd.md#fsp-starter-reason-contract)给材质预期、当前可达性和首工业目标关联；没有资料就标明缺口，不仅因位置近而承诺可完成首配方。

如果任一步失败，返回明确错误：
- `world_seed_missing`
- `starter_chunk_generation_failed`
- `starter_chunk_not_committed`
- `starter_resource_provenance_missing`
- `starter_resource_pocket_missing`
- `starter_spawn_location_missing`

这些是原目标的原因分类，既有通道是否有对应 typed code/可读映射仍须接线；不冒称当前所有后端已返回这些拼写。`starter_chunk_not_committed` 在已知请求待决时表示等待；请求是否发出不明则保留未知，不能暗换 nonce 再提交。无资源/生成失败的阻塞不把合法观察、帮助或返回入口一并隐藏。

<a id="unified-observation"></a>
## 7. Viewer / Provider Observation

Trace refs: O7-REQ-fd8b7e31-afa8-4361-81f0-849c07099dfd；承接[既有产品消费者](../../product/agents-world-simulation/prd.md#shared-snapshot-resource-observation)，只沿原 R5 与对应 AC 范围。

Snapshot 新增或确保暴露：
- `world_seed` 或 `world_seed_ref`
- `chunk_generation_schema_version`
- per-location `position_source`: `seed_fragment` / `runtime_coord_anchor` / `fallback`
- per-location `resource_provenance`: `chain_committed` / `chain_pending` / `provisional` / `compat_anchor`
- per-location `resource_commit_ref`
- nearby resource summary: dominant material, remaining key elements, chunk coord

Provider observation 使用同一 snapshot 派生，禁止单独查本地缓存生成不同资源摘要。

双方比较的切点包括 world/chain/genesis、正式 checkpoint/commit ref、manifest hash、seed/version、Agent/owner 与获准可见范围；输出可按各自用途裁剪，但相同可披露地点/资源的身份和数量不能矛盾。缺权限不复制另一消费者的私有字段，缺权威资源投影不以 Agent 的 self-resources 冒充附近可采预算。provisional 可在 Viewer 的明确预览中出现；Provider 不能把它放入正式可消耗集合，只能返回该资源不可用的状态。读取失败、hash 冲突或旧版本时保留真实未知，不调用生成器“修复”观察值。

## 8. Migration Plan
### M1 文档与 contract
- 维护本专题 PRD/design 与原任务证据，不另建并行状态账本。
- 在 runtime live/formal release 文档中引用 seed-authoritative contract。
- 明确 chain-authoritative resource contract。

### M2 Snapshot contract
- 为 runtime live snapshot 增加 seed/version/resource provenance 字段。
- Viewer 显示正式资源地点与 compat anchor 区分。

### M3 Runtime bootstrap
- formal release 默认世界按已承诺 manifest 与中心 chunk bootstrap。
- 首次新 starter 绑定真实 frag 地点；既有 Agent 恢复与独立 grant 边界保留。

### M4 Chain resource commitment
- genesis 预提交 starter chunk resource manifest。
- chunk 按需生成接入既有正式 action/result/commit，`generate_chunk` 表示业务意图，不宣称当前已存在同名可用 API。
- 采矿/生产/补种提交 resource delta，committed snapshot 从 chain delta 恢复 remaining budget。

### M5 Testnet evidence
- public/local testnet readiness 增加 world seed 与 chunk generation schema 验证。
- provider bridge smoke 增加 observation seed/resource provenance 校验。
- 增加 refresh/replay 后 remaining budget 不被本地重算复原的验证。

<a id="unified-decisions"></a>
## 9. 四项正式设计选择与原问题处置

以下在原 R1–R6 范围作出选择；不以“以后实施再定”保留二选一，也不从已有局部实现倒推整个正式目标已采用或已通过。

| 原问题 | 选定规则、理由及原文处置 | 直接承接 |
| --- | --- | --- |
| `world_seed` 在 public_testnet genesis 的字段与位置 | 使用现有 `snapshot.chain_resource_manifest.world_seed`，由原正式初始 checkpoint/state root 承诺；外部初始工件制作方提供明确 seed/固定上下文，原授权方接收。避免新增独立 genesis seed 与运行期派生值互相覆盖；公开 example JSON 不是正式来源。 | [固定输入与绑定](#unified-seed-binding)，R1/R2/AC1/AC5 |
| starter 中心默认或 manifest 显式指定 | 固定为有效空间几何中心所在 chunk，不新增 manifest override。原“中心或指定”改成一个可重现选择；没有资源则明确阻塞，不移动中心、刷 seed 或改预算以保证成功。其他 bootstrap chunks 继续可用但不决定 starter。 | [正式启动](#51-world-bootstrap)，R3/R4/AC2/AC6 |
| 首 Agent 的逻辑地点是 frag 或 nearby base | 新 starter 直接绑定选定 frag，保留原 standoff 几何规则；不创建代理资源的 base。原中心 fallback 仅供只读定位，不能支撑成功 claim；既有 Agent 不自动迁移。这样成功 `location_id` 有真实可解释来源，同时保留 empty-world 失败路径。 | [Location](#52-location-语义)、[认领](#unified-claim-flow)，R3/R4/AC2/AC3 |
| 现有 action/event 或专用 execution record | 使用原正式 action/result/commit；现有 manifest/delta 是同次资源内容与引用。不新增专用执行记录、排序或权限；初始几何的固定上下文重建与后续已提交 delta 分层，完整后继历史恢复剩余量。 | [资源承载](#unified-resource-record)，R2/R5/R6/AC4/AC6–AC8 |
| 现有 action/event 或专用 execution record | 使用原正式 action/result/commit；现有 manifest/delta 是同次资源内容与引用。不新增专用执行记录、排序或权限；初始几何的固定上下文重建与后续已提交 delta 分层，完整后继历史恢复剩余量。 | [余额恢复](#43-资源变更权威)，R2/R5/R6/AC4/AC6–AC8 |

这次实质消除了旧 §5/§9 的选择歧义，并收窄了“fallback 仍可注册”的读法；保留要求的正式自然资源来源、成功 claim 上下文和明确失败，未减少 R1–R6 的消费者、AC 或链上义务。未提供的真实初始工件、部署授权、自然资源写端和运行证据仍是交付缺口。


<a id="unified-profile-register"></a>
## 10. 已采用 profile 与正式剩余范围

<a id="simulator-facts-design"></a>
### 10.1 Simulator 生成与预算来源

Trace refs: O7-REQ-9a16e402-a0e7-45eb-8122-4741155843c2；唯一工程声明与范围见下方对应 PRD 身份。

[既有生成事实身份](unified-world-seed-fragment-runtime.prd.md#uws-simulator-facts)只接收当前 Simulator 子范围。[init.rs](../../../crates/oasis7_simulator/src/init.rs)、[chunking.rs](../../../crates/oasis7_simulator/src/chunking.rs)、[from_simulator_state](../../../crates/oasis7_simulator/src/chain_resource.rs)及 [host snapshot](../../../crates/oasis7/src/simulator/kernel_host.rs)分别定位初始化、chunk 派生、预算投影与归档。旧 `chain_resource_schema.rs` 现为 shared DTO re-export，不再作为 `from_simulator_model` 的实现位置。fragment/chunk total/remaining 沿[chunked 原专业需求](chunked-fragment-generation.prd.md)，不从 snapshot 字段重新发明账本。当前 [from_runtime_state](../../../crates/oasis7_runtime/src/chain_resource.rs) 的 `runtime_agent_chunk_v1` 仍从 world/chain/config hash 派生 seed，构造 runtime-starter refs 与预算；共用 DTO 不证明统一来源，也不能因本设计选定目标就宣称这条路径已替换。

产品消费者仍适用 R1/R2/R5/R6 及 AC1/AC4/AC5/AC7；工程投影来源不是端到端验收。CG6/CG10 与 CG7 的原冲突现由[分层恢复裁决](chunked-fragment-generation.design.md#chunk-replay-conflict-register)明确：初始 `ChunkGenerated` 按固定上下文重建校验，后续补种/消费按已成 entries/delta 应用。§4.3 的 remaining 连续性依然适用，不能用初始生成校验代签余额恢复。

<a id="local-spawn-adoption-design"></a>
### 10.2 本地 formal-release 默认初始化

[已有 #547](https://github.com/nil-studio/oasis7/pull/547)接收本地 Simulator seed/fragment spawn，chain-authoritative resource 保留后续范围。[gameplay_actions.rs](../../../crates/oasis7/src/runtime_host/gameplay_actions.rs)的默认 seed、bootstrap `(0,0,0)` 与 build_world_model 接线，以及[support.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/support.rs)的本地 starter Agent，属于已采用子范围。frag 优先与上方 standoff 的唯一身份沿[原出生要求](unified-world-seed-fragment-runtime.prd.md#uws-local-spawn-adoption)，不重复声明。

support 仍有固定 material/electricity/Data 与 factory bootstrap；grant 不变为自然资源。当前取模型 Agent 坐标不证明正式 claim 的 location_id、nearby resource、commit provenance 与 fail-closed 已绑定。坐标零不等于“世界中心”的产品定义，不能把本地常量升级成所有正式 profile 的 starter 默认选择。

<a id="artifact-separation-design"></a>
### 10.3 Generated sidecar 与 governed runtime world

Trace refs: O7-REQ-41fc7ce2-9979-43b2-be4f-1c7f8c80e58b；唯一工程声明与范围见下方对应 PRD 身份。

[分离身份](unified-world-seed-fragment-runtime.prd.md#uws-artifact-separation)由[#1797 采用记录](https://github.com/nil-studio/oasis7/issues/1797#issuecomment-4867727910)及[#1801](https://github.com/nil-studio/oasis7/pull/1801)支持。[build/validate 脚本](../../../scripts/p2p-build-governed-bootstrap-world.sh)保留 audited governed runtime world，将生成场景写入 generated-scenario-world；[generator](../../../crates/oasis7_ops_tools/src/bin/oasis7_generate_world_artifact.rs)保存 scenario/seed/config/public manifest provenance。artifact 生成成功或 sidecar 存在不代替 governed world 激活，也不证明 public genesis 已选择 canonical seed 字段、starter 或自然资源承载。

[world_initialization.rs](../../../crates/oasis7_chain_runtime/src/host/world_initialization.rs)接收 externally prepared world/snapshot/journal/issuer/approval，并不创建 World 或制造身份；这是真实正式激活信任边界，不是业务生成方案的选择记录。

<a id="status-source-design"></a>
### 10.4 当前 resource status 的权威来源接收

Trace refs: O7-REQ-88cc07e6-ae98-4ecf-a173-39b235e39fe6；唯一工程声明与范围见下方对应 PRD 身份。

[status 来源身份](unified-world-seed-fragment-runtime.prd.md#uws-status-source)由[#2762](https://github.com/nil-studio/oasis7/issues/2762)与[#2766](https://github.com/nil-studio/oasis7/pull/2766)接收。[status_payload_world_resource.rs](../../../crates/oasis7_chain_runtime/src/host/status_payload_world_resource.rs)优先最新 execution record/CAS；其损坏、无效或高度不一致路径拒绝用旧 ready 材料掩盖，合法 genesis/recovery fallback 仍保留。这里接收的是 status 读取决定，不是新 resource execution record schema、正式生成动作或最终性协议选择。

原 R6/AC6/AC7 要求的生成、消费、提交、恢复 remaining 与跨消费者 provenance 仍需单独验证。latest manifest/delta 有字段、有 hash 或 status ready，都不能据此宣称自然资源 chain-authoritative 全部落地。

<a id="unified-decision-profile"></a>
### 10.5 已选方案与真实实施边界

§9 已回答四项设计问题。下表保留已采用局部范围并指出正式实现所需的真实输入，不创建另一个 profile 或把字段存在写成接线完成。

| 原问题 | 已采用子范围 | 已选设计下仍未核实的正式实施 |
| --- | --- | --- |
| public_testnet genesis 的 world_seed 位置 | Simulator seed、sidecar provenance、Runtime 派生 seed 和外部 checkpoint 承载均存在 | 取得并验证真实初始工件中所选 canonical seed/完整上下文；接入正式写端及所有消费者，禁止普通 Runtime 派生覆盖 |
| starter 中心还是 manifest 指定 | 本地 `(0,0,0)` 与 Simulator bootstrap_chunks 保留原范围 | 实现正式中心 chunk 计算、预提交/实际等待、当前可采候选与空世界阻塞；不把本地默认测试当正式结果 |
| first Agent frag 或 logical base | 本地 frag 优先与 standoff 已采用 | 正式新 starter 的逻辑 location、当前预算/来源重核、原请求恢复和独立 grant 路径；坐标断言不够证明 |
| existing action/event 或专用 record | manifest/delta、snapshot、Simulator event 派生及最新 CAS/status 已采用 | 真实自然资源业务经原 typed action/result/commit 发布、fragment 明细与双级预算原子一致、消费者同切点和完整历史恢复；当前 formal 禁止 Simulator mutation 不绕过 |

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
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r1-统一世界种子) | [seed 绑定](#unified-seed-binding) | AC1/AC5 同世界 seed、固定上下文与可验证来源；原适用条件保留 | [独立逐项计划](../../../testing-manual.md#uws-v-seed-reference)及 §11.3；核实际初始材料/恢复/跨入口正反例 | 同候选初始记录、固定配置与各消费者输出/差异 | 方案已选；真实材料、实现与验证未完成 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r2-复用现有-chunkfragmentresource-生成) | [seed 派生](#32-seed-派生) | total/remaining 同源，非零消费、缺预算及错 chunk 不以重算代当前量 | [独立逐项计划](../../../testing-manual.md#uws-v-resource-budget)、§11.3 及 chunked 恢复判据 | 生成输入、fragment/chunk 账本、每次 delta 与恢复结果 | 本地 fixture 不代签正式资源写端 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r2-复用现有-chunkfragmentresource-生成) | [预算核对](#53-resource-ledger) | total/remaining 同源，非零消费、缺预算及错 chunk 不以重算代当前量 | [独立逐项计划](../../../testing-manual.md#uws-v-resource-budget)、§11.3 及 chunked 恢复判据 | 生成输入、fragment/chunk 账本、每次 delta 与恢复结果 | 本地 fixture 不代签正式资源写端 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r3-runtime-live-消费世界生成事实) | [正式启动](#51-world-bootstrap) | AC2/AC3 中心 starter、真实 frag 或明确 empty-world；不假成功 | [UWS-V-CONSUMERS](../../../testing-manual.md#uws-v-consumers)及 §11.3 的中心/边界/缺资源/缺能力场景 | 实际 starter coord、Agent/location/context/commit、原因和等待来源 | 中心与地点选择已定；正式测试接线及真实入口未验证 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r4-新用户入口) | [首次认领](#unified-claim-flow) | AC2/AC3 当前归属重核、原结果恢复、注册与 grant 分离 | [UWS-V-CONSUMERS](../../../testing-manual.md#uws-v-consumers)及 §11.3 的身份变化/重复/失败组合 | 各原操作、实际 Agent/location、独立领取结果和副作用计数 | 本地坐标或 grant 单测不代签正式引导 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r5-viewer-与-provider-观察一致) | [对应条款](#unified-observation) | AC4 两消费者同 snapshot 同 IDs/摘要  原 AC 与其 profile 条件仍适用。 | [独立逐项计划](../../../testing-manual.md#uws-v-observation-parity)；按该计划固定输入、正反例、原合同 oracle 与输出，未执行 | 两输出、snapshot/Agent/权限引用与差异 | 待接真实消费者 harness；不伪造已存在测试或玩家结果 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r6-资源生成全部链上化) | [资源承载](#unified-resource-record) | AC6/AC7/AC8 正式记录、remaining 恢复和 provisional 拒绝；原适用条件保留 | [UWS-V-CONSUMERS](../../../testing-manual.md#uws-v-consumers)与 §11.3，核生成→提交→消费→刷新→replay；provisional 独立负例 | 真实 action/result/commit、完整 delta、前后 remaining 及消费者引用 | 承载已选；正式自然资源实现与执行结果仍未完成，status 不代签 |
| [对应原要求](unified-world-seed-fragment-runtime.prd.md#r6-资源生成全部链上化) | [余额恢复](#43-资源变更权威) | AC6/AC7/AC8 正式记录、remaining 恢复和 provisional 拒绝；原适用条件保留 | [UWS-V-CONSUMERS](../../../testing-manual.md#uws-v-consumers)与 §11.3，核生成→提交→消费→刷新→replay；provisional 独立负例 | 真实 action/result/commit、完整 delta、前后 remaining 及消费者引用 | 承载已选；正式自然资源实现与执行结果仍未完成，status 不代签 |

### 11.2 计划执行入口

[artifact 计划](../../../testing-manual.md#uws-v-artifact)、[status 计划](../../../testing-manual.md#uws-v-status)、[本地初始化计划](../../../testing-manual.md#uws-v-local-spawn)与[正式消费者计划](../../../testing-manual.md#uws-v-consumers)明确候选、命令/输入、断言、输出和阻塞；计划存在不等于执行完成。

<a id="unified-decision-validation"></a>
### 11.3 已选规则的成功、拒绝与恢复判据

以下是原 R1–R6/AC1–AC8 的具体输入与独立 oracle，不新增 requirement ID，也不假称已有可直接执行的正式 harness。实施方先固定同一 source/执行材料、世界身份、真实初始工件、授权与可用消费者；记录缺失前置，不能用 local 场景替换正式目标。

| 对应原验收 | 输入/操作 | 独立预期与证据 |
| --- | --- | --- |
| AC1/AC5 seed 与绑定 | 一个有效正式 checkpoint 及其 config/chunk runtime；分别更改 seed、offset、配置 hash、world/chain/genesis、算法版本或撤去固定材料；另用合法 seed=0 | 有效组合在 local/public testnet、runtime、Launcher、Viewer、Provider 引用同一实际承诺；各不匹配组合阻塞、不另派生 seed。seed=0 只按完整绑定判定。保留原材料和每个消费者实际引用。 |
| AC2/AC6 starter 选择 | 有效默认空间、非整块尺寸、中心恰在内部边界、任一零维；打乱 generated_chunks/map 顺序及其他 bootstrap chunks | 有效中心严格按 §5.1 计算，与集合遍历顺序无关；默认为 `(2,2,0)`，零维拒绝。核实际 chunk 的正式初始/后继记录，不只核坐标算式。 |
| AC2/AC3 地点与失败 | 中心块有两份合法 frag（按 ID 排序），再把首份设为已确认零余额；独立移除任一预算/provenance 或全部中心可采候选，其他块仍有资源 | 完整有效时成功新 starter 绑定中心块首个正余额 frag，逻辑 ID 与 standoff 坐标分开；缺预算/来源或无可采候选时阻塞，不能跳过坏证据、改到另一块、runtime anchor 或随机 seed；零新增 Agent/grant/资源。已接受生成请求与根本无生成能力分别显示。 |
| AC3 首次认领/恢复 | 注册前后分别变 owner、资源或 world/branch；注入受理后断连、重复原请求、已有 Agent，及注册成功而独立 grant 被拒绝 | 提交重核；未知先查原操作；既有结果不重复注册/领款或自动搬迁。grant 拒绝仍保留真实注册结果，当前 owner 校验不能由旧页面绕过。保存原 correlation、各结果及副作用计数。 |
| AC6/AC7 生成/消费恢复 | 生成初始 chunk/frag A→正式提交→A 非零消费→补种新增 frag B，分别从生成前和消费后合法 checkpoint 恢复；Viewer 刷新与 Provider 重读 | 初始布局按 CG6 校验；完整后继分别对账各 fragment 及其 chunk 汇总：本例 A=70、B=5、chunk=75，不将 B 的补种写回 A。各切点恢复得到同一原模型，消费后切点不再生成初始 100。记录各 action/event、前后 manifest、正式 proof 与读取输出。 |
| AC7/AC8 完整性与 provisional | 缺中间 delta、重复/乱序、错 fragment/chunk、改 budget hash；同一内容只留本地 preview 或 pending | 拒绝坏恢复候选，不发布部分预算；preview/pending 不能用于正式采集、生产、交易或 Provider 可消耗集合。仅有 DTO committed 字段或 CAS 文件不改变结论。 |
| AC4 观察一致 | 在同一世界/Agent/授权/snapshot 切点读 Viewer 与 Provider；再分别使某端只剩旧缓存、缺预算或权限收窄 | 相同可披露 IDs/seed/version/资源数量一致；旧/缺/无权不以 self-resources、坐标或生成算法补真值。输出差异按权限解释，不能扩大披露来凑一致。 |

中心坐标算术和文档路径可静态核查；真实正式提交、消费者、恢复与界面分别需要对应实施证据。上述方法不授权创建正式世界、启用资源动作、发 grant、改变部署或招募真人。

## 12. 风险与未完成边界

四项设计选择及旧 replay 措辞冲突已明确，原消费者、R1–R6、AC1–AC8、grant 权威与已采用局部范围均保留。尚未完成的是实际初始生成上下文的封装/验证、自然资源业务写端与原正式执行链接入、中心 starter/真实 location 的注册消费者、Viewer/Provider 同切点投影、完整恢复负例及目标环境验收。

固定中心会在部分合法生成输入下没有可采 pocket；本设计选择如实阻塞，不把“starter-safe”解释为强制增资源或无限重新生成。固定历史算法与完整上下文有维护成本；缺版本材料时应报告无法恢复，不制造一份新世界。真实初始工件、授权或历史材料尚未提供时记录具体未取得范围，不推断它们不存在。代码/schema 仍未修改，本次静态设计不能证明正式功能、全篇实现或发行就绪。
