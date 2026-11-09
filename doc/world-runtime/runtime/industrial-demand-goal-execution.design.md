# 工业需求目标、承诺与匹配结算系统设计

- 生命周期：active；本文是待 owner 评审采纳的目标技术设计，不声明实现、正式入口、实玩或发布完成。
- Owner role：runtime_engineer；协作：producer_system_designer、gameplay_designer、M4 工业与物流 owner、World Service、Viewer、Agent、QA。
- 审读基线：nil-studio/oasis7@4c80cd131adb1b3a37323a42bad847c2d7836ce2；Last reviewed：2026-11-06。
- 上游：[工业需求产品 PRD](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#sc31-leaf-requirements)、[需求变化后工作处置](../../game/gameplay/gameplay-industrial-demand-change-disposition-contract.prd.md)、[M4 工业合同](../../world-simulator/m4/industrial-resource-flow-contract.prd.md#4-technical-specifications)。独立验证：[需求目标验收手册](../../testing/manual/industrial-demand-goal-acceptance.manual.md)。

## 1. 问题、目标与非目标

现有 recipe、物流与普通 Goal 各有真实用途，但没有共同拥有 SC-31 目标量、承诺、匹配和补产的生产者。只在 Viewer 展示数量，或把成熟供应设计中“原 SC-31 authority”继续链接回产品文本，仍不能回答谁接受目标、何时占用未满足量、哪个 receipt 只能结算一次以及旧工作怎样停止追加。

本设计建立一个独立的需求执行接缝：唯一目标记录，接受时的有限目标承诺，原生产/物流/终端结果到 matching 的单次关系，以及补产和目标变化后的处置。原物理 writer 继续拥有材料、电力、产出、运输、库存和容量；需求记录不是可花费材料账本。

本批次明确选择普通工业输入交付的目标 profile，并给出 full/reduced、partial matching、surplus hold 和显式补产的技术解释。这是对原未决支持范围的本次设计选择，须随 PR 由相应 owner 采纳；不是 #3650 已批准的历史事实。#3650 完成了文档规范与产品样例，其关闭不代表本合同已决定或实现。已有 native_regional_supply_use_v1 的不可拆分 2 iron 输入目标继续采用其原政策。

不新增配方、价格、产率、费用、维护、损耗、市场预测、订单簿、奖励、自动补货、服务保证或 terminal admission 政策；不扩展 EconomicContract、不选择 SC-32 制度试点。未声明 terminal profile 的物理到达不会变成终端结算。

## 2. 上游约束与相关角色

产品拥有目标身份、玩家承诺与验收；gameplay 拥有支持的选择、机会成本及既有工作处置；M4 与原领域拥有执行单位、资源和有限容量。Runtime 校验这些输入，提交需求状态、关联与 receipts，提供可恢复投影。三类入口只表达同一事实，不能各建目标余额。

### 2.1 需求承接与分配表

| 上游 requirement / product AC（path#fragment） | 独立 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-SC31-001](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-001)；Trace refs: O7-REQ-bfd4b2f4-6b21-4c4a-961d-387b220e762f | 同cut目标、六数量层、资源/容量、机会成本与复查点 | [demand-authority](#demand-authority) | 原目标接受、recipe/物流quote与当前读权 | 不从汇总库存推测需求或维护安全 |
| [AC-SC31-001](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-001)；Trace refs: O7-REQ-bfd4b2f4-6b21-4c4a-961d-387b220e762f | 同cut目标、六数量层、资源/容量、机会成本与复查点 | [demand-preview](#demand-preview) | 原目标接受、recipe/物流quote与当前读权 | 不从汇总库存推测需求或维护安全 |
| [REQ-SC31-002](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-002)；Trace refs: O7-REQ-3c8dcb96-cb2b-4ec8-824a-5cb50b75f060 | 真实支持的full/reduced/准备/hold/stop路径，preview无效果 | [demand-profile](#demand-profile) | gameplay、M4合法quantum和原接受入口 | 不扩finite G/U的partial/生产政策 |
| [AC-SC31-002](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-002)；Trace refs: O7-REQ-3c8dcb96-cb2b-4ec8-824a-5cb50b75f060 | 真实支持的full/reduced/准备/hold/stop路径，preview无效果 | [demand-commit](#demand-commit) | gameplay、M4合法quantum和原接受入口 | 不扩finite G/U的partial/生产政策 |
| [REQ-SC31-003](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-003)；Trace refs: O7-REQ-ebb96559-33a9-4029-af82-a709c71225e0 | production不减需求，只有声明边界matching单次减少 | [demand-matching](#demand-matching) | 原物理/终端receipt、同根publication | 首版无terminal或reward能力 |
| [AC-SC31-003](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-003)；Trace refs: O7-REQ-ebb96559-33a9-4029-af82-a709c71225e0 | production不减需求，只有声明边界matching单次减少 | [demand-matching](#demand-matching) | 原物理/终端receipt、同根publication | 首版无terminal或reward能力 |
| [REQ-SC31-004](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-004)；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3 | 满足后无新sink；盈余及五类旧工作保持可读处置 | [demand-disposition](#demand-disposition) | 原工作图、各bucket处置与容量writer | 不新增drain、退款、销毁或返工权限 |
| [AC-SC31-004](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-004)；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3 | 目标匹配后所有旧schedule/后台/Agent retry均不得新增生产sink，旧工作仍被正确处置 | [demand-disposition](#demand-disposition) | 原工作图、各bucket处置与容量writer | 不新增drain、退款、销毁或返工权限 |
| [AC-SC31-005](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-005)；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3 | 合法batch超过remaining时，surplus实物/容量保留且无自动出售、销毁、伪成交或成长 | [demand-disposition](#demand-disposition) | 原工作图、各bucket处置与容量writer | 不新增drain、退款、销毁或返工权限 |
| [REQ-SC31-005](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-005)；Trace refs: O7-REQ-62129528-2238-4733-8b8e-d226dc40943d | partial后显式一次child或停止，保留baseline/actual/loss | [demand-supplement](#demand-supplement) | 原root/revision、因果分类和fresh资源 | 不自动递归补产，不转移W |
| [AC-SC31-006](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-006)；Trace refs: O7-REQ-62129528-2238-4733-8b8e-d226dc40943d | partial后显式一次child或停止，保留baseline/actual/loss | [demand-supplement](#demand-supplement) | 原root/revision、因果分类和fresh资源 | 不自动递归补产，不转移W |
| [REQ-SC31-006](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-006)；Trace refs: O7-REQ-544d08be-9d56-4106-bd80-89c46786bfdf | 目标/库存/quantum/yield/capacity/current authority漂移重新裁决 | [demand-commit](#demand-commit) | 原canonical order与authority revisions | 本profile无新pending队列或隐式缩量 |
| [AC-SC31-007](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-007)；Trace refs: O7-REQ-544d08be-9d56-4106-bd80-89c46786bfdf | 目标/库存/quantum/yield/capacity/current authority漂移重新裁决 | [demand-recovery](#demand-recovery) | 原canonical order与authority revisions | 本profile无新pending队列或隐式缩量 |
| [REQ-SC31-007](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-007)；Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd | 重复、乱序、恢复、处置和补偿各single effect | [demand-recovery](#demand-recovery) | 原generation、effect index和outbox | timeout不授权新intent；旧局部测试不代签组合 |
| [AC-SC31-008](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-008)；Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd | 重复、乱序、恢复、处置和补偿各single effect | [demand-recovery](#demand-recovery) | 原generation、effect index和outbox | timeout不授权新intent；旧局部测试不代签组合 |
| [REQ-SC31-008](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-008)；Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3 | Viewer/API/Agent同义、当前权限、完整来源与共享预算 | [demand-consumers](#demand-consumers) | World Service、正式三入口与原比较宿主 | 共享DTO/SVG不证明正式入口或玩家体验 |
| [AC-SC31-009](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-009)；Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3 | Viewer/API/Agent同义、当前权限、完整来源与共享预算 | [demand-consumers](#demand-consumers) | World Service、正式三入口与原比较宿主 | 共享DTO/SVG不证明正式入口或玩家体验 |
| [REQ-SC31-004](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-004)；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3 | 满足后无新sink；盈余及五类旧工作保持可读处置 | [demand-preview](#demand-preview) | 原工作图、各bucket处置与容量writer | 不新增drain、退款、销毁或返工权限 |
| [REQ-SC31-007](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-007)；Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd | 重复、乱序、恢复、处置和补偿各single effect | [demand-matching](#demand-matching) | 原generation、effect index和outbox | timeout不授权新intent；旧局部测试不代签组合 |
| [REQ-SC31-008](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-008)；Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3 | Viewer/API/Agent同义、当前权限、完整来源与共享预算 | [demand-budget](#demand-budget) | World Service、正式三入口与原比较宿主 | 共享DTO/SVG不证明正式入口或玩家体验 |

REQ-SC31-009 / AC-SC31-010..011 继续由[跨区发现/交付及 escrow 设计](cross-region-delivery-and-escrow-settlement.design.md#des-wr-cr-001)拥有；本设计只消费其经合同声明的 matching 结果，不再次决定商业最终性。

## 3. 当前状态、目标状态与差距

| 对象 | 固定基线的可复用事实 | 本设计尚须交付 |
| --- | --- | --- |
| 目标 | [普通 Goal](../../../crates/oasis7_agent_api/src/game/goal.rs)表达 Agent 目标；产品已规定 SC-31 | 独立需求 authority、接受/修订/取消、六数量层与目标匹配；普通文字目标不自动迁移 |
| 工业操作 | [工业 admission](../../../crates/oasis7_world_state/src/world_rules/industrial_work_admission.rs)、[工作图目标设计](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-identity)有原生子集与后续合同 | 按实际原 root/operation 关联，跨完整工业/物流/目标事务接通；不声称所有目标 work graph 已实现 |
| 生产与物流 | [recipe reducer](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs)、[transit reducer](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs)拥有真实 effect 与完成去重 | 目标 commitment/matching 与原 writer 同根提交、所有入口的下一 sink gate |
| 入口 | [Viewer E10 设计](../../world-simulator/viewer/viewer-economic-targets-uiux.design.md#t-eco-06)已有需求表现与未知边界 | 正式源、submit/恢复关联和 Viewer/API/Agent 同候选验收；静态图不算当前能力 |
| 成熟供应 | [有限 G/U 设计](mature-world-regional-contribution.design.md#contribution-delivery)已固定 2 iron 的原需求依赖 | 通过同一个目标 producer 接入；其后继使用证明仍由原设计拥有 |

本表为静态源码/设计审读。本轮没有运行 Rust、WASM、World Service、游戏、模型、真人或故障注入测试；历史产品实现核查保持原版本。

## 4. 边界与结构

<a id="demand-authority"></a>
### 4.1 唯一目标生产者、身份与数量

IndustrialDemandAuthority 接受有权 recipient 的显式目标请求；这是本设计要交付的 producer，不是假定已有的 API。普通输入目标绑定同一 world/branch、当前 recipient/control generation、确切 input Site、material 与单位、已有工业用途 intent 的 input obligation、目标数量、适用窗口和 matching boundary。用途 intent 的原 owner/输入要求是来源；同一 input obligation 已接受的目标和可复用库存分配不得超过其真实缺口。没有真实用途或当前接收权限时不创建目标。新 goal 不自动接受生产、运输或 recipient 后续制造。

goal_authority_ref 含 authority domain/instance、goal id、immutable profile/version、当前 revision 和内容 digest。目标接受、数量/窗口修订、暂停或取消仅由原 recipient 或正式被授权主体提交；供应方只可确认自己承担的计划，不能写目标量或用途。用途、recipient、material/unit、destination 或满足边界改变时建立明确关联的新目标并处置旧工作；不得原地改写已匹配的历史。数量-only 变更采用新 goal revision，保留原累计结果。

| 数量 | 唯一来源与解释 |
| --- | --- |
| target T | 当前受权 goal revision；非负整数和原 material unit |
| committed | 同时返回累计已接受量和仍待处置的 committed_unsettled C；决策只用 C，不能把已匹配量再次算承诺 |
| production_settled P | 该计划原生产 receipt 的实际主产物量；副产物单列，旧库存输入不能伪称新生产 |
| delivery_settled D | 关联原物理 delivery receipt 的实际入账量；其中 matched/unmatched 分列 |
| terminal_settled X | 原 terminal authority 的实际结果；delivery-only profile 明确 not_applicable，不填成“已终端完成” |
| fulfilled F / remaining R | F 只累加目标指定边界的 matching quantity；R=max(T−F,0)，不将 D 与 X 相加 |

新承诺可用量 A=max(R−C,0)。所有运算使用 checked integer；未知、单位不合或溢出返回 unknown/blocked。目标缩减可使历史 F>T；F 不回减、不退款，单列缩减后的历史超额，R 为零。P、D、X 是不同事实投影，不能相加为总进度；同一切点实物位置另按原工作图互斥 bucket 表示。

目标是目的与数量的权威，材料仍在原 ledger。C 只占用本目标尚未满足的量，不创建可花费库存、物流/终端 capacity reservation、未来供给保证或受益人制造权。原物理资源未被真实预留时始终标为 boundary_revalidated/non_reserved。

<a id="demand-profile"></a>
### 4.2 本批次目标支持矩阵

目标 profile 名 industrial_input_delivery_v1 仅标识本次待采纳的技术选择。原生配方及原 ScheduleRecipe/TransferMaterial 是其唯一业务执行者；同一目标由真实输入用途接受，匹配边界是明确目的 Site 的原物理交付与受权接收共同成立。无金钱对价、交付奖励、认可或终端价值承诺。支持任意正整数的合法材料交付量；生产仍只能是 canonical recipe 的正整数 batches，不允许半个 recipe cycle。

| 选择/事件 | industrial_input_delivery_v1 | 已有 native_regional_supply_use_v1 适配 |
| --- | --- | --- |
| produce_full | 使用当前合法整数 batch；最多承诺 A，超过目标的合法产物在接受前列为 surplus；所有主/副产物去向须有原 authority | 不新增生产；仅原已接受的 2 iron 旧库存供应 |
| produce_reduced | 用户显式选较少但正的合法整数 batches；保留 shortage，不自动缩请求 | 不拆分 2 iron；不借本设计开放 reduced |
| replenish_or_transfer | 指向真实原补料/补电/调运选项，各自独立确认；未满足前不执行生产 | 维持原供应/路线/费用和 G/U 条件 |
| partial matching | 仅记录实际且满足该目标条件的整数数量，剩余缺口可见 | 维持原不可拆分 2 iron 条件；不推广为部分用途 |
| hold_surplus | 保留原合法账本与实际容量占用；只读“持有”不新增 material effect | 原剩余 iron 仍是 A 的普通资产 |
| stop/defer | 关闭本次计划追加许可，保留旧工作和缺口；恢复须 fresh 显式确认 | 已匹配 G 不自动续供；新需求按原政策显式接受 |
| supplemental_plan | 同目的、同候选、剩余缺口经 fresh 检查后一次 parent-linked revision | 本适配不为已完成 G 自动补产；原政策未支持的选项不可用 |
| cancel/expiry/reduce | 关闭旧准入，固定完整旧工作处置；数量-only 修订不撤销历史 | 原窗口、goal state 与不可拆分约束不变 |
| terminal / rewards / paid escrow | 不支持；仅保留通用适配合同与原来源引用 | 不支持，不继承其他制度效果 |

full 是用户提交的完整合法候选，不是系统承诺自动找到最优批数。预览可展示原 recipe authority 给出的 full/reduced 候选，但不进行客户端取整、隐藏降低数量或以损耗预测自动追单。所选量、合法量与目标分配量分别保存。

## 5. 关键运行流程

<a id="demand-preview"></a>
### 5.1 同一切点的预览与正常数量例

读取者在同一 committed cut 取得目标/current revision、用途授权、全部既有目标承诺、原 recipe candidate/quantum、投入/电力、真实物流/库存/容量、各 receipt 和当前 audience。得到的候选保存 source cut、各 authority revisions、可执行量、目标分配量、预计 shortage/matched/surplus、成本来源、不可逆边界和 next_recheck。不同切点的事实不得拼装；分页不完整或未知 writer 使相关比较 unknown，不只取前几项求和。

每项机会成本用实际物理量说明：full 多占用哪些原料/电力/产物与副产物空间，reduced 留下多少缺口，持有继续占用哪个 ledger，延期须等什么 authority/资源事件。未发布的价格、维护、损耗、价值和容量不补零；既有原契约明确不适用时才标 not_applicable。next_recheck 是原逻辑时间或具体依赖事件，不承诺未预留 ETA。

下表是依据[原生 recipe catalog](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)的静态计划算例，不是运行或余额报告。每个 iron batch 的业务配方为 4 iron_ore + 1 carbon_fuel → 3 iron_ingot + 1 slag，原 recipe power 为 8E。G 接收方都有真实 2 batches gear 用途（总需 4 iron）；其已明确分配的合法既有 input iron 分别为 2、1、0，因此外部需求分别为 2、3、4。用途接受不自动开始 gear。每个例子是独立世界切片，原工厂、输入、主副产物去向、有限容量、产品验证和物流准入须全部有效。

| 原需求 R，C=0 | 玩家显式选择 | 计划业务投入/产物 | 目标分配与真实下一步 |
| --- | --- | --- | --- |
| 2，小于一个 quantum | full 1 batch | ore4/fuel1、8E；iron3/slag1 | C=2；produced3 不减少 R；独立交付2匹配后 F=2/R=0，iron1 仍持有 |
| 3，等于一个 quantum | full 1 batch | ore4/fuel1、8E；iron3/slag1 | C=3；交付3匹配后 F=3/R=0，无 iron surplus，slag 仍单列 |
| 4，大于一个 quantum | full 2 batches | ore8/fuel2、16E；iron6/slag2 | C=4；交付4匹配后 F=4/R=0，iron2 与 slag2 留在原合法账本 |
| 4，大于一个 quantum | reduced 1 batch | ore4/fuel1、8E；iron3/slag1 | C=3；交付3匹配后 F=3/R=1；显式补产或 stop/defer，不能报已完成 |

8E/16E 只是原 recipe 主业务 sink；实际 validator/module owner 费用、维护、物流 tariff、资源背景 delta 仍由原来源分别加入，不可当作完整总成本或最终余额。数值例未设新价格、零损耗、免费验证或无限存储。若真实物流发生损耗，只按原 received/loss receipt 对账，不能仍用表中的无损量宣称匹配。

preview 不创建 goal、intent、root、revision、material/goal hold、queue、顺位、模块调用费用或时间推进。正常路径的可用结论需要所有该候选适用前置；缺条件时保留可读已知量与具体 blocker。

<a id="demand-commit"></a>
### 5.2 fresh 接受、并发和下一 sink

1. 正式提交解析当前用户/Agent 授权、world、原 request key 和 payload digest；同 key 同 payload 先返回原 disposition，改 payload 为 idempotency_conflict。preview 没有 root；只在 authoritative accepted outcome 创建或绑定真实 root/revision 与 immutable baseline。
2. 在原执行事务内重读 goal/profile、用途、authority revisions、R/C、工厂/配方、材料、电力、产品/副产物去向、物流及容量。若 target/quantum/yield/权限等改变，返回 requote 或无新业务效果拒绝，不静默更换候选、缩批或目的地。本 profile 不使用新 pending 队列；旧物理工作的 pending 保持原合同。
3. goal allocation q 必须为正且 q≤A，planned output 必须覆盖 q；完整输出大于 q 的 surplus 在接受前披露并有合法去向。原 production 或 supply 接受与新 allocation、baseline、原 receipt/operation 绑定在同一 canonical transaction 提交，失败不留下单独 C 或半个物理 sink。仅延期/取消比较不创建 allocation。
4. 两个供应者或两个 goal 争用同一资源，由原 canonical order/allocator 与 expected revisions 决定；同 goal 竞争同一 A 的第二提交必须重算或拒绝。不同 goal 仍竞争同一原材料/电力/route，C 不能替代资源预留；禁止按入口、重试或新别名刷新顺位。
5. 每个尚未发生的不可逆生产/运输 sink 重验该 allocation 仍有效、目标未满足/取消/过期、当前 purpose 与原执行权限；这层 gate 必须覆盖原生、模块、direct/trusted、tick/后台、Agent 等所有可写入口。原目标已 matched 时旧 schedule 不可开新生产；已有 sink 的 WIP 按 §5.4 保留，不倒扣历史。

goal 的接受不是生产接受；recipe 的接受也不是后续 TransferMaterial 的隐含授权。玩家依原能力显式确认运输；有预先合法授权的受限执行计划也须逐边界重验，不能由展示/推荐补出代办权。

<a id="demand-matching"></a>
### 5.3 原物理结果与唯一 matching

生产完成先由原 writer 记实际 output/byproduct、loss 和原 production receipt，仅更新 P。供应开始/完成由原物流 writer 记源扣减、在途、实际入账、费用和容量释放；这些效果不在需求 authority 重做。原 receipt 必须绑定受接受的 supply operation、goal allocation、准确 material/unit、source/destination、recipient、world/branch、当时授权与窗口。

匹配 adapter 在原物理结算事务内校验：目标当前有效；原接受绑定成立；声明的满足边界已发生；实际收货与目的、资格、窗口相符；allocation 尚未结算；收到数量真实可归属。只有匹配部分 m 更新 F，且 m≤实际合法 received、m≤该 allocation 未处置量、m≤当前 R。由确定的原 receipt 和 policy 决定 m，不按重试次数或旧 expected output。ordinary profile 支持显式 partial；finite G/U 仍采用自己的不可拆分规则。

这次原 delivery 已最终处置的 allocation 同时从 C 释放；已匹配量、实际损耗、未匹配交付和剩余 shortage 分别保留。存在原 profile 支持的后续未到达分支时，只关闭该已终结 branch 的 allocation portion；不能释放其他 pending claim。普通首版每次 transfer 只绑定一个 goal，不将同一收货 receipt 分给多个目标。

稳定 effect key 由 world/原 settlement receipt/material/原 allocation role 构成，不含新请求键、报告名或后来 revision。原 settlement 还维护跨目标的已归属数量/唯一绑定索引，防止用新 goal 或新 allocation 再消费同一 receipt。delivery-boundary 已匹配的量不能在后续 terminal receipt 再加一次 F；terminal-boundary profile 只有其正式 terminal receipt 可匹配，arrival 只更新原物理事实。

目标 cancelled/expired/unknown、wrong recipient/destination 或非 matching receipt 不减少 F、不发交付奖励。已合法的物理完成不能因目标失败被销毁或自动退回：保存原 received 及明确 unmatched disposition，C 依该操作终态结清；后续重试只重读，不把后来新目标倒签到旧收货。目标自身的单次错误更正只能走原受权 linked correction，保留旧结果和来源，不能重跑物流或奖励。

本 profile 无 reward writer。未来确有独立已批准奖励时，该 authority 只能消费一次原 matching receipt，并以自己的稳定 effect role 去重；本合同不创建其准入、数值或发奖能力。

<a id="demand-disposition"></a>
### 5.4 满足、缩减、取消和五类旧工作

goal matched/cancelled/expired、数量缩减或 authority 不可判定时，先阻止该目标的新准入；不能等待遍历完历史才开放下一 sink。受权变更基于完整 active-allocation/index closure 固定受影响集合与处置 revision，完成与变更竞态由 canonical order 决定。超预算时保留安全 admission fence 和既有义务，处置按已登记独立 root 继续；不得漏项、部分删账或声称全部已停止。

| 原 bucket | 普通输入交付首版的安全路径 | 保留与拒绝边界 |
| --- | --- | --- |
| accepted-unstarted | stop/defer 关闭追加许可；原已支持的 release 只释放真实未消费 hold | 目标 C 与物理 hold 分别结清；无 sink 不产生退款；没有底层取消能力不伪造取消 receipt |
| active WIP | 已承诺的原原子 recipe 按原合同完成到合法输出或保持原 blocked 状态，不开下一 batch | 原 sink/损耗/lineage 留存；完成不再满足取消/过期目标；不凭需求标志新增 pause、salvage、abandon 或销毁 |
| in-transit | 原合法运输按原权限到达/等待；匹配重新检查当前 goal | 保留费用、损耗、目的地和物流 obligation；未支持 return/reroute 不列可选 |
| buffer-held | 保留原合法 output 与容量；显式新用途须 fresh 验证和新的合法后续操作 | 不自动出售、销毁、改所有权或将旧 receipt 匹配新目标 |
| terminal-pending | 由[原 terminal/disposition authority](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-backpressure-terminal)处理，首版不新建该 bucket | 原 recipient、destination、admission、capacity obligation、provenance 全保留；不得因新 profile 不支持而丢弃既有终端义务 |

缩减 action 须预览并确认新 T、当前 F/C、受影响工作和 residual。已开始的产物不会自动缩小，partial matching 只按本 profile 与新受权 target 处理；finite G/U 不支持的缩量要先取消/处置旧 G 后建立新的合法目标。read source unavailable 只降低读结论，不能由客户端写成 cancelled/expired；权威缺失时对应新 sink fail closed。

持有 surplus 是保留原资产，不是第二次 credit 或永久免成本的仓储承诺。原容量、维护/衰减和继续合法性照常适用。所有新目的须显式接受；旧 F、receipt、奖励、W、顺位和 reservation 不迁移。未知恢复为有效或目标扩大只触发 fresh 比较，不能自动恢复旧计划。

<a id="demand-supplement"></a>
### 5.5 partial 后的补产与停止

补产 eligibility 从原最终 matching disposition 和当前 R/C 推导。前一承诺尚有未解释 pending 量时，不把它当损失补一份；先保留 C 或完成原受权处置。supplemental decision 绑定 goal、已读 parent revision、原 matching/shortage cut、请求 key 与明确新计划。同一次确认以此身份 CAS 接受一次 child revision；不同 key 在相同 parent 上并发也只能有一个 child，失败方得到已存在的 child/ref 并重新比较。

数量-only 且证明仍为同用途/工厂/recipe/path/输出因果时，沿原 root 建立 parent-linked revision，原 baseline、actual、loss、F 和已关闭义务永不改写。用途、recipient、recipe/factory/path/terminal 或其他因果变化依[原工作图 cutover](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-cutover)创建链接的新 candidate/root，W=0；补产不能作为绕过因果分类的名称。

R=1 的正常例可显式选新 1 batch iron，原配方产生3，仅再分配并交付1，其他2为披露的 surplus；其原费用/验证/容量重新检查。只在真实匹配后 R=0，旧 revision 不重计产出或交付。若新一轮仍有真实 shortage，必须在新结果上再明确选择；每一轮至多一个 child，不能把“一次补产”实现成后台递归循环。

stop/defer 写原计划的单次选择/准入状态，保留 R、P/F、损耗、当前 bucket 和 next_recheck；不创建 supplemental revision、生产 sink、虚假取消 refund 或“已完成”状态。

## 6. 接口与数据合同

以下为目标逻辑记录和语义操作名，不宣称已有同名 endpoint、Rust 类型或可调用 tool。版本化编码/namespace 仍由原 Runtime/World Service/ABI 发布。

| 记录/接口 | 必需绑定与输出 | 生产者与消费者 |
| --- | --- | --- |
| DemandGoalV1 / AcceptDemandGoal | 原用途/input obligation、authority/recipient/control、material/unit、T、窗口、profile/boundary、revision/digest、接受 receipt | 受权 recipient 请求；IndustrialDemandAuthority 校验并持久化；三入口只读 |
| DemandAllocationV1 / CommitDemandPlan | 原 root/revision、goal revision、候选/计划 digest、requested/admissible/output/q、原 operation、baseline、当前 disposition、未处置量 | 原生产/供应接受事务共同写；R/C 投影消费 |
| DemandMatchV1 / ApplyDemandMatch | 原 settlement、allocation、匹配/未匹配量、原因、source cut/current goal revision、单次键 | 原物理/终端 writer 同事务调用 adapter；其余入口无直接写权 |
| DemandDispositionV1 / ChangeDemandGoal | 受权目标变化、完整 active set、admission fence、逐 bucket 原 obligation/所选处置/执行 receipt | 原目标与工作图事务；之后每个原 operation 自己完成处置 |
| DemandSupplementV1 / ConfirmDemandSupplement | 唯一 parent、shortage cut、fresh plan、child 与接受结果 | 原工业接受接口；同 parent 并发 CAS |
| DemandProjectionV1 / ReadDemandGoal | 同 cut/version、T/累计 committed/C/P/D/X/F/R、互斥 buckets、surplus/shortage、真实可用 actions、primary blocker、cost refs、next_recheck、audience、coverage | 授权 scoped reader；Viewer/API/Agent 使用同一投影 |

错误至少区分 unknown_authority、not_authorized、unsupported_profile_mode、stale_goal、stale_plan、goal_closed、insufficient_uncommitted_demand、missing_receipt、non_matching、capacity_unavailable、idempotency_conflict、budget_exceeded 和 commit_unknown。每个错误携带有权可读的原 identity、已知效果/仍占用和真实下一步；不泄露其他供应者私密 payload 或排队位置。

<a id="demand-recovery"></a>
## 7. 状态、事务与持久化恢复

goal 的业务生命周期为 active、deferred、matched、cancelled、expired；reduced 是带新 T 的受权变更，不是额外完成态；unknown/not_tracked 是读/authority 判定，不能伪造业务终态。plan 独立经历 accepted、各原工作 bucket、matched/partial/unmatched、stopped/closed；goal matched 不代表全部物理义务都消失。

所有写入使用[统一 ExecutionTransaction](../design.md#62-unified-executiontransaction-architecture-p0)的目标合同：goal/allocation/match/index、触及的原资源/工作/receipt、journal、sequence 和 durable publication 同一提交点。原物理 writer 接缝仍未完成统一接入时不得启用此 profile；不能在 append 成功后另开一个可能失败的目标副事务。目标与用途只存一份 canonical 状态，缓存/Viewer/task text 不可参与裁决。

接收方撤回权限、goal expiry、生产/到达与目标修改按原 canonical world-time/order 串行判定；不使用客户端 wall clock。即使 admission fence 与后续长时处置分步，所有 obligation 均在 fence 提交时登记并保持可查询，每次后续效果仍使用原 operation 和处置键。显式修改尚未终结处置必须链接新 disposition revision，不重复释放已结清量。

snapshot/generation 包含目标/用途绑定、全部 live allocation、唯一 matching/source-consumption 索引、parent-child CAS、admission fences、bucket dispositions、receipt refs、权限历史及已终结去重 tombstones；journal 有确定版本的事件和 delta。导入先验证闭包、单位/数量守恒、F 来源、C 与 active set 一致、parent 唯一、settlement 单次及完整 schema；任何半个 generation 或重复归属拒绝安装，不用当前余额猜历史。

replay 仅还原原已提交 delta/决定，不调用模块、不重报价、不新匹配、不延窗口、不再执行物流或费用。snapshot 与非空 journal 后得到同一 world root、目标量和原 receipts。commit_unknown 必须按原 intent/receipt/generation 查询；重连不得以新 key 盲重试。发布失败使用原 outbox/receipt 恢复；已经 committed 的 matching 不因 UI 超时撤销或重发。

根发布遵守原[G1 提交判定](runtime-storage-footprint-governance.design.md#storage-generation-g1-design)与[普通 generation 保存合同](../design.md#runtime-generation-persistence-design)：只有已证 NotCommitted 才丢弃该候选并继续旧 World；Committed 安装同一完整候选/receipt，提交后维护错误不能倒回旧状态；StatusUnknown 隔离整个旧 World 的 writer 与 tick，保留原 operation、候选、root/outbox 和查询线索，核为 Committed 或已证 NotCommitted 后才解除。不能将全根未知降为某 goal 的局部 blocked 后继续其他写入；网络/ACK/查询超时不证明未提交。局部已知缺源或 profile 不支持与全根提交未知必须分开。

物理补偿由原领域授权并产生 linked compensation；需求只消费其明示目标更正合同，不默认减 F、退款、重新开单或恢复进度。缺该专业政策时保留原历史和争议/unknown，不虚构补偿效果。

<a id="demand-consumers"></a>
## 8. 正式入口、权限与运行约束

World Service 提供同源的 scoped goal/quote/result 读取和原 intent 提交 adapter。pure API 依正式 API reference/capability 发现调用；Agent 的正式 game skill 只使用同一支持集和 request identity；Viewer 接入已有 [E10 demand goal](../../world-simulator/viewer/viewer-economic-targets-uiux.design.md#t-eco-06)，展示分层数量、成本、blocker 和显式确认。实现时必须逐入口交付，不能先将未注册命令写入按钮或 prompt。

投影将数量 disposition（shortage/matched/surplus/unknown）与 goal state 分列；produced/undelivered、partial matching、surplus held、stop/defer 和 closed-goal retry 均有同义结果。用户已确认的选择与 Agent 推荐分开；切换入口、刷新或重连不建立新业务意图。

当前读权按 recipient/供方实际 scope 最小披露：供应者可读其承担的目标条件、自己成本和匹配结果，不据此读受益人的其他私人目标/预算或其他竞争者身份。历史归因固定原控制事实，当前查询/操作重新校验现 owner/delegation；接管/退出后不继承旧提交权限。私有来源缺失保持不可读取，不用缓存回填。

目标记录与适配按[原 native 分类](../prd.md#native-surface-分类与冻结规则)进入原工业执行域；这是既有 SC-31 的目的/数量/receipt 接缝，不增加制度裁决或绕过 Kernel。未来带付费/税费/认可以及制度命令的 goal profile 必须沿原分类和 module instance 合同独立设计，不能继承本 profile 名义继续扩展 native 制度 schema。

<a id="demand-budget"></a>
## 9. 质量、容量与完整性

比较读取复用[原生产恢复比较宿主预算](production-recovery-comparison.design.md#production-recovery-budget)：最多3候选、每候选最多6原动作和3个明确完成/等待边界、最多4补料kind，同请求32KiB、256对象、原始与解码 source 合计4MiB、单对象含解码≤256KiB、10批读取（每批≤64对象/512KiB）、1024次索引/proof访问、最多32次有界原规则求值、响应256KiB、overlay512KiB；单槽≤6MiB，同宿主4槽/24MiB、每主体1槽、无内部等待队列和3秒取消边界。SC-31、成熟供应与原比较共同计数，不能各开满预算子查询再只计摘要。工程上限不是实测时延承诺。

完整目标数量使用同事务维护的 exact-key F/C 汇总与 source coverage，不逐次扫描无限历史。逐工作显示可稳定分页，但不完整页面不得用于变更/匹配或“全部已停止”结论。当前目标的 active set/原义务超一次事务预算时，先持久 admission fence，再在登记集合上逐独立原 operation 处理，续页绑定同一 closure revision；并发状态改变须重新定位，不漏已完成或后来合法处置。

写入沿原工业域 record/bytes/receipt 与原 command 预算，不为新目标增开材料、容量或执行池。达到上限拒绝新 goal/plan，已接受义务和去重记录不得驱逐；live obligation 的结清与原恢复继续使用保留的运行能力。部署前必须量测一正常目标、full/reduced、补产及五类旧工作在限内可完成；不支持的长事务不能靠截断集合通过。

| 质量场景 | 判定指标 | 方法与当前限制 |
| --- | --- | --- |
| 数量/并发 | F 单次、C 等于尚未处置量，同资源不超承诺或物理余额 | required 并发两个计划及不同 goal；未运行 |
| 正常可完成 | 同输入 cut 支持低于/等于/高于 quantum 的明确路径，补产正常返回 | [DG-M01..05](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m01)；不能只有 unknown/deny |
| 有界读取/写入 | limit 与 limit+1、超大单记录、嵌套读取、当前权限、取消 | [DG-M08](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m08)；量测实际峰值，不承诺未测吞吐 |
| 恢复/持久 | 崩溃前后同 goal/receipt/world root，无孤立 allocation 和重复 effect | [DG-M07](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m07)；原子底座子测试不代签新链 |

## 10. 兼容、迁移与回滚

旧文本 Goal、普通 Task/recipe、legacy 物流继续原语义，不因缺新字段追溯加入新 goal gate。新 profile 必须有显式 activation/schema boundary，producer、所有 writer、持久/replay reader 与实际入口同时支持后才可声明启用。已有有限 G/U 接缝按其原2 iron政策接入同一个 producer，不另存 RegionalDemand。

旧 job/库存只有在新目标允许其来源、真实当前权属/用途/数量可证且发生新的显式接受时才能成为合法供给；不能给过去任意 receipt 补 goal 身份。新 schema 启用后的降级必须保留其完整 reader/reducer/义务和去重数据；旧 binary 无法理解则停止新写入，不能删字段或恢复旧快照来撤销已发生交付。

## 11. 验证设计与可追溯性

验证按[测试手册](../../../testing-manual.md)与原产品 required/full 执行。专用手册定义目标方法，没有可调用的新测试命令或已通过声明；实施 PR 应提供同一 candidate/profile/authority 与真实原入口的证据。

### 11.1 验证映射表

| 上游 requirement / product AC（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-SC31-001](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-001)；Trace refs: O7-REQ-bfd4b2f4-6b21-4c4a-961d-387b220e762f | [demand-authority](#demand-authority) | 同cut目标、六数量层、资源/容量、机会成本与复查点 | [DG-M01](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m01)：三个量子关系、真实原生来源、缺源/溢出与只读前后根；required contract/readonly；固定同一commit/profile/authority与实际原入口 | source cut、原quote、分层数量与无效果根 | 不从汇总库存推测需求或维护安全 |
| [AC-SC31-001](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-001)；Trace refs: O7-REQ-bfd4b2f4-6b21-4c4a-961d-387b220e762f | [demand-preview](#demand-preview) | 同cut目标、六数量层、资源/容量、机会成本与复查点 | [DG-M01](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m01)：三个量子关系、真实原生来源、缺源/溢出与只读前后根；required contract/readonly；固定同一commit/profile/authority与实际原入口 | source cut、原quote、分层数量与无效果根 | 不从汇总库存推测需求或维护安全 |
| [REQ-SC31-002](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-002)；Trace refs: O7-REQ-3c8dcb96-cb2b-4ec8-824a-5cb50b75f060 | [demand-profile](#demand-profile) | 真实支持的full/reduced/准备/hold/stop路径，preview无效果 | [DG-M02](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m02)：普通与finite G/U各自支持集、同/异goal争用和显式接受；required command；固定同一commit/profile/authority与实际原入口 | 能力集合、原选择/接受/拒绝、q/C及实际资源账 | 不扩finite G/U的partial/生产政策 |
| [AC-SC31-002](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-002)；Trace refs: O7-REQ-3c8dcb96-cb2b-4ec8-824a-5cb50b75f060 | [demand-commit](#demand-commit) | 真实支持的full/reduced/准备/hold/stop路径，preview无效果 | [DG-M02](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m02)：普通与finite G/U各自支持集、同/异goal争用和显式接受；required command；固定同一commit/profile/authority与实际原入口 | 能力集合、原选择/接受/拒绝、q/C及实际资源账 | 不扩finite G/U的partial/生产政策 |
| [REQ-SC31-003](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-003)；Trace refs: O7-REQ-ebb96559-33a9-4029-af82-a709c71225e0 | [demand-matching](#demand-matching) | production不减需求，只有声明边界matching单次减少 | [DG-M03](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m03)：原生产/物流正例、非matching/跨goal重用、声明terminal的独立对照；required settlement/full分支；固定同一commit/profile/authority与实际原入口 | P/D/X/F/C/R、原receipt和跨目标归属键 | 首版无terminal或reward能力 |
| [AC-SC31-003](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-003)；Trace refs: O7-REQ-ebb96559-33a9-4029-af82-a709c71225e0 | [demand-matching](#demand-matching) | production不减需求，只有声明边界matching单次减少 | [DG-M03](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m03)：原生产/物流正例、非matching/跨goal重用、声明terminal的独立对照；required settlement/full分支；固定同一commit/profile/authority与实际原入口 | P/D/X/F/C/R、原receipt和跨目标归属键 | 首版无terminal或reward能力 |
| [REQ-SC31-004](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-004)；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3 | [demand-disposition](#demand-disposition) | 满足后无新sink；盈余及五类旧工作保持可读处置 | [DG-M04](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m04)：五状态×五bucket、全部实际可写入口、surplus原账和容量漂移；required lifecycle/full race；固定同一commit/profile/authority与实际原入口 | admission fence、完整closure、原处置/占用与surplus | 不新增drain、退款、销毁或返工权限 |
| [AC-SC31-004](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-004)；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3 | [demand-disposition](#demand-disposition) | 目标匹配后所有旧schedule/后台/Agent retry均不得新增生产sink，旧工作仍被正确处置 | [DG-M04](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m04)：五状态×五bucket、全部实际可写入口、surplus原账和容量漂移；required lifecycle/full race；固定同一commit/profile/authority与实际原入口 | admission fence、完整closure、原处置/占用与surplus | 不新增drain、退款、销毁或返工权限 |
| [AC-SC31-005](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-005)；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3 | [demand-disposition](#demand-disposition) | 合法batch超过remaining时，surplus实物/容量保留且无自动出售、销毁、伪成交或成长 | [DG-M04](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m04)：五状态×五bucket、全部实际可写入口、surplus原账和容量漂移；required lifecycle/full race；固定同一commit/profile/authority与实际原入口 | admission fence、完整closure、原处置/占用与surplus | 不新增drain、退款、销毁或返工权限 |
| [REQ-SC31-005](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-005)；Trace refs: O7-REQ-62129528-2238-4733-8b8e-d226dc40943d | [demand-supplement](#demand-supplement) | partial后显式一次child或停止，保留baseline/actual/loss | [DG-M05](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m05)：G4 reduced3后补1或停止；同parent并发及3轮显式链；required/full revision；固定同一commit/profile/authority与实际原入口 | parent/child、原结果、F/C/R和显式选择 | 不自动递归补产，不转移W |
| [AC-SC31-006](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-006)；Trace refs: O7-REQ-62129528-2238-4733-8b8e-d226dc40943d | [demand-supplement](#demand-supplement) | partial后显式一次child或停止，保留baseline/actual/loss | [DG-M05](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m05)：G4 reduced3后补1或停止；同parent并发及3轮显式链；required/full revision；固定同一commit/profile/authority与实际原入口 | parent/child、原结果、F/C/R和显式选择 | 不自动递归补产，不转移W |
| [REQ-SC31-006](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-006)；Trace refs: O7-REQ-544d08be-9d56-4106-bd80-89c46786bfdf | [demand-commit](#demand-commit) | 目标/库存/quantum/yield/capacity/current authority漂移重新裁决 | [DG-M06](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m06)：逐一漂移及sink/arrival竞态；required/full multi-goal；固定同一commit/profile/authority与实际原入口 | fresh revisions、requote/reject、原世界与未决义务 | 本profile无新pending队列或隐式缩量 |
| [AC-SC31-007](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-007)；Trace refs: O7-REQ-544d08be-9d56-4106-bd80-89c46786bfdf | [demand-recovery](#demand-recovery) | 目标/库存/quantum/yield/capacity/current authority漂移重新裁决 | [DG-M06](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m06)：逐一漂移及sink/arrival竞态；required/full multi-goal；固定同一commit/profile/authority与实际原入口 | fresh revisions、requote/reject、原世界与未决义务 | 本profile无新pending队列或隐式缩量 |
| [REQ-SC31-007](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-007)；Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd | [demand-recovery](#demand-recovery) | 重复、乱序、恢复、处置和补偿各single effect | [DG-M07](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m07)：各commit故障点、snapshot+非空journal、同/异key及linked补偿；required/full；固定同一commit/profile/authority与实际原入口 | intent/result、effect index、generation/world root及原账闭包 | timeout不授权新intent；旧局部测试不代签组合 |
| [AC-SC31-008](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-008)；Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd | [demand-recovery](#demand-recovery) | 重复、乱序、恢复、处置和补偿各single effect | [DG-M07](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m07)：各commit故障点、snapshot+非空journal、同/异key及linked补偿；required/full；固定同一commit/profile/authority与实际原入口 | intent/result、effect index、generation/world root及原账闭包 | timeout不授权新intent；旧局部测试不代签组合 |
| [REQ-SC31-008](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-008)；Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3 | [demand-consumers](#demand-consumers) | Viewer/API/Agent同义、当前权限、完整来源与共享预算 | [DG-M08](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m08)：同candidate真实三入口、authority切换、limit/limit+1与资源量测；required/full parity；固定同一commit/profile/authority与实际原入口 | 真实请求/响应/cut、界面/Skill轨迹、bytes/reads与原结果 | 共享DTO/SVG不证明正式入口或玩家体验 |
| [AC-SC31-009](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-009)；Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3 | [demand-consumers](#demand-consumers) | Viewer/API/Agent同义、当前权限、完整来源与共享预算 | [DG-M08](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m08)：同candidate真实三入口、authority切换、limit/limit+1与资源量测；required/full parity；固定同一commit/profile/authority与实际原入口 | 真实请求/响应/cut、界面/Skill轨迹、bytes/reads与原结果 | 共享DTO/SVG不证明正式入口或玩家体验 |
| [REQ-SC31-004](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-004)；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3 | [demand-preview](#demand-preview) | 满足后无新sink；盈余及五类旧工作保持可读处置 | [DG-M04](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m04)：五状态×五bucket、全部实际可写入口、surplus原账和容量漂移；required lifecycle/full race；固定同一commit/profile/authority与实际原入口 | admission fence、完整closure、原处置/占用与surplus | 不新增drain、退款、销毁或返工权限 |
| [REQ-SC31-007](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-007)；Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd | [demand-matching](#demand-matching) | 重复、乱序、恢复、处置和补偿各single effect | [DG-M07](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m07)：各commit故障点、snapshot+非空journal、同/异key及linked补偿；required/full；固定同一commit/profile/authority与实际原入口 | intent/result、effect index、generation/world root及原账闭包 | timeout不授权新intent；旧局部测试不代签组合 |
| [REQ-SC31-008](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-008)；Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3 | [demand-budget](#demand-budget) | Viewer/API/Agent同义、当前权限、完整来源与共享预算 | [DG-M08](../../testing/manual/industrial-demand-goal-acceptance.manual.md#dg-m08)：同candidate真实三入口、authority切换、limit/limit+1与资源量测；required/full parity；固定同一commit/profile/authority与实际原入口 | 真实请求/响应/cut、界面/Skill轨迹、bytes/reads与原结果 | 共享DTO/SVG不证明正式入口或玩家体验 |

## 12. 设计取舍、风险与后续触发

本批次选择输入交付而非自行创建新的终端市场，因其可直接复用已采用的目标/原生配方/物流与有限 G/U 依赖。完整合法 batch 和目标分配量分离，保留 surplus 与机会成本；目标 commit 数量不是物理 hold，避免复制可花费资产。匹配随原 writer 原子提交，防止“货已到而目标永远未记”或目标已减少但物理交易回滚；代价是所有实际写入口和恢复 generation 必须共同接通。

本设计采用的普通输入 profile、逻辑 producer、匹配/补产和消费者合同须经 owner 随 PR 采纳。其他 profile 的量化/终端/奖励/服务窗口与制度政策仍需各自原 authority 决定，不能通过新 profile 名隐含启用。本范围没有把未来 terminal 支持当成现成能力，也没有用其尚未实施阻断现有输入交付设计。

实施前尚须完成：正式编码与 capability 发布、所有原 writer/目标索引共同事务、同 generation 恢复、三入口接线及上述独立验收；这些是实现与验证交付，不能以本设计存在作完成证明。新增 profile、用途/满足边界、quantity policy、writer/authority 或 ABI 变化触发相应条款和验证复核，不改变既有历史量与产品验收范围。
