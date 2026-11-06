# 工业需求目标与生产交付结算产品设计

## 文档身份

- 配对产品 PRD：[`industrial-demand-goals-and-settlement.prd.md`](industrial-demand-goals-and-settlement.prd.md)
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-11-06
- 专业域权威：[`gameplay 工业 walkthrough 合同`](../../game/gameplay/gameplay-industrial-representative-execution-walkthrough.prd.md#3-五阶段玩家-walkthrough)、[`world-runtime 专业 PRD`](../../world-runtime/prd.md#consumer-compatibility-industrial-profile-and-stage-execution)、[`M4 工业资源流转合同`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md#4-technical-specifications)、[`testing 专业 PRD`](../../testing/prd.md#6-validation-decision-record)

目标技术承接：[工业需求目标、承诺与匹配结算](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-authority)与[独立验收方法](../../testing/manual/industrial-demand-goal-acceptance.manual.md)补齐 producer、支持集、matching、补产、恢复与正式消费者；本批次普通输入交付选择待 owner 随 PR 采纳，不代表现有能力。

本文说明玩家怎样把一个 demand goal 变成可比较、可追溯、可恢复的工业承诺。配对 PRD 拥有 SC-31 叶子要求和验收；专业 authority 拥有批量、产率、receipt、状态、队列、API 和测试实现。本设计不复制任务状态、实现字段或专业数值。

## 1. 设计命题

玩家面对一个有权威身份的需求目标，需要知道目标还缺多少、已有多少在承诺/生产/运输/结算中、什么会被占用，以及选择整批、减量、补料/调运、持有盈余或暂缓的代价。目标完成的体验必须来自 matching delivery 或 terminal settlement，而不是生产、buffer、预测或 Agent retry。

核心决策是先比较，再确认承诺，再分别观察生产和匹配结算，最后决定补产或保留缺口。preview 和 recommendation 不产生 sink、hold、queue、产出、结算或奖励；提交后按当前 authority 重新校验。

设计假设是保留 shortage 与 surplus 的独立可见结果，能让玩家理解资源压力和机会成本，并减少重试套利。该假设需要玩法、入口和 QA 证据验证。

## 2. 状态、信息与反馈

| 阶段 | 玩家可见信息 | 可选行动 | 成本/承诺 | 反馈与下一步 |
| --- | --- | --- | --- | --- |
| Preview | `goal_authority_ref`、target、committed、produced、delivery-settled、terminal-settled、remaining、shortage/matched/surplus、占用和 `next_recheck` | 比较专业合同支持的 full、reduced、补料/调运、hold、stop/defer | 无世界效果；unknown/blocked 不补成零成本 | 选择确认、重新报价或回到目标选择 |
| Accepted | 绑定目标、数量、当前条件和可重试边界 | 等待、修复、补料、调运、重排或停止 | 只在权威接受时形成有限承诺；不等于生产/交付 | accepted、blocked、requote 或有界 pending |
| Produced | production receipt、实际产物、在途/buffer 和仍占用价值 | 等待 matching settlement、处理输出、持有或恢复 | production 不减少需求、不发交付奖励 | produced/undelivered，保留交付 blocker |
| Matched settlement | matching delivery/terminal receipt、已满足量和 remaining shortage | 继续另一个目标、一次 supplemental revision 或 stop/defer | 只按匹配数量减少一次目标 | satisfied、partial shortage 或 surplus |
| Surplus/shortage | 合法 surplus、baseline、actual、损耗、已满足量和缺口 | 仅选择专业合同支持的后续处置 | 不自动倾销、销毁、伪成交或计成长 | 资产和缺口可回看，下一复查点明确 |
| Drift/unknown | 变化了的目标、库存、批量、产率、容量或缺失 authority | requote、无副作用拒绝、有界 pending 或改目标 | 旧报价不继承资格、风险、容量或奖励 | 显示变化原因、保留结果和下一步 |

状态优先级保护授权、不可逆损失和资源占用，再呈现可恢复前置与补充信息。入口不得把 accepted 当成 started，把 production 当成 delivery，把 buffer 当成 settlement，或把 unknown 当成零。

## 3. 正常循环与关键取舍

正常循环是“读取同一快照 → 比较真实支持的方案 → fresh confirm → 生产与运输 → matching settlement → 补产或停止”。整批可能更高效但占用更多容量和资源；减量可能保留灵活性但留下缺口；补料/调运可能改善可达性但引入时间、能量或运输成本；hold surplus 保留资产但继续占用空间；stop/defer 放弃即时完成但不制造虚假的补产。

产品层只表达可比较的作用范围、追加成本、仍占用价值、主要风险、预计结果和复查点。正式 profile 不支持的路径不应出现在玩家动作集中。matching settlement 不足时，supplemental revision 必须与原目标 parent-linked，并保留 baseline、actual、损耗、已满足量和 shortage；stop/defer 不创建 revision。

## 4. 机制与体验关系

| 体验目标 | 玩家决策 | 权威约束 | 预期结果 | 失败信号 |
| --- | --- | --- | --- | --- |
| 目标可比较 | 是否确认某一计划 | 同一 authority snapshot 和 `goal_authority_ref` | 数量层次、占用、机会成本和复查点一致 | unknown/blocked 或报价过期 |
| 生产可归因 | 是否等待或处理生产结果 | root/revision、production receipt、实际账本 | 产出可追溯但目标仍可能未满足 | produced/undelivered 或 terminal pending |
| 结算可验证 | 是否继续、补产或停止 | matching delivery/terminal settlement | 目标只按匹配数量减少一次 | non-matching settlement 不改变目标 |
| 风险可恢复 | 是否重报价、改道或放弃 | profile 的 pending/reject/revision policy | 保留已消费、占用、损耗和缺口 | drift、容量、资格或 authority 失效 |
| 入口同义 | 是否切换 Viewer、pure API 或 Agent | 同一权威快照、blocker 和 action contract | 三入口共享目标真值和下一步 | 任一入口宣称不同完成或奖励 |

## 5. 行为与后果

生产、交付、terminal settlement、需求减少、surplus 处置、奖励和容量释放都必须有各自可追溯的权威结果。旧 schedule、后台循环或 Agent retry 在目标满足后不得追加生产或重复奖励；新的生产必须是新目标或新的显式选择。

重复 submit、delivery、reconnect、乱序、restore、retry 和 replay 只能返回同一 root/revision 的原 disposition。不同 accepted intent 即使 payload 相同，也要分别归因；旧 receipt、reservation、损耗和未决义务不能迁移到新因果链。因果变化建立 parent-linked 新 revision/candidate，原历史保持不变。

## 6. 资源与经济影响

产品读面需要分开表达原料、owner-held electricity、物流损耗、buffer/terminal 容量、已承诺价值、实际消耗、仍占用和可追回价值。缺失的维护、价格、损耗、容量或终端价值 authority 必须显示 unknown/degraded，不能补成零成本、零损耗或无限容量。

该循环保持 world-first：成本和价值来自同一世界历史及 receipts；emergence-first：来源、路径、容量和终端之间存在可理解的取舍；persistent/auditable：计划、baseline、actual、损耗、缺口和 settlement 跨重连与 replay 延续；extensible：未来 profile 可以声明自己的 cost/value policy，但仍须遵守分层、单次效果和 authority 边界。

## 7. 失败、阻塞与恢复

目标、库存、batch quantum、产率或 terminal capacity 漂移时，玩家看到 requote、无副作用 atomic reject 或有界 pending，并知道保留结果、变化原因和下一步。匹配不足时可选择一次受支持的 parent-linked supplemental revision，或停止/延期并保留 shortage；surplus 只能进入专业合同支持的显式处置。

终端 owner、资格、需求、路径或容量失效时，不得自动退款、免费补偿、静默改道、伪造交付或把 production receipt 当作 settlement。恢复说明 primary blocker、已消费/仍占用/已损失/可追回价值和 `next_recheck`；没有安全路径时停止并返回目标选择。

## 8. 入口、可读性与可访问性边界

Viewer、pure API 和 Agent 可以采用不同表达方式，但必须使用同一权威事实，保持 target/committed/produced/delivery-settled/terminal-settled/remaining、shortage/matched/surplus/unknown、primary blocker、允许动作、机会成本和复查点同义。设计不冻结字段名、API、Agent prompt、控件位置、布局或动效；入口需让 accepted、produced、pending、settled、shortage 和 surplus 可辨识，并给出真实恢复动作。

## 9. 取舍、验证与证据边界

采用“分层结算、显式补产、显式停止”会牺牲后台自动化的表面顺滑，但保留目标因果、资源压力、机会成本和反套利边界。采用同一快照的提交前比较会要求玩家接受 unknown/blocked 的谨慎结果，但避免沿旧价格、容量或资格作出不可逆承诺。

设计覆盖的产品要求和场景见：[`同一快照比较`](industrial-demand-goals-and-settlement.prd.md#req-sc31-001)、[`受支持的选择`](industrial-demand-goals-and-settlement.prd.md#req-sc31-002)、[`生产与结算分层`](industrial-demand-goals-and-settlement.prd.md#req-sc31-003)、[`满足与 surplus`](industrial-demand-goals-and-settlement.prd.md#req-sc31-004)、[`补产与缺口`](industrial-demand-goals-and-settlement.prd.md#req-sc31-005)、[`漂移处置`](industrial-demand-goals-and-settlement.prd.md#req-sc31-006)、[`幂等恢复`](industrial-demand-goals-and-settlement.prd.md#req-sc31-007)、[`多入口同义`](industrial-demand-goals-and-settlement.prd.md#req-sc31-008)、[`提交前验收`](industrial-demand-goals-and-settlement.prd.md#ac-sc31-001)、[`结算分层验收`](industrial-demand-goals-and-settlement.prd.md#ac-sc31-003)、[`补产验收`](industrial-demand-goals-and-settlement.prd.md#ac-sc31-006)、[`漂移验收`](industrial-demand-goals-and-settlement.prd.md#ac-sc31-007)、[`重复与回放验收`](industrial-demand-goals-and-settlement.prd.md#ac-sc31-008) 和 [`入口同义验收`](industrial-demand-goals-and-settlement.prd.md#ac-sc31-009)。本次目标 profile 选择、原数值 authority 和入口实施条件见 [`PRD 未决问题`](industrial-demand-goals-and-settlement.prd.md#7-设计取舍与未决问题)，该链接区分本次待采纳专业选择、其他 profile 的未声明政策及实现/实证交付，不复制任务执行状态。

`test_tier_required` 应覆盖同一快照 preview、只展示真实支持的方案、生产/交付分层、满足后停止、surplus、一次 supplemental revision 或 stop/defer、drift 重报价/拒绝、重复无副作用和三入口 parity。`test_tier_full` 再覆盖并发目标、跨窗口、多批次/损耗、容量争用、持久化恢复、补偿和长期入口一致性。文档采纳、checker green 或合成样例只能证明结构/合同可判定性，不能证明当前实现、数值平衡、玩家体验或发行就绪。

## 10. 相邻权威与未决边界

[`game` 专业 PRD](../../game/prd.md#3-player-facing-authority-boundary) 拥有玩家动作和机会成本，[`world-runtime` 专业 PRD](../../world-runtime/prd.md#consumer-compatibility-industrial-profile-and-stage-execution) 拥有状态、root/revision、receipt、持久化和 replay，[`M4 工业资源流转合同`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md#4-technical-specifications) 拥有批次、物流、终端、守恒和 profile，[`testing` 专业 PRD](../../testing/prd.md#6-validation-decision-record) 拥有组合证据；本设计只拥有产品体验和跨域连接。[本次目标设计选择](industrial-demand-goals-and-settlement.prd.md#72-本次目标设计选择与后续条件)将普通工业输入交付的 full/reduced、partial、surplus、显式补产及多入口语义具体化；它是本 PR 待采纳设计，尚无当前实现或组合证据。配方/费用/容量仍由原专业来源拥有，其他 profile 的 terminal、奖励、制度和服务窗口不隐含启用。#3650 是产品文档历史来源，不能证明本次策略早已批准。
成熟世界的[有限供应用途政策](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#regional-supply-use-policy-v1)与[执行设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-delivery)采用本专题原输入需求及匹配交付 authority：既有合法库存可以满足明确接受该来源的需求，原需求匹配后停止追加。接收方之后显式执行的用途属于其独立当前意图；用途已证、首次成长和本专题的交付/终端结算分别读取，不互相补签，也不自动续产、发奖或生成另一套需求账。该有限适配尚须实现，不改变其他 profile 的支持范围。

## PRD REQ/AC fragment mapping

This design delegates product requirements and acceptance semantics to its paired PRD. Every row below is a navigable fragment mapping; this design does not create a second requirement authority.

| PRD requirement | PRD acceptance |
| --- | --- |
| [REQ-SC31-001](industrial-demand-goals-and-settlement.prd.md#req-sc31-001) Trace refs: O7-REQ-bfd4b2f4-6b21-4c4a-961d-387b220e762f | [AC-SC31-001](industrial-demand-goals-and-settlement.prd.md#ac-sc31-001) |
| [REQ-SC31-002](industrial-demand-goals-and-settlement.prd.md#req-sc31-002) Trace refs: O7-REQ-3c8dcb96-cb2b-4ec8-824a-5cb50b75f060 | [AC-SC31-002](industrial-demand-goals-and-settlement.prd.md#ac-sc31-002) |
| [REQ-SC31-003](industrial-demand-goals-and-settlement.prd.md#req-sc31-003) Trace refs: O7-REQ-ebb96559-33a9-4029-af82-a709c71225e0 | [AC-SC31-003](industrial-demand-goals-and-settlement.prd.md#ac-sc31-003) |
| [REQ-SC31-004](industrial-demand-goals-and-settlement.prd.md#req-sc31-004) Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3 | [AC-SC31-004](industrial-demand-goals-and-settlement.prd.md#ac-sc31-004) / [AC-SC31-005](industrial-demand-goals-and-settlement.prd.md#ac-sc31-005) |
| [REQ-SC31-005](industrial-demand-goals-and-settlement.prd.md#req-sc31-005) Trace refs: O7-REQ-62129528-2238-4733-8b8e-d226dc40943d | [AC-SC31-006](industrial-demand-goals-and-settlement.prd.md#ac-sc31-006) |
| [REQ-SC31-006](industrial-demand-goals-and-settlement.prd.md#req-sc31-006) Trace refs: O7-REQ-544d08be-9d56-4106-bd80-89c46786bfdf | [AC-SC31-007](industrial-demand-goals-and-settlement.prd.md#ac-sc31-007) |
| [REQ-SC31-007](industrial-demand-goals-and-settlement.prd.md#req-sc31-007) Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd | [AC-SC31-008](industrial-demand-goals-and-settlement.prd.md#ac-sc31-008) |
| [REQ-SC31-008](industrial-demand-goals-and-settlement.prd.md#req-sc31-008) Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3 | [AC-SC31-009](industrial-demand-goals-and-settlement.prd.md#ac-sc31-009) |
| [REQ-SC31-009](industrial-demand-goals-and-settlement.prd.md#req-sc31-009) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | [AC-SC31-010](industrial-demand-goals-and-settlement.prd.md#ac-sc31-010) / [AC-SC31-011](industrial-demand-goals-and-settlement.prd.md#ac-sc31-011) |
