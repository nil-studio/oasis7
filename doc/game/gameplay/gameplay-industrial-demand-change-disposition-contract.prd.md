# 工业需求变化后的既有工作处置合同

- 上层产品映射：本合同承接 `doc/product/world-rules-core-gameplay/prd.md` 的 demand goal identity、数量分层、matching settlement、取消/到期/缩减与满足边界。
- 产品语义入口：[`工业需求目标与生产交付结算`](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-001)；本合同保留 demand 变化后的 bucket 处置与恢复权威。
- 主题 authority：本文件只拥有 demand goal 变为 `matched / cancelled / expired / unknown / reduced` 后，玩家如何处置已接受、在制、在途、已入 buffer 与 terminal-pending 工作的选择、机会成本、恢复和验收语义。
- 专业边界：M4/runtime 继续拥有 bucket、root/revision、receipt、数量守恒、ordering、持久化与 replay；本文是 target gameplay contract，不声明任何 surface 已实现。

## 1. 玩家问题与触发边界

目标状态变化不能只阻止下一次排程，还必须说明如何处置已承诺或开始的工业工作。Surface 必须从同一 fresh authority snapshot 说明 demand goal 与 `goal_authority_ref`、触发状态/数量变化、matching settlement 边界、受影响的 root/candidate/revision，以及 `accepted-unstarted / active WIP / in-transit / buffer-held / terminal-pending` 各 bucket 的数量、已消费或仍占用价值、当前用途资格、primary blocker、下一复查点和 progression effect。本文的 `goal_state=matched` 表示目标已达到其专业 authority 声明的满足边界；批量预览中的 `quantity_disposition=matched` 只表示本次数量与 remaining demand 匹配，不能单独把目标状态或 delivery/terminal settlement 提升为已完成。

`matched` 后不得为旧目标创建新的不可逆生产 sink；`cancelled / expired / unknown` 时不得继续计算需求减少、matched/surplus 或交付奖励；`reduced` 时必须按新目标量披露 remaining demand、既有 commitment 与可能形成的 residual/surplus。上述状态来自产品/专业 authority，不得由 Agent 推荐、旧订单、市场热度、客户端缓存或 production receipt 推断。

## 2. 只读处置预览

在释放 hold、继续 WIP、改变 transit、处置 buffer output、绑定新用途或产生下一不可逆 sink 前，玩家必须能读取只读 `demand_change_disposition_preview`。预览至少绑定 goal/authority snapshot、旧 root/candidate/revision、受影响 bucket 与数量、已消费/held/expected output、旧目的与候选新用途、每个真实选择的保留/损失、容量与时间机会成本、delivery/terminal/`W` 影响、可撤回性、`next_recheck` 与 `recommended_reason`。

Preview 不释放、不暂停、不完成、不改道、不迁移、不退款，也不创建新目标、revision、hold、队列位置或 reward。Authority 缺失、过期或冲突时显示 `demand_disposition_unknown/degraded`；涉及新 sink、新用途或新目标的选择必须不可提交、重新报价或原子拒绝，不能以零需求、自动完成或“沿用旧计划”填充未知事实。

## 3. 逐 bucket 玩家选择

| 既有工作 | 真实可比较的处置 | 收益、成本与边界 |
| --- | --- | --- |
| `accepted-unstarted` | profile 支持的 `release_unconsumed_hold`、`defer_existing_intent`，或显式 `bind_to_new_purpose` | 释放只处理未消费 hold/allocation 一次；延期保留可读占用；新用途必须 fresh validation，不能刷新顺位、自动迁移或把 accepted 冒充生产 |
| `active WIP` | profile 支持的 `finish_for_existing_purpose`、`hold_or_quarantine`、`rework_or_salvage`、`abandon` | 已消费投入与 lineage 保留；完成不自动减少已取消/过期目标，不默认退款；返工/新用途只产生一次 parent-linked disposition，不能继承旧奖励或 `W` |
| `in-transit` | 继续原合法承诺、`hold/return`，或 profile 支持且显式确认的 reroute/handoff | 保留原 edge、数量、损耗与 receipt；目标变化不瞬移、不免费改道，新的目的资格和容量必须重验，旧 delivery 不能被改写 |
| `buffer-held` | `hold_existing_output`、交付仍合法的既有用途，或显式绑定新 demand/purpose | 产物与 provenance 保留并继续占用有限容量；不得自动倾销、销毁、降价、伪造成交或把旧 production receipt 当作新目标 settlement |
| `terminal-pending` | 继续原合法 terminal commitment、等待/取得准入，或 profile 支持且显式确认的 hold/return/reroute/handoff | 保留 recipient、destination、admission、capacity obligation 与 provenance；旧目标变化不能提前 settlement、静默释放/迁移容量义务或把 pending 结算到新目标，释放/迁移只能由一次显式 disposition 完成 |

只有 profile 已声明且能映射到现有 authority 的选择才可展示。若某个 bucket 只有一种安全处置，仍须说明其他路径为何不可用；没有安全处置时返回 `no_safe_demand_change_disposition`，保留既有状态并给出下一次权威复查条件。

## 4. 新目标、数量与 progression 边界

新 demand/purpose 必须由玩家或有权 authority 显式选择。只改变未开始数量且专业合同证明为同一目的/候选的修订，可沿既有 root 建立 parent-linked plan revision；改变 purpose、recipient、recipe/factory/path/terminal 或其他产出因果内容时，继续服从新 candidate/root/cutover，并从 `W=0` 开始。无法证明为非因果变化时 fail closed。

旧目标的 production/delivery/terminal receipt、已满足量、奖励与 `W` 不得转移到新目标。旧工作按原 contract 合法完成时只产生其唯一旧 receipt；它不能减少已取消/过期目标，也不能自动满足新目标。只有新目标声明的 matching settlement 才能减少其 remaining demand，且同一数量不得同时归属旧目标、新目标与 surplus。

需求从 matched 恢复为新缺口、从 unknown 恢复可用或再次扩大时，必须从 fresh snapshot 重新报价；旧 stop/defer、release 或 disposition 不自动反转。继续生产、补产或建立新 revision 必须是新的显式决定，不能由后台循环、Agent retry、重连或 replay 自动恢复。

## 5. 提交、恢复与 exactly-once

提交必须重验目标状态/数量、matching settlement、bucket 数量、owner/用途资格、root/revision、输入/输出容量和可选 disposition。报价后目标满足、取消、到期、缩减、恢复或改变 recipient 时，只能返回新报价、在下一不可逆 sink 前原子拒绝，或按 profile 保持有界 pending；不得静默缩量、继续、完成、释放、改道、退款或改绑。

每个 bucket 的 release、finish、hold/return、rework/salvage、handoff 与 abandon disposition 各至多生效一次。重复 preview/submit、目标事件乱序、matching settlement 重放、重连、Agent retry、snapshot restore 与 replay 只能重读同一结果，不得复制 sink、产出、transit、buffer credit、需求减少、surplus 处置、reward、hold release 或 `W`。

Viewer、pure API 与 Agent 必须同义表达 goal state/quantity、matching boundary、五类 bucket、已消费/占用/损失价值、真实选择、机会成本、阻断、`next_recheck` 与 progression effect；对 terminal-pending 还须同义保留 recipient、destination、admission、capacity obligation、provenance 与 settlement/disposition 边界。Agent 推荐必须保留原因，且不能替玩家创建新用途或把 unsupported disposition 包装成可选。

## 6. 验收与非目标

`test_tier_required` 至少覆盖：

- 同一 deterministic fixture 中目标分别变为 matched、cancelled、expired、unknown 与 reduced；
- 五类 bucket 各存在时的 profile-supported 处置与 unsupported 隐藏；
- matched 后无新生产 sink，cancelled/expired/unknown 无需求减少或奖励，reduced 披露 residual/surplus；
- 未开始 hold 只释放一次，WIP/transit/buffer 保留 lineage，terminal-pending 保留 recipient/destination/admission/capacity obligation/provenance 且不提前结算或静默改绑，新用途 fresh validation 且按因果边界建立 revision/candidate；
- 状态恢复只 fresh requote，不自动续产；
- 重复提交、乱序、重连、retry、restore/replay 不复制任何效果；
- Viewer、pure API 与 Agent 对五类 bucket 保持 parity。

本合同不新增或冻结 demand source/预测、价格/利润/阈值/批量公式、自动取消/排空/完成/返工/退款/改道/迁移算法、maintenance 规则、runtime state/action/API/schema、Agent 行为或 UI，也不把 stop/disposition 解释成 delivery、terminal settlement、需求减少、reward、稳定窗口或 current-readiness 证据。
