# 工业流水线背压恢复合同

- 上层产品映射：本合同承接 `doc/product/world-rules-core-gameplay/prd.md` 的下游阻塞、机会成本与恢复表达，并消费 `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md` 的有限 buffer、allocation、hold 与 backpressure 权威事实。
- 产品语义入口：[`多阶段工业流水线与中间品背压`](../../product/world-rules-core-gameplay/prd.md#多阶段工业流水线与中间品背压)；本合同保留背压下玩家选择、机会成本与恢复权威。
- 主题 authority：本文件只拥有“下游容量不足时玩家如何比较暂停、降载、持有、合法改道或等待，以及容量恢复后如何继续”的玩法节奏、机会成本与验收语义。
- 可变执行状态：对应 GitHub Project task 与 issue evidence comments；本文是 target gameplay contract，不声明 runtime、Viewer、pure API 或 Agent 已完成。

## 1. 玩家问题与触发边界

当 destination buffer、下游 stage、edge throughput 或 terminal admission 无法接收当前或下一批输出时，玩家不能只看到“生产失败”或无限等待。Surface 必须从同一权威快照指出 earliest downstream blocker、受影响的上游 root/stage、尚未开始与已经开始的工作、当前 WIP/in-transit/buffer 数量、已消费或仍占用的价值、对交付与稳定窗口 `W` 的影响，以及下一次权威复查点。

背压恢复只在 profile 已声明对应动作、作用域与处置能力时成立。`pause_upstream`、`reduce_or_defer_upstream`、`hold_output`、`reroute_to_supported_destination`、`release_unstarted_capacity` 与 `wait_for_capacity` 都是玩家语义标签，不是新 runtime action；无法映射到现有权威能力的选项不得显示为 selectable。

## 2. 只读恢复预览

在新的上游 input sink、WIP 创建或输出 branch credit 之前，玩家必须能读取只读 `backpressure_recovery_preview`。预览至少绑定 root/candidate/revision、阻塞 stage/edge/buffer/terminal、当前容量与已占用量、accepted-unstarted/WIP/in-transit/buffer-held/terminal-pending 分层、输入/产出/hold 的保留或损失、预计交付与 `W` 影响、每个真实选项的可撤回性、`next_recheck` 与 `recommended_reason`。`terminal-pending` 必须保留 recipient、admission、capacity obligation、destination 与 provenance；matching settlement 前不得产生 delivery/terminal effect，容量义务只能由一次显式 profile disposition 释放或迁移。

Preview 不锁容量、不释放 hold、不暂停或启动工作、不改道、不创建队列位置，也不保证 ETA、优先级或未来容量。Authority 缺失、过期或冲突时显示 `backpressure_unknown/degraded`；涉及新不可逆 sink 的候选必须保持不可选择、重新报价或原子拒绝，不能用 `0` 容量、空 blocker 或 Agent 推荐补齐未知事实。

## 3. 玩家选择与机会成本

| 选择 | 玩家即时收益 | 成本、失败与恢复边界 |
| --- | --- | --- |
| `pause_upstream` | 在 profile 支持时避免继续生成无法接收的 WIP/输出，保留后续恢复选择 | 承担停机、交付延期与 `W` 影响；已经消费的投入、WIP、transit 与 buffered output 不自动退款、回滚或迁移 |
| `reduce_or_defer_upstream` | 降低下一合法 cycle 的输入、输出和容量压力，保留原料与恢复弹性 | 吞吐与目标推进变慢；只有合法 executable quantum/profile-supported partial 才能降载，不得客户端截断或生成半批产物 |
| `hold_output` | 在合法且有界的 ledger/buffer 中保留已结算产物与 lineage，等待下游恢复 | 继续占用容量并承担时间、保管与交付机会成本；buffer 已满或资格不成立时不能无限堆积、静默销毁或免费扩容 |
| `reroute_to_supported_destination` | 在已有专业合同明确支持时，把未决输出或后续承诺转向合法目的地 | 披露额外 edge/time/power/loss 与用途变化；既有 WIP/transit/buffer 不静默迁移，因果变化继续服从 parent-linked candidate/cutover 与 `W=0` |
| `release_unstarted_capacity` | 释放尚未消费的 hold/allocation，减少占用并允许一次可归因重评 | 只释放 accepted-unstarted 的未消费部分一次；不能撤销已发生 sink、刷新顺位或把开始后工作伪装成未开始 |
| `wait_for_capacity` | 保持当前未决意图与 lineage，避免不受支持的处置 | 承担可读的库存、buffer、时间和交付机会成本；等待不续期 quote/hold、不保证顺位，也不能替代 bounded termination/replan |

推荐只能解释哪项真实选择最能保护当前目标、已投入价值或恢复弹性，不能自动执行动作、隐瞒缺少安全替代方案的事实，或把纯等待说成无成本方案。只有一个真实选择时仍须说明其他选择为何不可用；没有安全选择时返回 `no_safe_backpressure_recovery` 与下一复查条件。

## 4. 提交、恢复与状态守恒

提交必须从 fresh authority state 重验 blocker、容量、owner/资格、root/revision 与受影响工作。状态漂移只能返回新的只读报价、在下一不可逆 sink 前原子拒绝，或按 profile 进入有界 pending；不得静默暂停、缩量、改道、释放、扩容或生成隐式 hold。

容量释放或下游恢复只重评仍 pending 的 edge、branch、intent、hold 或 terminal admission。已经 settled 的 branch、已释放的 capacity、已完成的 input sink、WIP、transit、buffer-held、terminal-pending、production/delivery receipt 与稳定进度保持原 identity 与已发生结果；terminal-pending 只能继续等待或走一次 profile-supported disposition，不得自动重启上游、提前 terminal settlement、重放已结算 branch、复制输入/产出/奖励，或把旧 quote 变成新授权。

重复预览、提交、重连、Agent retry、arrival reorder、snapshot restore 与 replay 对同一 root/revision/disposition 只能重读同一结果。表中选择及其 profile-supported continuation/rejection disposition 各自至多形成一次受支持结果；部分失败必须保留可追溯的原工作或显式拒绝，不能留下 orphan hold、双重容量占用或 pending/settled 并存的同一数量。

## 5. 玩家节奏与 anti-grind

背压处理的玩法收益是保护稀缺投入、避免无意义 WIP、恢复可交付能力或打开一个有根据的重新规划选择；它本身不构成新产能、产出、需求满足、`W` 或成长奖励。玩家反复暂停、等待、重报价、释放/重占同一容量，若没有新的世界用途、恢复弹性或可执行路径，应标记为 unresolved backpressure，而非新的工业里程碑。

Viewer、pure API 与 Agent 必须同义表达 earliest blocker、上游影响、各 bucket 数量、held/consumed/lost value、真实可选动作、机会成本、`next_recheck` 与 progression effect；对 terminal-pending 还须同义保留 recipient、admission、capacity obligation、destination、provenance 与 settlement 边界。Agent 选择与建议必须留下原因；任何 surface 都不得独自发明表中动作、continuation 或 compensation 能力。

## 6. 验收与非目标

`test_tier_required` 至少覆盖一个 deterministic 三阶段 fixture：

- 下游 buffer 满时比较两条以上 profile-supported 恢复路径；
- 暂停/降载不创建额外 sink/WIP，合法 hold 保留 lineage，已开始 WIP/in-transit 不被退款或迁移；
- terminal admission 阻塞时 terminal-pending 保留 recipient/admission/capacity obligation/destination/provenance，matching settlement 前不产生 delivery/terminal effect，容量义务只由一次显式 disposition 释放或迁移；
- capacity release 只释放一次并只重评 pending 工作；
- 恢复后 continuation 只结算一次；
- unknown/stale authority 不产生 selectable effect；
- 重复 preview/submit/reconnect/retry/restore/replay 不复制容量、输入、输出、receipt、`W`、奖励或优先级；
- Viewer、pure API 与 Agent 对所有 bucket 及 settlement 边界保持 parity。

本合同不新增或冻结 buffer/queue/routing/fairness 算法、容量/优先级/ETA/损耗/价格公式、runtime state/action/API/schema、UI 布局、自动重启/停机/改道/倾销/退款/补偿/迁移，也不改写批次质量、主副产物 bundle、需求停产、工厂生命周期、外部性、生产/交付结算或 current-readiness 合同。
