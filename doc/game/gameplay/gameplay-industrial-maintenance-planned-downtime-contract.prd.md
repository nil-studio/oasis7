# 工业工厂维护与计划停机合同

- 上层产品映射：本合同承接 `doc/product/world-rules-core-gameplay/prd.md` 的 SC-22 维护/计划停机产品承诺。
- 产品语义入口：[`工业流水线生命周期到玩家决策的收口`](../../product/world-rules-core-gameplay/prd.md#工业流水线生命周期到玩家决策的收口)；本合同保留维护前比较、停机处置与恢复节奏权威。
- 主题 authority：本文件只拥有维护前玩家选择、机会成本、计划停机处置、恢复节奏与 gameplay 验收语义；Product 继续拥有维护承诺，M4/runtime 继续拥有未来维护事实、bucket、receipt、执行、持久化与 replay。
- 可变执行状态：对应 GitHub Project task 与 issue evidence comments；本文是 target gameplay contract，不声明 maintenance runtime、Viewer、pure API 或 Agent 已完成。

## 1. Current/target evidence cutline

当前 `ScheduleRecipe` 可证明的只有排程电力账本与 Agent idle-battery 风险信号：`electricity_cost / electricity_after`、`runway_before_ticks / runway_after_ticks / downtime_threshold_ppm / continue_production_risk`。当前排程不扣 battery，两个 battery runway 值相等；`maintenance_pressure_delta=unchanged` 只表示现有路径没有维护压力变化，不能证明工厂拥有 maintenance runway、折旧、维修消耗、计划停机能力或“可安全继续生产”。

只有 runtime 提供可扣减、可恢复且会阻断工厂的权威维护真值后，`factory_maintenance_status` / `schedule_quote` 才能表达 `maintenance_runway_before_ticks / maintenance_runway_after_ticks / maintenance_downtime_threshold / maintenance_pressure_delta / maintenance_failure_cost / recommended_maintenance_action`。没有该 authority 时必须显示 `maintenance_not_tracked` 或完全不展示维护 runway，不得以 `0`、无限值、电力余额、battery runway、旧 receipt 或 `unchanged` 填充。

当前回归只能证明电力账本与 battery runway 独立，以及 maintenance 尚未被错误冒充；它不能支持 maintenance/player-surface readiness。Future maintenance fields、临界/停机、恢复建议和 Viewer/LLM playtest 都保持 target-only，必须由 fresh runtime + gameplay + Viewer/Agent + QA composite evidence 才能升级。

## 2. 只读维护决策预览

当 profile 存在权威维护状态或计划维护窗口时，在维护 sink、排程 sink、WIP 创建或停机承诺之前，玩家必须能读取只读 `maintenance_decision_preview`。预览至少说明 factory/capability/recipe/root/revision、维护目标与作用范围、当前 runway/threshold/pressure、追加资源与仍占用、预计产出与复查时延、对 accepted/WIP/transit/buffer/terminal、稳定窗口 `W` 和交付承诺的影响、故障/停机/产出损失风险、可撤回性、`next_recheck` 与 `recommended_reason`。

Preview 不推进 maintenance、排程、tick、资源账本、停机、hold、`W` 或 reward，不锁定未来维修能力或队列位置。Authority 缺失、过期、冲突或在 submit boundary 仍为 unknown 时，涉及不可逆 sink、停机/稳定影响或交付承诺的候选必须不可选择、重新报价、原子拒绝或按 profile 保持有界 pending；pending 不产生隐式 hold。只有专业合同真实支持且明确标记 `at-risk/unknown` 的路径可继续展示。

## 3. 玩家选择与机会成本

| 选择 | 玩家收益 | 成本、失败与恢复边界 |
| --- | --- | --- |
| `maintain_before_run` | 先恢复 profile 声明的维护能力，保护稳定产线与下一次交付机会 | 承担维护资源、停机时间与容量占用；维护完成不等于生产、交付或奖励 |
| `run_at_risk` | 在 profile 真实支持时保留当前生产时机 | 承担明确的故障、产出损失、WIP/交付延期与恢复成本；unknown authority 不能包装成可接受风险 |
| `reduce_load` | 降低下一合法 cycle 的维护/停机暴露，保留输入和恢复弹性 | 吞吐、目标与交付变慢；只按合法 quantum/profile-supported partial 降载，不得客户端截断 |
| `defer` | 保留资源与选择，等待维护 authority、材料或窗口改善 | 承担交付与进度延迟；等待不续期 quote/hold、不刷新优先级，也不自动恢复排程 |

推荐必须指向当前最小可执行动作，解释它如何恢复交付、打开下一单或避免当前工业目标倒退。只有一个真实选择时说明其他路径为何不可用；没有安全选择时返回 `no_safe_maintenance_decision` 与下一次权威复查条件，不能以通用修复、补电或 Agent 建议伪造维护能力。

## 4. 计划停机与既有工作

计划维护暂停继续服从现有稳定产线合同：当前候选的 `W` 清零，暂停期间不增加进度，恢复后从 candidate 重新完成完整窗口；暂停前历史达成可保留，但不能作为恢复后的当前窗口进度或再次发奖。维护本身不改变 production/delivery/terminal settlement 边界。

| 既有工作 | 允许的玩家表达与处置边界 |
| --- | --- |
| `accepted-unstarted` | profile 支持时释放未消费 hold、保持有界 pending 或延期；不得把未开始工作伪装成 WIP、返还已经发生的 sink 或刷新顺位 |
| `active WIP` | 保留已消费投入、root 与 lineage；只能按 profile 支持继续、暂停、hold/quarantine、finish、rework/salvage 或 terminate，不能自动退款、完成或迁移 |
| `in-transit` | 保留原 edge、数量、损耗、目的和 receipt；维护不能瞬移、免费 reroute 或把 arrival 冒充 delivery settlement |
| `buffer-held` | 保留已生产产物、有限 buffer 占用与 provenance；可按 profile 等待、持有或路由到合法下一阶段。Production-only profile 可保持 production-stable，但不得冒充已交付、需求减少或 terminal reward |
| `terminal-pending` | 保留目的、recipient、资格、容量义务与 provenance；只能等待/取得准入、合法交付或走 profile-supported 处置。若 terminal admission 属于 `W` 条件则 pending 不计进度，且任何 profile 都不得在 matching settlement 前发放终端效果 |

恢复必须从 fresh maintenance、owner、factory/recipe、power、capacity、bucket 与 terminal state 重验。维护完成只允许一次受支持的 maintenance/production disposition；既有 WIP/transit/buffer 只继续原 lineage 或走显式 parent-linked disposition，不因恢复创建新产物、交付、`W`、奖励或新用途。

## 5. Exactly-once、parity 与 anti-abuse

重复 preview/submit、维护完成或窗口事件乱序、重连、Agent retry、snapshot restore 与 replay 只能重读同一维护/停机/恢复结果，不得复制维护 sink、hold release、排程 sink、WIP、产出、delivery、reward、里程碑或 `W`。前置漂移必须原子拒绝或重报价，并保留原任务及未结算状态；部分失败不能留下 orphan hold、既停机又完成或既退款又产出的状态。

Viewer、pure API 与 Agent 必须同义表达 maintenance authority/evidence class、目标与作用范围、runway/threshold/pressure、四类选择、accepted/WIP/transit/buffer/terminal-pending 影响、已消费/占用/损失价值、`W`/delivery/terminal 影响、primary blocker、`next_recheck` 与 progression effect。对 `terminal-pending` 还必须同义保留 destination、recipient、admission、capacity obligation、provenance 与适用的 `W` policy：terminal admission 属于 `W` 条件时 pending 不计进度；production-only profile 在维护后从 candidate 重计 `W`，可达到 production-stable，但 matching settlement 前任何 profile 都不产生 delivery/terminal effect，容量义务也只能由一次显式 profile disposition 释放或迁移。Agent 推荐必须保留原因，且不得独自创建 maintain/pause/resume/reduce/terminate 能力。

反复查看 quote、计划/取消尚未生效的维护、暂停/恢复或拆分低负载批次不能刷 maintenance、稳定进度、容量顺位或成长奖励。维护玩法收益是保护已有能力、降低已披露故障风险或恢复可交付性，不是独立的免费产出或里程碑来源。

## 6. 验收与非目标

`test_tier_required` 分层覆盖：

1. current regression 证明电力/battery 信号与 maintenance 独立，`maintenance_pressure_delta=unchanged` 不产生 maintenance cover；
2. target fixture 证明正常/临界维护状态下四类选择、quote 无副作用、submit fresh revalidation、unknown fail-closed、有界 pending 无隐式 hold；
3. planned downtime 在 accepted-unstarted/WIP/in-transit/buffer-held/terminal-pending 各 bucket 中保留 lineage、唯一 disposition、`W` 清零与恢复重计，其中 terminal-pending 保留 destination、recipient、admission、capacity obligation 与 provenance，容量义务只由一次显式 profile disposition 释放或迁移，并分别证明 terminal-admission profile 在 matching settlement 前不计 `W`、production-only profile 从 candidate 重计后可达到 production-stable，而两者都不提前产生 delivery/terminal effect；
4. 重复提交、乱序、重连、retry、restore/replay 不复制效果；
5. Viewer、pure API 与 Agent 对上述所有 bucket 及其 profile-specific `W` policy 保持 parity。

`test_tier_full` 再覆盖多工厂共享维护资源、连续计划窗口、长 WIP/transit、恢复失败与 composite playtest。

本合同不新增或冻结维护资源、折旧、损耗、价格、runway/threshold/产率/停机时长公式，不新增 runtime state/action/API/schema、queue/fairness、自动维护/停机/恢复/退款/迁移算法、Agent 行为或 UI，也不改写电力/battery、稳定 `W`、生产/交付结算、工厂 lifecycle、需求、背压或 current-readiness authority。
