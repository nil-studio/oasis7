# 工业外部性范围、缓解授权与独立处置系统设计

- 状态：`active`（目标系统合同；未证明当前 externality 功能可用）
- Owner role：`runtime_engineer`；领域协作：M4/domain、`gameplay_designer`、`agent_engineer`、`viewer_engineer`、`qa_engineer`
- 适用范围：明确声明 `industrial_externality` / `operational_mitigation` 的 factory/recipe operation；未声明且未被要求采用该 profile 的 legacy 路径不变。
- 审读基线：`e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`；实现跟踪：[可选工业外部性 #4862](https://github.com/nil-studio/oasis7/issues/4862)。

## 1. 问题、目标与非目标

普通配方完成、材料损耗、辐射源和物流结算各自证明自己的事实，不能证明一次工业运行已经满足局部外部性义务。目标是在生产的合法原子边界绑定 profile、scope、mitigation authority 与容量，独立记录生产结果和外部性处置，跨存量工作变更、重连和恢复保留原义务。

本设计确定对象身份、逻辑模型、授权检查、提交点、旧工作处置和失败恢复。污染/环境公式、扩散或衰减、阈值、清理经济、税费、区域 credit、治理/claim 权和危机 containment 不在本范围；没有已批准能力的 profile 不得通过写一个标签启用这些效果。

## 2. 上游约束与相关角色

产品 [SC-33](../../product/world-rules-core-gameplay/prd.md#sc-33)拥有完整适用条件及目标/当前边界；[外部性玩法合同](../../game/gameplay/gameplay-industrial-externality-contract.prd.md#2-玩家循环)拥有选择和机会成本；[M4 外部性 authority](../../world-simulator/m4/industrial-resource-flow-contract.prd.md#工业外部性与-operational_mitigation-authority（target-only）)拥有局部事实、容量、评估及处置类别；runtime 拥有排序、身份、事务和恢复。工业工作图提供原 root/revision/work node，本文不另建工作图。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [可选外部性 profile 与 legacy 边界](../../product/world-rules-core-gameplay/prd.md#root-sc33-extscope)；Trace refs: O7-REQ-e6f4ffa7-0b1b-4d0e-b7a3-8ca38c8d0244 | 仅显式 profile 启用；scope/authority 缺失、过期或不可核验时 unknown/blocked；不取得区域或治理权。 | [Profile与授权模型](#externality-profile) | M4 profile、当前授权；[配方生命周期](industrial-work-graph-and-recipe-lifecycle.design.md#recipe-lifecycle) | 不推断污染数值、不默认启用 legacy、不扩大局部 mitigation。 |
| [外部性处置与其他事实隔离](../../product/world-rules-core-gameplay/prd.md#root-sc33-extdispose)；Trace refs: O7-REQ-bd83785c-d414-4bd7-b291-41a4d7272aa3 | preview 只读、submit fresh；生产/处置独立且单次；WIP/transit/buffer/terminal 保留身份义务，不改 quality/custody/byproduct/maintenance/settlement/W。 | [联合结果及处置协议](#externality-commit) | [工作图与 ChildRefV1](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-identity)、[工作处置](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-cutover)；资源/物流/终端 authority | 不用普通工业 receipt 代签外部性；不宣称长时、区域影响或完整 current evidence。 |

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线与证据 | 目标及差距 |
| --- | --- | --- |
| Profile | [RecipeProfileV1/FactoryProfileV1](../../../crates/oasis7_wasm_abi/src/economy.rs)承载既有配方/工厂元数据；未提供本文的 scope、mitigation authority 与处置模型。 | 增加显式、有版本和已声明语义的可选 facet；不从 tags 补出授权。 |
| 生产与物流 | [工业事件准备](../../../crates/oasis7_runtime/src/world/event_processing/publication.rs)、[配方生命周期](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs)、[物流](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs)有真实各域写入。 | 外部性消费者必须独立存在，并与该运行的生产提交关联；现有 writer 不能替代它。 |
| 状态及读面 | [持久化](../../../crates/oasis7_runtime/src/world/persistence.rs)提供既有存取；[专业 current/target 边界](../../game/gameplay/gameplay-industrial-externality-contract.prd.md#4-currenttarget-evidence-cutline)保留目标状态。 | 接入 profile/authority/capacity/impact/obligation/receipt 的共同恢复和读模型。 |

当前结论限仓库默认/内置路径的静态审读。没有运行本专题生产、故障或跨入口验收；文件和普通工业实现存在不代表 externality 已实现。

## 4. 边界与结构

M4 产生有出处的 profile、局部 impact/mitigation 计算及支持的动作集合；调用者提交选择，runtime 重验当前状态并产生权威事件。权限与资源检查完成前，M4 返回值仍是待校验领域结果，不能自己扣容量、写生产余额或发区域奖励。遵循[共同权威规则](../design.md#runtime-gameplay-authority-designs)，评估绑定相同 canonical 输入、确切 module/profile 版本及有权来源的证据摘要；外部测量必须先通过合法来源写成可重放事实，不能在 sink guard 中按节点即时 HTTP、模型响应或客户端观测计算不同结果。

工作图拥有 root/revision/child 和存量节点；材料域拥有 quantity/quality/custody；生产域拥有 cycle/output；物流及终端域拥有在途、destination/recipient/admission/settlement。外部性只拥有局部义务、缓解容量及其处置引用。一个联合事务可以写多个独立领域事实，但不合并各领域的完成语义。

<a id="externality-profile"></a>
### 4.1 适用性、身份与持久模型

以下 profile、binding、mitigation authority、capacity pool、obligation 和 disposition 的身份均为 `(world_id, module_instance_id, object_kind, local_id)`；`module_instance_id` 属于该对象的权威 owner，引用同时绑定该对象的确切 revision，不能把引用方实例或 caller 提供的命名空间当作 owner。表中的短 ID 是这个全限定身份的局部字段；原工作身份继续消费工作图的全限定 `ChildRefV1`。profile/config binding 只能从当前权威 registry 解析，授权固定其真实 capacity pool 引用；跨实例共享容量仍写原 pool owner 的同一账本，不能按调用者实例复制余额、义务或去重索引。跨实例交接必须由现有受支持处置接口明确绑定源/目标全限定身份、两侧当前权限及原 receipt，在同一条件提交中处理源端未结容量与目标承接，保留原义务 lineage；任一侧不支持该原子接口就拒绝，不能通过重命名、切实例或跨 world 移用引用完成交接。

| 目标模型 | 必需字段与语义 |
| --- | --- |
| `ExternalityProfileV1` | `profile_id, revision, content_digest`；来源 module/release 与 authority；factory/recipe/operation scope；`obligation_kind`（明确无义务或已支持的局部义务）；mandatory mitigation 条件、容量单位/计算规则、适用时间、动作集合、合法提交/完成边界、失效/存量处置规则。 |
| `ExternalityBindingV1` | 精确 factory/recipe version、profile revision 集合、`ChildRefV1` 或尚未接受时的 candidate/config 身份、相关 authority revisions；显式 `not_declared / declared / required_but_unavailable`。没有 root 的首次只读预览不发 root。 |
| `MitigationAuthorityV1` | `authority_id, revision`、合法来源、主体、局部作用对象/范围、允许动作/资源、有效区间、撤销引用及可消费 capacity pool；治理或 crisis authority 不能按名称转换为该授权。 |
| `MitigationCapacityV1` | `pool_id, revision, resource_unit, total_available`、每个已接受 operation 的占用及实际消费/释放 receipt、适用 profile/authority 引用；同一容量不能同时给多个操作全额使用。 |
| `ExternalityObligationV1` | `obligation_id`、原 root/revision/child/effect role、原 profile/authority、实际受影响范围、已占用/消费容量、未履行条件、支持的后续处置、当前状态和 revision。profile 更新不覆盖原义务。 |
| `ExternalityDispositionV1` | `disposition_id`、义务/状态转换身份、原 profile/authority、选择及实际处理量/成本、源与后继 work node、production receipt 引用（适用时）、保留义务、权威提交与前后状态。不可变 receipt 不能从 hold 改写成处理成功。 |

`not_declared` 必须来自当前权威 profile/version 的明确读取，不能由网络缺项或旧客户端不识别字段推断。已声明“无外部性”仍保留 profile 和评估来源，以 `not_required` 表达该步无缓解义务，不分配 mitigation reservation 或虚构处理成本；它与没有 profile 不同。factory 和 recipe 都有 mandatory 条件时必须全部满足；版本冲突或相互不兼容就 blocked，不默认挑较宽松者覆盖另一方。

这些字段不是仅供展示的标签；它们必须进入[当前授权和预览检查](#externality-authorization)，缺失/失效就禁止该 operation 的不可逆 sink，且不能转化为区域或治理权限。

处置键基于原工作图提供的全限定工作身份及实际效果角色，即 `ChildRefV1 + effect_role + obligation_transition`；profile revision 是被绑定的证据，不是清空去重的维度。替换 profile、重复提交或另起请求不能重做已完成的同一步。合法后继 cycle/branch 通过工作图自己的新 child identity 区分，不把整个 root 限为只能生产一次。

## 5. 关键运行流程

<a id="externality-authorization"></a>
### 5.1 授权、预览与重新校验

1. 在同一权威快照读取 factory/recipe 的适用版本、profile、scope、mitigation authority、可用/占用容量及已有 output/terminal obligations。legacy 明确未声明且未被要求采用 profile 时走原路；已声明/必需但缺失、过期、冲突或不可核验时返回 `unknown/blocked`。
2. 评估只展示当前 profile 支持的 `run_with_existing_mitigation`、`run_reduced_or_defer`、`hold_for_declared_treatment_or_legal_disposition` 及其他已批准局部动作。不得从“已声明 treatment”推断当前主体、容量或目的地已经允许执行。没有安全动作时只返回阻塞和复查条件。
3. 预览绑定选择摘要、profile/authority/capacity/config revisions 与观察快照，说明吞吐/交付影响、缓解占用、时间/空间/电力成本、风险、保留价值和复查点。preview 不创建 root、hold、影响、生产、清理或区域 credit。
4. 提交从当前权威状态重验这些相关输入以及动作参数、owner/scope、原工作义务和容量。金额/容量/范围/语义漂移、权限失效或终端不适用时拒绝或要求明确重评；不能自动改为减产、换 profile、清理、退款或改道。无关世界变化可以保留有效预览，但不免除当前检查。

授权区间为权威时间 `[effective_from, expires_at)`。首个不可逆 sink 前和 profile 声明的后续合法 sink 边界均检查；先运行后补授权不成立。签名只证明来源，局部适用范围与容量必须另外满足。

<a id="externality-commit"></a>
### 5.2 接受、容量占用与联合结果

原工业提交接受新工作时，按[工作图](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-model)颁发 root/child 并在同一条件提交中写入外部性义务及必要 mitigation reservation。仅请求入队或只读评估不能占用容量；有界 pending/hold 必须由 profile 支持、可见、可恢复且具有到期/释放规则，不等于生产或处置完成。

运行时，输入消费、生产阶段更新与适用缓解消费共享领域的合法原子边界。生产完成必须在同一权威事务里写一次 production result 和一次独立、相互引用的 externality disposition，以及相应容量变化。任何一项准备或检查失败，整组完成效果不发布；不允许先把产物记为完成，再异步补一个可能缺失的缓解 receipt。

外部性 disposition 必须表达真实结果：`contained` 需要完整有效的缓解证据；`held_pending_disposition` 只证明有代价的受支持持有及保留义务，不表示 treatment/cleanup 完成。只有 profile 明确允许该持有结果且终端义务仍成立，生产结果才可以与其共同提交。profile 要求先完成 treatment 才能出产时，不能用 held 状态绕过该闸门。

已经原子记录的物理投入或阶段事实保持真实。授权在 WIP 期间到期/撤销时，停止新的未授权 sink；只按原 profile 和当前合法处置权限继续已经承诺的排空/持有/处理，不假装过去生产未发生。没有安全完成路径时保持 WIP 或其他合法待处置节点，不发布伪造完成结果。实际效果的恢复与重放依据已提交记录，不重新求一次有利的 M4 结果。

延期选择本身无生产，不制造虚假的零产量 production receipt。真正的 hold/reduce/treatment 是各自受支持的动作：有世界占用或损失时要产生对应 receipt；纯决定保留无效果语义。后续 treatment 或释放使用新 child transition 引用原义务和处置，不能覆盖旧 receipt，也不能再次消费同一阶段。

本结果协议同时消费[存量工作处置](#externality-existing-work)的原 root/revision 与义务，[事务恢复](#externality-recovery)负责单次重放，[授权读面](#externality-read)保持生产、外部性及终端各自语义；任何一层缺失都不能把联合结果声明为完成。

<a id="externality-existing-work"></a>
### 5.3 Profile、授权变化与存量工作

变更先通过[工业 cutover](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-cutover)形成有原工作节点版本和唯一选择的处置计划。外部性 facet 变化不能静默解除原 obligation；新 profile 只能承接显式、当前合法的后继动作。改变产出因果时创建 parent-linked child root/new candidate，稳定窗口从 `W=0` 开始；非因果 revision 沿原 root 保留各自 receipt 归属。

| 存量工作 | 必须保留 | 允许的有界处置及禁止事项 |
| --- | --- | --- |
| 已接受未开始/容量 hold | 原 root/revision/child、reservation、profile/authority、金额/容量和到期 | 按原合同继续、释放或拒绝一次；不得因为换 profile 刷新优先级或释放两次。 |
| WIP | 已消费投入、实际进度/损失、缓解占用、待满足义务 | 当前合法且 profile 支持才可完成、暂停、treatment、返工或 salvage；缺路径保留待处置，不能免费恢复投入。 |
| In-transit | 原 edge/目的账本、batch、数量、损耗、recipient 和缓解义务 | 显式 hold、return、handoff 或拒绝；新的目的地需领域重新校验，不能因外部性变化瞬移/重贴 lineage。 |
| Buffer-held | 已入库 batch、owner/容量、质量/保管和 provenance | 继续持有或显式受支持处理/转交；新候选消费须有兼容及交接证明，不自动解除原义务。 |
| Terminal-pending | quantity、recipient、destination、admission、capacity obligation、provenance、原 production/外部性 receipt | 保留到匹配 settlement 或一次受支持处置；不得提前减少需求、发奖励、释放容量或改绑收货方。 |

处置选择至多一次不意味着 hold 后永远不能完成：选择记录不可改写，后续每个合法履行/释放有独立 transition 和 receipt，明确引用同一选择。与正常完成竞争时，工作节点 revision 的条件提交只能接受一个有效分支，另一分支读原结果或冲突；不能同时完成与退款。改变 profile/authority 后实际义务不明时保留状态并阻断该工作，不能自动推出外部性“已消失”。

## 6. 接口与数据合同

| 接口 | producer → consumer | 身份 / 输入输出 | 顺序、失败与兼容 |
| --- | --- | --- | --- |
| 解析 profile binding | 配方/工厂 registry → runtime/M4 | 精确版本到 `ExternalityBindingV1`，包括明确 legacy、declared 与 required-unavailable | 同快照解析；未知必需 facet 拒绝，不降为 legacy。 |
| `preview_externality` | API/Viewer/Agent → 权威只读评估 | candidate/config 或原 ChildRef、选择参数、profile/authority；返回预览摘要、成本/范围/动作和 blocker | 不写世界；超时只表示查询未知；刷新不创建工作。 |
| 接受/运行检查 | 原工业 action → runtime/M4 | 原请求摘要、ChildRef、相关 revisions、义务及容量需求 | 与工业接受和 sink 的条件提交整合；缺项/过期/范围冲突/容量不足拒绝。 |
| `commit_externality_disposition` | M4 领域结果 + runtime → 工业/外部性状态及 journal | 原义务、唯一 transition、实际数量/成本、production/工作处置引用与后继义务 | 实际 producer 须受检查，客户端不能提交一个“成功”就直接落账；生产完成联合提交。 |
| `read_externality_operation` | 已提交状态/receipt → 授权读取者 | 原 root/revision/child、profile/authority、生产/外部性/终端各自结果和新鲜度 | 按当前读取权限重读；不补发执行；缺原件保持未知。 |

错误类别至少为 `profile_required_unavailable`、`authority_unknown`、`authority_expired_or_revoked`、`scope_conflict`、`mitigation_capacity_unavailable`、`preview_stale`、`disposition_unsupported`、`obligation_conflict`、`identity_conflict`、`version_unsupported`、`outcome_unknown`。具体 resource unit、容量算法和支持的处置类型必须来自已批准 profile；未知规则明确拒绝，不能自动转环境模型。profile/证据/处置集合有版本化大小上限，超过上限整项拒绝，不能遗漏部分 obligation 后继续运行。

<a id="externality-read"></a>
Viewer、pure API 与 Agent 读取同一 root/revision 和来源快照，分别表达 profile 适用性、scope、缓解容量可用/占用/消费、生产阶段、外部性状态、终端状态、primary blocker、实际成本/损失、可执行动作、下一步及复查点。已生产不等于已缓解，已缓解不等于已交付；陈旧/缺失结果不显示为安全或零影响。数据脱敏由当前读取权限控制，保留历史关联不等于授予读取他人材料或资格证据的权限。

## 7. 状态、事务与持久化

义务状态为 `accepted_unstarted -> active -> fulfilled`，或进入 `held/pending_disposition`；拒绝且未接受的请求不产生 obligation。每个 hold/处理/释放有其 transition revision，`pending_disposition -> fulfilled/disposed` 需要受支持的权威结果；取消未消费占用与已经消费的实际损失分别记账。`unknown` 是缺乏有效事实的观察/准入结果，不能当作已经完成或清理状态。

<a id="externality-recovery"></a>
### 7.1 事务、重放和恢复边界

接受、生产完成和后续处置可以是不同事务；每个事务共同验证原工作节点 revision、profile/authority/capacity、义务状态及领域资源，并原子写入自己的工作状态、容量变化、去重结果和 receipt。生产完成事务还须共同提交独立 production 与 externality result。事务准备不改变已发布状态，journal 提交失败不保留半组结果。顺序读取后一项失败时不允许只撤销一部分经济效果。

相同 semantic effect key 同内容返回原记录；同键不同数量、profile 或目的地冲突。缓解占用分配共享一个当前容量账本；多个 operation 竞争时按权威提交序及原资源分配规则处理，不能各自用同一预览余额透支，也不在此新增公平或优先政策。

snapshot+journal 同时保留 profile binding、全部被引用 profile 版本、authority 撤销/时界、工作图、缓解容量、obligation、处置选择、已提交 transition 和各域 receipts。恢复先重建原事实，再按当前权限允许读取/继续；缺少某个旧版本或原件时隔离关联工作，保持 unknown/blocked，不用当前 profile 补旧义务。响应丢失只查询原请求/效果，replay 不再次调用有副作用的 M4 producer、扣容量、产出或发奖。

生产、quality/custody、byproduct、maintenance、delivery、需求和稳定 `W` 各由原 authority 更新；外部性投影或恢复没有这些写权。去重和旧义务至少保留至所有相关工作、重放及争议窗口关闭；按[存储治理](runtime-storage-footprint-governance.prd.md)归档原件与索引，不能因 profile 到期先删除。

## 8. 部署、安全与运行约束

可选 facet 通过明确支持的 registry、WASM/API 版本及 runtime 能力协商启用，不以客户端猜测启用。模块被允许提出局部缓解结果也不代表具有区域/治理权；shared environment、radiation、crisis、quality 等写入类型在 externality 提交路径不可达。

缺 profile/证据、落后快照、容量服务失效、超时或不支持版本只阻断有关 operation。无 profile legacy 与不受影响工业操作继续；未知状态不能触发免费 cleanup 或全世界停机。真实跨节点和重启证据须由原持久化/共识路径取得，单进程 fixture 不证明部署可用。

## 9. 质量与容量

| 场景 | 环境 / 输入或刺激 | 预期响应 / 判定指标 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 可选性与授权 | 同候选无 profile、明确无义务、known-contained、容量满、unknown/expired 组合 | legacy 差异为零；无效声明路径不可产生 sink；比较和成本同义 | [EXT-VERIFY-001](#ext-verify-001) | 目标 required fixture，未运行。 |
| 联合提交与隔离 | 生产完成前故障、缓解容量竞争、重复 submit/retry/replay | 每个真实 step 生产/处置各一次；无半提交或容量透支；其他事实不变 | [EXT-VERIFY-002](#ext-verify-002) | 目标 required fixture，未运行。 |
| 存量与恢复 | 各类工作存在时 profile/authority 变化，响应丢失、snapshot 恢复 | 原身份/义务不丢失，完成与处置不双重生效 | [EXT-VERIFY-003](#ext-verify-003) | required 代表性恢复；长时/多阶段 full 另提供实际证据。 |
| 规模隔离 | 最大声明 profile/处置集合和超限输入 | 有界完成或明确无效果拒绝，不截断义务 | [EXT-VERIFY-002](#ext-verify-002) | 容量上限由实现版本声明；没有性能或环境模拟指标。 |

## 10. 兼容、迁移与回滚

旧快照的无 profile 状态保持原行为；不把旧 radiation/tag/contamination 迁移成 externality。启用的 profile/version 和 mandatory 支持能力必须可判定，新客户端缺字段不能获得“安全”结论，旧 runtime 不能忽略 mandatory facet 继续生产。

接入前须具备本次实际消费的工作身份、适用配方/工厂版本及准入断言、事务和存量处置接口，再连接 profile/authority、联合提交、恢复及消费者；不要求把工作图专题所有其他功能或全部六态实现当作统一前置，只有具体 profile 消费的状态能力需要就绪。已写入新 facet 的快照必须保留原 profile/义务/容量；关闭新入口不释放旧占用或抹去待处置 WIP。回退目标必须支持原新字段与待处置读取；不兼容时禁用有关新执行、保留可恢复状态，不能丢字段回到旧世界。已产生物理结果只通过有权限的新处置修正，不回滚成免费资源。

## 11. 验证设计与可追溯性

<a id="ext-verify-001"></a>
**EXT-VERIFY-001（范围、授权及只读）**：固定实际 candidate、profile/authority revisions 和世界初值，沿 SC-33 required 生成无 profile legacy、已声明无义务、known-contained 且容量充足、容量满、authority unknown/expired，以及 factory/recipe profile 冲突样例。只读 preview 前后权威摘要相同；只有 profile 支持的动作可选，逐个核对成本/吞吐/复查及缺权限禁止 sink，Viewer/API/Agent 分别读取同一来源。

<a id="ext-verify-002"></a>
**EXT-VERIFY-002（事务与隔离）**：以[配方生命周期](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs)和[工业 prepared publication](../../../crates/oasis7_runtime/src/world/event_processing/publication.rs)的实际 writer 为测试边界，扩展专用 runtime 回归。注入准备/提交失败、容量竞争、重复同 step、同键异内容、profile 换版再投与超限输入；两个不同 module instance 持有同 local profile/pool ID，由第三实例消费，必须只写授权绑定的原 pool，换 profile/实例不能复制余额或刷新原义务去重，缺两侧权限或联合提交能力的交接拒绝。断言 production/disposition 相互引用且各至多一次，quality/custody/byproduct/maintenance/terminal/W 差分仅来自原域合法事件。没有 production 的 defer 不生成假生产 receipt。这些为实现验证方法，不代表现有测试已覆盖。

<a id="ext-verify-003"></a>
**EXT-VERIFY-003（存量、幂等与恢复）**：在[工作图处置合同](industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)的代表性未开始/WIP/transit/buffer/terminal 节点分别撤销授权、更新 profile、竞争完成/处置并丢失响应；扩展[恢复回归](../../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs)验证 snapshot、reorder、replay、重连和 Agent retry。核对每个原 node/义务/容量/receipt；terminal 六项字段保留至 matching settlement 或合法处置。原 required 不删除任一恢复或 parity 子义务；full 多阶段、长期、区域影响只按未来独立专业 authority 和实际环境验收。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [可选外部性 profile 与 legacy 边界](../../product/world-rules-core-gameplay/prd.md#root-sc33-extscope)；Trace refs: O7-REQ-e6f4ffa7-0b1b-4d0e-b7a3-8ca38c8d0244 | [Profile与授权模型](#externality-profile) | 显式启用、scope/authority fail-closed、局部授权及 legacy 隔离。 | `EXT-VERIFY-001`；SC-33 required 固定 candidate/profile 与各入口同源观察；按[测试手册](../../../testing-manual.md)扩展本节定义的真实提交/恢复场景。 | 实施 PR 的真实 profile/submit、前后摘要及各消费者证据。 | 不证明污染/区域模拟、治理资格或 release readiness。 |
| [外部性处置与其他事实隔离](../../product/world-rules-core-gameplay/prd.md#root-sc33-extdispose)；Trace refs: O7-REQ-bd83785c-d414-4bd7-b291-41a4d7272aa3 | [联合结果及处置协议](#externality-commit) | preview/fresh submit、分层单次结果、旧工作与其他事实守恒。 | `EXT-VERIFY-001`、`002`、`003`；原 required 的故障/恢复/重复/parity，完整保留根 SC 的条件；按[测试手册](../../../testing-manual.md)扩展本节定义的真实提交/恢复场景。 | 实施 PR 的工作图、容量及各域 receipt 对账和实际 CI。 | 本文未运行验收；普通工业测试不代签新增 facet，单节点不代签长时/区域影响。 |

## 12. 决策、长期风险与未决问题

| 决策或未决边界 | 理由、后果及 owner | 解除/复核触发 |
| --- | --- | --- |
| 显式 facet 与独立义务/receipt | 防标签替代权限和生产替代处置；新增跨域联合提交成本。M4 / runtime。 | profile 或工业提交边界变更时共同复核。 |
| 消费原工作图，effect key 不由 profile revision 重置 | 保持旧义务及合法后继 cycle；避免换版复制效果。runtime。 | 工作节点或处置版本演进时验证全链映射。 |
| 未提供环境/容量规则不启用具体 profile | 本文冻结逻辑接口，不自行建立污染、区域或清理政策。M4 / producer / gameplay。 | 具体 profile 提供已批准的局部计算、容量单位、动作及失效样例后才能启用。 |
| 生产完成与外部性结果联合提交，后续处置独立关联 | 防半提交与伪造清理；合法持有可能延迟完成，需要真实容量。runtime / 资源域。 | 若现有事务封套不足，先补联合提交能力并完成故障验证。 |

权限、资源及持久化实现变更仍按[开发流程规范](../../engineering/workflow/source-of-truth.md)独立评审。系统设计补齐不改变 SC-33 的 target-only 及 fresh composite evidence 要求。
