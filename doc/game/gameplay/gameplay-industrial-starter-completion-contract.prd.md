# 工业首产物结算合同

- 上层产品映射：本合同承接 `doc/game/gameplay/gameplay-top-level-design.prd.md` §2.5 的首个工业目标，并与 `doc/product/world-rules-core-gameplay/prd.md` 的资源/生产权威边界、`doc/world-simulator/m4/industrial-resource-flow-contract.prd.md` 的生产与终端结算边界对齐。
- 产品叶子入口：[`REQ-FIRST-INDUSTRIAL-001`](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)；本文保留首个工业目标的详细玩法合同，不将 target-only 能力写成当前可玩结论。
- 主题 authority：本文件只拥有“首个工业目标如何完成、失败后如何恢复、如何打开下一步”的详细玩法语义；不覆盖上层产品承诺、runtime schema 或终端实现。
- 可变执行状态：对应 GitHub Project task 与 issue evidence comments；当前实现完成度不得由本合同单独宣称。

## 1. 目标与术语

首个制成品必须在排程预览或确认前绑定唯一的玩家语义 `starter_completion_profile`。预览至少说明 evidence grade、完成边界、`next_action`、`next_recheck` 与 `progression_effect`。`RecipeScheduled`、accepted 或库存变化都不是生产完成证据。

本合同支持两个显式 profile：`production_only` 把匹配的生产 receipt 作为首产物完成；`terminal-admission` 要求匹配的 delivery/terminal settlement receipt。未声明或无法由 authority 证明 profile 时，必须 fail closed。

## 2. 玩家闭环

玩家先选择目标 profile，查看配方、原材料、能源、物流/终端前置和当前 blocker，再确认排程。成功收益是获得可追溯的生产结果或终端资格，并看到下一步工业用途、交易预览或交付选择；失败成本必须是可读的等待、容量、权限、材料、能源、路线或机会成本，而不是静默扣除。

玩家下一步只能来自权威阶段：等待匹配 receipt、补料/补电、修复前置、取得容量、交付、持有、改道、重报价或延期。系统不得把推荐、排队、accepted 或 terminal pending 伪装成完成，不得自动销毁、改道、退款或发放未结算收益。

## 3. 五阶段结算表

| 阶段 | 玩家动作 | 进度/收益/下一动力 | 失败成本与恢复边界 |
| --- | --- | --- | --- |
| `accepted/scheduled` | 查看生产回执并按 profile 等待；修复前置、补料/补电、改道、重报价或延期 | 仅证明意图接受；不推进首产物、稳定窗口、交付需求、奖励或下一目标 | 状态漂移只能原子拒绝或重报价；不得当作生产完成 |
| `production_settled` | 仅 `production_only` 可完成首产物；随后继续稳定产线或打开交易/交付预览 | 一个 matching production receipt 至多完成一次首产物，结果标为 `produced/undelivered`；只有另行声明的稳定条件全部通过后才可标为 `production-stable`，动力转为稳定产线、首笔交易或工业用途 | 无匹配 receipt 保持待决；不减少交付需求，不发 terminal 奖励，也不以首产物 receipt 代替稳定窗口证据 |
| `terminal_pending` | 等待/取得终端容量、交付、持有、改道、重报价或延期（profile 支持时） | 生产结果存在但终端未结算；不减需求、不发交付奖励，玩家承担库存/容量占用和延迟 | owner、资格、容量或路线失效时保留有界 pending/hold；不得免费销毁、自动改道或退款 |
| `delivery/terminal_settled` | 仅显式 `terminal-admission` 可进入交易、区域服务或下一工业目标 | matching delivery/terminal receipt 至多完成一次 profile，并打开一次下一 beat | 不同 root、非 matching、旧 receipt 或 replay 不得重复目标、奖励或释放容量 |
| `profile/authority unknown` | 建立/选择有效目标，或等待补证后复查 | 返回 `no_safe_starter_chain`；不产生排程、sink、稳定进度、奖励或下一 beat | 不得默认 profile，不得用 `0`、空缺口或“已完成”填充缺失 authority |

## 4. Current/target evidence cutline

当前复用基线已不是旧 simulator 的排程扣减链。`crates/oasis7_runtime/src/state/starter_industrial.rs` 定义 canonical profile、revision、completion boundary 和纯读 feasibility；`industry_transition/recipe_lifecycle.rs` 在匹配 Smelter/recipe、owner-bound output ledger、合法正批次与正量 `iron_ingot` 生产结算时写入持久 `StarterIndustrialMilestoneV1`。feasibility 消费该持久事实，只开放 Assembler 候选，避免短期 receipt 列表裁剪成为另一套进度权威。`viewer/runtime_live/gameplay_snapshot.rs` 投影同一 feasibility，`viewer/gameplay_actions.rs` 提供正式 starter 入口。这些是代码复用事实，不是同候选全入口通过结论。

当前选定路线为 `starter-industrial-smelter-to-assembler-v1` / `production_only`。正式推荐与可用性仍由同候选 fresh composite runtime + QA 证据判定：真实入口完成合法建厂、输入/电力、周期与 owner-bound 铁锭结算；核验 Viewer、pure API、Agent 的 profile revision/parity，以及重复提交、回复丢失、重连、receipt 裁剪、恢复和 replay。缺当前 authority 时返回 `no_safe_starter_chain` 并保留 blocker/复查路径。稳定窗口与 terminal delivery 独立验收；文档旧切线修正不意味着全链通过，也不能将已有 runtime milestone 一概降回仅 accepted 的旧结论。

## 5. Exactly-once、replay 与跨 surface 验收

`scripts/oasis7-pure-api-parity-smoke.sh` 的 canonical `production_only` 路径必须同时检查真实 build/recipe ack、生产前无 starter milestone、匹配 profile/revision 与 production receipt、工厂所属 site 的 owner-bound output ledger 正量铁锭 credit，以及 reconnect 后相同 milestone/产出；阶段、进度、action error 或普通库存变化不能代签。该检查读取已有完整 runtime snapshot，不新增玩家投影或完成权威。它证明该运行窗口的生产与重连接续，不单独证明独立故障域持久确认、节点灾备、稳定窗口或 delivery。

已知待验收边界：gameplay-action 重用认证 nonce 会返回 `auth_nonce_replay`，该防重放拒绝不是查询原生产结果的幂等回执。需另行验证回复丢失后沿同一请求身份查询/恢复，以及重启后不重复扣费、生产；不得将换新 nonce 的重提默认解释为原请求恢复。本检查不实现该协议变更。

- 同一 root、profile 与 matching receipt 只能推进一次；重复确认、重连、Agent retry、snapshot restore、乱序事件与 replay 必须返回原处置，不得复制 production、delivery、需求减少、奖励、`W` 或下一目标解锁。
- Viewer、pure API 与 Agent 必须对 profile、阶段、primary blocker、已占用/已消费价值、`next_action`、`next_recheck` 与 `progression_effect` 给出同义结果；任一 surface 缺少 authority 时都显示 `no_safe_starter_chain`，不得自造默认完成。
- 验收必须覆盖：accepted 不推进；`production_only` 的单个 matching production receipt 只完成一次首产物且不会提前产生 `production-stable`；稳定标签仅在另行声明的稳定条件全部通过后产生；terminal pending 不发交付收益；terminal settled 只由显式 terminal profile 完成；缺 profile/authority fail closed；报价/权限/容量漂移、重复提交、重连、乱序与 replay 不复制 sink、receipt、奖励或下一目标。

## 6. 非目标与 residual risk

本合同不新增 runtime/API/schema 字段，不规定物流或终端算法，不拍价格、税费、运输、产消或奖励数值，不设计 UI 布局，也不宣称当前 Viewer、pure API、Agent、runtime 或 QA 已完成。`production_only` 降低首局冷启动歧义，但会把物流/终端动机推迟到下一 beat；后续实现仍需验证 receipt authority、恢复动作和三端 parity。
