# 工业配方生命周期决策合同

- 上层产品映射：本合同承接 `doc/product/world-rules-core-gameplay/prd.md` 的 SC-24 配方生命周期、版本兼容与受控退役产品承诺；六状态及其转换条件以 Product SC-24 为唯一产品 authority。
- 产品语义入口：[`配方生命周期、版本兼容与受控退役`](../../product/world-rules-core-gameplay/prd.md#配方生命周期、版本兼容与受控退役)；本合同保留六状态向玩家选择、机会成本与恢复的专业权威。
- 主题 authority：本文件只拥有六状态到玩家选择、机会成本、失败恢复、旧工作处置、progression 与 gameplay 验收的投影；不创建第二套状态或转换规则。
- 专业边界：M4/Recipe/Factory profile 拥有 recipe/factory fit、批次适用性、executable cycle、output bundle、cutover 与 disposition；runtime 拥有状态、事件、sink、receipt、持久化、replay 与幂等。

## 1. Current/target evidence cutline

当前窄证据仅覆盖静态 recipe plan 与 `ScheduleRecipe` 接受/`RecipeScheduled` 路径，不能证明 `pending_validation / validated_pending_admission / active / restricted / retiring / retired` 六态 player surface、准入、受控退役、successor conversion 或 Viewer/Agent parity 已实现。本文全部六态玩法为 `target-contract`；只有 fresh runtime + Product/M4 + gameplay + Viewer/pure API/Agent + QA composite evidence 才能升级为 current claim。

`not_licensed / incompatible / expired / unknown` 只能作为 authority/blocker reason，不是新的生命周期状态。客户端、Viewer、Agent 推荐、同名配方、下载/模拟结果或旧 receipt 都不能推断状态、激活版本、扩大作用域或复活 retired recipe。

## 2. 只读生命周期决策预览

在 `ScheduleRecipe`、新 hold、input sink、WIP 或 successor handoff 之前，玩家必须能读取只读 `recipe_lifecycle_decision_preview`。预览至少说明 recipe version/state/scope、Product authority reference、匹配的 factory capability、原材料批次数量与品质/保管适用性、必需 logistics path/capacity、power/maintenance、output bundle/terminal 前置、受影响 accepted/WIP/transit/buffer/terminal-pending 工作、已占用或已消费价值、机会成本、primary blocker、对稳定窗口 `W` 的影响、`next_recheck` 与 `recommended_reason`；terminal-pending 还必须显示 recipient、destination、admission、capacity obligation 与 provenance。

Preview 不验证、准入、激活、退役、锁定容量、创建 hold、排程、扣输入、生成 WIP/产出、推进 `W` 或发放奖励。Authority 缺失、过期或冲突时显示 `recipe_lifecycle_unknown/degraded`；会产生不可逆 sink 的候选必须不可选择、重新报价或原子拒绝，不能以零成本、通用 `recipe-fit`、同名版本或 Agent 建议填充。

## 3. 六状态的玩家投影

下表消费 Product SC-24 的 canonical state，不定义其转换：

| recipe lifecycle state | 玩家可比较的真实动作 | 即时收益、成本与 progression 边界 |
| --- | --- | --- |
| `pending_validation` | 查看缺失证据，选择 `wait_for_validation`、补证/复验、撤回或 `defer` | 收益是保留候选并明确补证路径；承担等待/验证成本。不可排程，不产生 hold、input sink、`W`、产出或奖励 |
| `validated_pending_admission` | 查看验证与准入范围，选择等待/请求准入、比较当前有效版本、重新报价、撤回或延期 | validation 仍是 evidence-only；“已验证”不得伪装已生效，不产生生产资格、reservation、sink、`W` 或奖励 |
| `active` | 比较当前 batch、工厂、来源、路径、power、terminal 后排程或延期 | 只有 submit 时 fresh revalidation 仍通过才启动一次；preview/accepted 不等于生产、交付或里程碑 |
| `restricted` | 在声明作用域内排程/继续承诺、缩小到合法作用域、请求重新授权、hold 或延期 | 只有 owner/区域/用途/工厂/时间作用域内实际 receipt 产生一次收益；越界无 sink、不继承 `W`，不得显示为永久或全局容量 |
| `retiring` | 对既有工作选择 profile 支持的 drain/finish、hold、quarantine、return、rework、salvage、reroute、terminate，或显式确认 successor | 不接受新排程；保全或处置旧价值会承担时间、容量、损耗与交付成本。退出本身不发奖励，successor 必须 fresh validation、parent-linked 且 `W=0` |
| `retired` | 查看历史/最终处置/successor lineage，选择当前有效新版本或其他工业目标 | 终态且不可排程、激活或复活；新版本/候选从自己的 authority 与 `W=0` 开始，历史读取无世界效果 |

每个非 active 状态只展示 profile 真实支持的动作，并说明下一决策点。只有一个安全动作时，说明其他路径不可用；补证、等待、继续、授权、处置或 successor 均无安全路径时返回 `no_safe_recipe_lifecycle_fallback`，不得自动换配方、免费转换、静默降级或伪造退款。

## 4. 既有工作的处置边界

每个 accepted 任务固定其 recipe version/authority、factory capability、input batches、path/edge、output bundle 与 terminal purpose。Recipe state/scope 在提交后变化时，旧工作保持 pinned 旧身份，不被静默取消、改绑、迁移或重贴标签。

| 既有工作 | profile-gated 处置与边界 |
| --- | --- |
| `accepted-unstarted` | 尚未发生首个 sink 时可保持无效果 pending、release/hold、重报价或 terminate；不得刷新顺位、自动切 successor 或把 accepted 冒充 WIP |
| `active WIP` | 保留已消费投入与 lineage；只允许 finish/pause/hold/quarantine/rework/salvage/terminate 中真实支持的结果，不默认退款、完成或转换 |
| `in-transit` | 保留原 edge、数量、损耗、目的与 receipt；只允许 finish/hold/return/reroute/reject 中 profile 支持的处置，不瞬移或冒充 delivery |
| `buffer-held` | 保留产品、owner、ledger、容量与 provenance；只允许 hold/handoff/conversion/reject 中 profile 支持的处置，不自动销毁、转卖或 credit successor |
| `terminal-pending` | 保留 recipient、destination、admission、capacity obligation 与 provenance；只允许继续原合法 commitment，或使用 profile 支持的 hold/return/reroute/handoff/reject 单次处置，不提前 settlement、静默改绑或重复释放容量义务 |

`retiring` 只允许 cutoff 前已有工作按声明规则排空或一次性处置；`retired` 不产生新效果，只保留历史与已记录的最终 disposition。旧版本合法完成时只产生其唯一旧 receipt，不贡献 successor 的 stable window、需求或奖励。

## 5. Successor、fresh submit 与 exactly-once

改变输入/输出/副产物、规格/品质、阶段/边、factory fit、power/logistics、terminal purpose/eligibility 等产出因果内容时，Product/M4 authority 必须建立新 version/candidate 与 parent linkage；gameplay 只表达旧/新选择、保留/损失和 `W=0`。非因果 metadata 才能保留 identity；无法证明为非因果时 fail closed。

兼容 successor 也必须由玩家或有权 authority 显式确认，并经过 fresh authorization、factory/input/path/power/terminal revalidation。Conversion/handoff 只产生一次 parent-linked receipt，记录旧/新身份、实际数量/损耗与 disposition；successor 不继承旧 reservation、queue priority、receipt、delivery、`W`、progress 或 reward。

提交必须从 fresh state 重验 lifecycle state/scope 与全部前置。报价后版本、授权、factory fit、batch quality、path/capacity、power 或 terminal 漂移时，只能重报价、在首个 sink 前原子拒绝，或按 profile 保持有界 pending；不得静默激活、扩大 scope、切换同名版本或自动迁移旧工作。

每个旧工作、cutoff 与 successor handoff 各至多产生一个 disposition/result。重复 preview/submit、授权或状态事件乱序、重连、Agent retry、snapshot restore 与 replay 只能重读同一状态、blocker 与 receipt，不得复活 retired version、复制 sink/WIP/产出/delivery/hold release/`W`/reward、刷新容量顺位或重复 conversion。

## 6. Parity、验收与非目标

Viewer、pure API 与 Agent 必须同义表达 canonical lifecycle state、authority/blocker reason、scope、factory/input/path/power/terminal fit、五类旧工作、真实动作、保留/损失/占用、successor/cutoff、`W` 影响、`next_recheck` 与 progression effect。Agent 只能在授权 scope 内建议或请求确认，不能改变 state、扩大授权或替玩家迁移工作。

`test_tier_required` 至少覆盖：六态各一例；preview 无副作用；pending/admission 无 sink/`W`/reward；active/restricted fresh submit 与越界拒绝；retiring/retired 无新排程；accepted-unstarted/WIP/transit/buffer/terminal-pending 各一个唯一处置，其中 terminal-pending 保留 recipient/destination/admission/capacity obligation/provenance 且不得提前 settlement 或静默改绑；causal successor parent-linked 且 `W=0`；authority/factory/input/path/power/terminal drift 的 requote/atomic reject；重复提交、乱序、重连、retry、restore/replay 不复制或复活；Viewer/pure API/Agent parity。`test_tier_full` 再覆盖多版本并发、多阶段 WIP/transit/buffer/terminal-pending、连续 scope 变化、cutover 部分失败、持久化/replay 与 successor conversion。

本合同不定义或冻结六态转换、recipe catalog/发现/解锁、授权角色、版本兼容算法、配比/产率/quantization/rounding、quality/custody、byproduct、changeover/maintenance 成本、factory capability lifecycle、M4/runtime state/event/API/schema、queue/fairness、自动换配方/迁移/退款、Agent 行为或 UI，也不声明 current implementation/readiness。
