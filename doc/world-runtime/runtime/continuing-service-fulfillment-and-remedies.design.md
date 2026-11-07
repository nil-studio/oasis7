# 持续服务履约、责任与有限救济系统设计

- 生命周期：active（目标技术合同；不表示代码、治理激活或运行验证完成）。
- Owner role：runtime_engineer；gameplay_designer 拥有服务政策，Agent/Viewer 消费结果，治理/P2P 拥有真实身份、权限和随机来源，qa_engineer 负责独立验收。
- 固定审读输入：eng-cc/oasis7，ebd9b8431f2dc084c4ef6be3bcea9ca054ab2a81，2026-11-05。
- 上游：[CR002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；政策：[持续服务合同](../../game/gameplay/gameplay-continuing-service-fulfillment-and-remedies-contract.prd.md#continuing-service-policy)；方法：[验收手册](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#service-acceptance-scope)。

## 1. 问题、目标与非目标

既有电力检查点已设计两人供能的履约和救济；原 CR002 同时要求维护/维修、运输、协作及组织/多方责任；一般组织/多方的有限供能也须在同族有真实接收路径。当前物理维修、物流、工业工作和组织连续性各有真值，仍缺把这些真实结果与服务单位、价款、有限责任和旧义务保护连接起来的共同合同。

目标是一个有界、可恢复的持续服务族：有限Electricity供能、维护/维修、物料运输及原领域工作协作采用同一服务义务和责任账，实际效果沿原领域生产；每个已完成范围、未满足范围、费用、保全、继受和待付款都可追溯。有限容量是实现 admission，不能把某一类原采纳服务留作无接口的“其他类型”。

本设计不改原物理公式、R&D 归因/份额、工业许可、组织清偿优先级或原个人供能参数；不创建自由文本执行、贷款、外部资产桥、全球法院、普通 native Service 新语义或 SC-32 pilot。新状态属于受治理 institution instance；没有真实适配器/权限时禁用相应业务，而不是把设计写成当前运行事实。

## 2. 上游约束与相关角色

[共同形成合同](agent-mediated-contract-formation.design.md#contract-formation-family-handoff)产生完整不可变条款、必需接受和当前 mandate 的可验证输入；它不提前产生服务 obligation。本文的 family executor 在同一 activation 中验证真实资金/执行闭包并建立原义务。物理域拥有真实工作、损耗、保管和终端结果；Kernel 拥有受保护资源与统一发布；本地服务 authority 才能确认责任；情境来源只读消费结果。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 四类服务以稳定单元保存已完成、未满足和责任，不把换版本当新效果 | [义务模型](#service-obligations) | 原领域 receipt/所有权与完整源索引 | 无来源的自然语言结果不产生履约 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 个人/组织/多方角色、风险、资金和历史责任准确归属 | [主体与责任](#service-parties-liability) | CR001 当前代表 mandate、原资产权属、组织清偿 | 不把组织或成员关系变为财产权/无限连带责任 |
| [AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 维护、运输、协作各有真实净履行与部分结果的正反判据 | [领域证据](#service-domain-evidence) | 维修/物流/工业/R&D 原 writer 与真实接收 | 当前源码的局部物理能力不代签服务接通 |
| [AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 修复只处理残余，终止保留真实损失，最小保全不阻断无关义务 | [范围恢复](#service-scoped-recovery) | 原 work disposition、真实容量/返程/停止能力 | 不回滚实际 sink、在途或第三方结果 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 有界证据/归责/随机本地程序、一次复核和有效责任优先的尾部 | [本地复核](#service-local-review) | 合法服务事项权限、名册、随机与费用 | 不借其他领域 panel 或运维身份裁定 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 必需方明确接受后只交接真正残余；旧责任/资金/时界继续 | [继受与控制变化](#service-succession) | 原接受/当前 mandate、物理闭包及组织顺序 | 不自动转移个人 credential 或未知在途效果 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 真实 payer、完整调用/物理成本及各主体程序/关闭预算可计算 | [资源与预算](#service-resource-budget) | 合法收费/资源 authority、mandatory metering、源 cap | 本次不证明生产价格、硬件吞吐或经济平衡 |
| [AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 同一原请求/单位/付款去重，根未知 fence、完整恢复和历史重放 | [持久恢复](#service-persistence) | ExecutionTransaction、authority profile 与 generation | fixture 不代签未运行节点/故障域 |
| [AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 重连同源区分已做/未满足/待裁/待付款及可做下一步 | [结果与来源读取](#service-read) | Service/API/Viewer/Agent 与情境来源 consumer | 不用投影或声誉改写原责任 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 与共同形成同根激活完整服务，报价/接受待校验无业务效果 | [激活接缝](#service-activation) | CR001 不可变条款/所有必需接受与 family handoff | 不重复定义 CR001 签署/一般形成状态机 |
| [AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | 运输的当前路线/容量、真实托管/货权、部分交付/损耗和安全返程逐原件区分；资金与cargo一次结算 | [运输与托管](#service-transport) | 原物流/custody/terminal writer及跨区结算authority | arrival、运费或解除价款hold不代替交付/货权或真实返还 |

## 3. 当前状态、目标状态与差距

下述是在固定输入亲读的静态事实。原服务文档的历史基线/测试状态保持；未运行 Rust、服务、模型、玩家或跨节点验证。

| 直接来源 | 当前可证范围 | 目标接线 |
| --- | --- | --- |
| [EconomicContractState](../../../crates/oasis7_world_state/src/gameplay_state.rs)、[生产入口](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_policy_contract.rs)、[prepared publication](../../../crates/oasis7_world_state/src/world/economic_contract_publication.rs)、[expiry](../../../crates/oasis7_runtime/src/world/gameplay_loop.rs) | 旧 Open/Accepted/Settled/Expired，生产 Open/Settle 拒绝 Service，Settled/Expired prepared 分支也拒绝；expiry 只处理 AtomicExchange。Opened/Accepted prepared 分支不包含同样的拒绝 | 保留旧协议；新服务使用明确 module-owned 记录、unit/资金/责任和新消费者 |
| [资源转移入口](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)、[raw资源publication](../../../crates/oasis7_world_state/src/world/economy_data_publication.rs) | 原入口核正量/双方存在/共址、Data权限和余额；prepared raw transfer只执行实际余额变化，同账户不产生净交付 | resource_delivery须补全部原单位/授权/hold/净量/价款/费用同根接线，既有资源事件不自动是服务完成 |
| [MaintainFactory](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)、[factory reducer](../../../crates/oasis7_world_state/src/state/industry_transition/factory_lifecycle.rs) | 仅 builder 可维护；按 hardware_part 每份 25,000 ppm、最大 1,000,000 恢复；先检查请求量，按实际恢复向上取整消费。prepared 层重验 builder/retired/material，但没有服务绑定/归责/付款 | 新服务 adapter 必须验证 exact 资产授权与 source sink；不能把供方 ID 冒充 builder 或由当前 condition 回填完成 |
| [TransferMaterial](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_core.rs)、[logistics due](../../../crates/oasis7_runtime/src/world/logistics.rs)、[transit reducer](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs) | 明确路径选择、源扣减、tariff、电力、在途和单次完成；完成核原 job 与 loss，记物流 receipt | 单独接入 custody/title/terminal 与服务 unit；tariff 和 arrival 不是商业服务完成 |
| [recipe 完成](../../../crates/oasis7_runtime/src/world/industrial_completion.rs)、[工作图](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-backpressure-terminal)、[制造](agent-industrial-manufacturing.design.md#des-agent-mfg-003)、[RD 贡献](rd-charter-provenance-and-share-transfer.design.md#rd-contribution-source) | 已有原工作/结果合同及有限实际子能力，生产不等于交付，RD fact 不自动判劳务责任 | 以精确原 role/effect 和 declared predicate 接入协作；不新增归因权或免费工作 |
| [组织处置](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_remedy_commands.rs)、[清单](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_helpers.rs) | ExistingContracts、ClaimsAndCosts、BusinessTransfer、Residual 分别有 adapter_unavailable；material 清单只来自真实 estate deposit，有限 held-in-trust 返还 | 服务提供完整 obligation/hold/claim inventory 和合法 disposition，原组织 consumer 再按原顺序执行 |
| [metering](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)、[算术](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs) | 实际 artifact owner 的 Data/Electricity 预检/收费和 treasury；缺 owner 跳过不是新服务免费 authority | 精确 fee hold、payer/current source 和全部真实 graph；不能只适配服务入口 |
| [Agent delegation](../../../crates/oasis7_runtime/src/world/agent_delegation.rs) | Move/TransferMaterial/Build/Schedule 等已有 action scope；维护没有完整现成 delegated action scope | 正式 adapter 按共同 mandate 与原物理授权正向接线，旧 API/CLI 不因文档自动支持 |

## 4. 边界与结构

服务条款/案件属于 L2 institution，采用准确 world/branch、module/instance、artifact/schema/activation；不新增制度专属核心 Action、WorldState 顶层服务字段或第二套余额。Kernel 只实施原资源/权利、可验证受限 effect 和同根发布；机构模块不能写任意账本。原[通用 hold](checkpoint-power-service-fulfillment.design.md#service-resource-holds)继续拥有 reserve/release/consume、原资金人及全路径保护机制，本设计补多资源/原物料托管的具体接线。

一 agreement 一个受权 service instance，current state 保存紧凑完整索引和必要状态；完整条款、接受、工作原件、证据和历史仍保存在可回读内容及原 journal。state 不是全部历史的替代品。原物理工作 graph、RD/许可/组织 instance 保持独立；任何同根跨实例操作都必须列出完整调用/读写闭包，不继承某个旧 profile 的两次调用假设。

服务绑定只给原域一份 ServiceWorkBindingV1：原 agreement/slot/range、精确工作 plan、当前主体/资产 grant、资源目的、时间和唯一 source-use identity。原域仍验证本来所有前置，包括 builder/owner、产品/配方/许可、位置、容量和当前受保护工作。目标受权维护 adapter 可以消费资产 owner 的明确狭义维修权限；不能通过伪造 builder、放开整个旧 native action 或管理员补事件绕过原前置。未具备该 adapter 时明确 unavailable。

源索引是同一根的受验证派生事实。按 agreement/party/asset/work/original effect 有界查完整集合；不全仓扫描合同、不把未读页当空。自由消息、LLM 解释、链外付款、当前余额和一个 event ID 均不单独证明履行、责任、权限或结果完整性。

## 5. 关键运行流程

<a id="service-activation"></a>
### 5.1 共同形成到一次真实激活

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

只读报价和接受登记消费 CR001 的[共同条款](agent-mediated-contract-formation.design.md#contract-formation-common-terms)、[必需接受](agent-mediated-contract-formation.design.md#contract-formation-required-acceptances)与[当前代表权限](agent-mediated-contract-formation.design.md#contract-formation-principals-mandates)。提交接收 exact immutable TermsBody、render digest、全部 required-principal acceptance bundles、mandate/source/period、原请求及当前 cut；服务字段不能作为未渲染扩展悄悄追加。

1. 解析完整 typed obligations，拒绝未知 kind、歧义单位、隐含角色、循环依赖、缺停止路径或只有 supported=true 的 adapter 声明。
2. 验证全部当前财产/代表/执行权限、源索引覆盖、单位/时间/风险/费用与当前合法 manifestation；完整请求必须被证据和授权包含。漂移时整个候选拒绝/重报价。
3. 为每类真实 work plan 列明 normal、repair 与安全关闭的精确 producer/输入/输出/消费/费用图；现有目标领域的每一种必需义务都有下述正向 adapter。未来物料/电力若不预存，明确为可失败风险，并保护已同意责任与费用；预期收入不能资助激活所需 hold。
4. 当前足额保护全部价款、已要求 collateral、执行/程序/关闭费用和声明预留的物料/容量；保护原累计授权暴露，缺必须先存的任一量拒绝。未来未预存的原输入不伪造 reserve，开始前必须实际具备。
5. 在 family executor 一次根提交建立原 units、accepted risk、hold/index、程序槽/关闭资金、原 mandate exposure 和 active receipt。共同形成验证在此 executor 内完成，不另收一个 formation module invoke。
6. 同原 activation key 的另一个请求或同内容重放只读原结果；两个接受并不产生后台自动开工。新工作必须由当前有权主体按原 unit/plan 明确发起。

family prepare 是原版本化 L2 institution 的业务执行，common 校验嵌入该完整调用图；Runtime/host 只做通用身份、来源、资源、成本和提交校验，不解释服务义务或另开 native Service activation。激活可真实运行 sandbox 并暂存真实计量；整项拒绝、已证 NotCommitted 或重复输入不发布业务费用、hold 或义务，成功才把实际完整调用费与 activation 一起提交。拒绝零扣款不表示从未使用 CPU；协议 admission 单独有界且不产生游戏权益。任何确需其他模块的校验都计入图，不能写零 invoke 或隐藏第二个形成调用。

<a id="service-domain-evidence"></a>
### 5.2 四类正向履约 adapter 与源消费

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

每个原义务绑定一个稳定 DomainEvidencePredicateV1，枚举为 resource_delivery（仅原Electricity供能）、maintenance_delta、maintenance_interval、material_delivery、work_contribution 或 joint_work_result。它指定 source schema、asset/work/role、原世界范围、接受条件、quantity/unit/quantum、直接来源和完成边界；不是客户端表达式。每个 source adapter 必须同时提供 current authority、真实 work prepare/publish、typed result、完整 history/coverage、safe disposition 与原请求查询五个接口，禁止用空回调跳过其中一项。

工作开始先取得唯一 WorkAttemptV1，保护当前实际输入、必要 completion 与 safe-exit 费用/容量并持久绑定原调用图。进入不可逆边界前再验当前授权。物理结果沿原 domain transaction 发布；匹配的 service delta 可同根提交时一起发布。长流程原结果已发生、服务核验/付款尚未完成时，原 domain publication 同根保留 ServiceEvidenceOutboxV1 和 source-use reservation；显示 performed_unsettled，不删除真实结果、不宣称 fulfilled，也不开放第二次执行。后继只核原件并一次推进服务单位/款项，不能再运行原动作。

source-use key 为 (world/branch, original_domain_effect, original_effect_role, original_lot_range)。原领域固定 effect_role，调用者不能改名；对“消耗为一次服务履行”的角色跨 agreement 检查唯一占用/消费。共同成果可以在同一已接受共同单元内服务多个受益人，其付款/数量分配事前列齐；不能为每个受益人再消费整份结果。RD provenance 等只读事实引用可以同时存在，不因此再发一笔服务钱。激活前的历史工作仅可作已披露 baseline，不能补成新的有偿履约。

**有限资源供能。** resource_delivery 只绑定原 Electricity 的整数净量，Data仍是明示价款/费用，不由本枚举开放任意货币或远程电网。原producer为受权资源转移：验证精确from/to账户、双方不同且canonical共址、当前资源/资产控制与服务mandate、可用Electricity减全部hold及本次同源费用后的足额、检查点窗口/原未满足单位，保留原资源producer全部前置。组织只可用其真实受权来源，成员或第三方资源另须原权利人明确同意；未来电力不预造，没电/暂停/不共址/失权均不计交付。

真实同根计划包含供方Electricity扣减、接收方等量credit、该原unit/source-use完成、已签价款或joint_completion条件权利的准确变化、适用collateral释放、Data/执行费用和receipt。多供方分别有原scope/来源，不能一个account自转或循环转回冒充多份净供能；同一原effect不能在两份合同领钱。若一个多方whole结果尚未齐备，实际已交电仍保留，价款是否earned严格取预签规则，不因下游未齐把已发生电免费退回或再交一次。消费同一原交易含price与fee时按合并前余额核，不能用本次未来价款融资本次供能；已建立的狭义Data付款权与普通可撤销grant分别重验。

当前 [EmitResourceTransfer](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)检查正量、双方存在、共址、Data方向授权及源余额；[raw资源publication](../../../crates/oasis7_world_state/src/world/economy_data_publication.rs)保存真实资源delta但没有本服务的授权/hold/单位语义，同账户分支仅活动更新不能证明净交付。目标adapter必须把服务、通用protected available和原raw/due/direct/trusted消费者一起接入，不能用合法原资源receipt之外的active或accepted状态代付电。repair、终止、有限归责与继受沿本族相同原单位/真实资源规则；个人power v1的两个主体、数量/价格/抵押和时界原文不动。

**维护/维修。** 读取原资产 lineage/当前 revision、受权 operator、原 condition 量纲/max/repair quantum、材料/电力真实 sink、before/after 与因果 receipt。每个维修单元在接受时固定可测目标或恢复区间，实际净变化只归原 source 一次；请求 parts 或新的 condition 总值不是完成量。factory adapter 保留 25,000 ppm/part 和实际向上取整消费，超 max 的部分不再计修复；chassis/其他原已采纳物理承载消费各自已授权规则，不能用工厂 ppm 替代。maintenance_interval 除效果外还需要原固定区间的完整状态/可用性 coverage；缺一个区间即 pending_evidence，不用末尾正常推全程合格。后来新的损坏/折旧有自己的 source，不复活已完成维修 unit。原 WIP、停机/W 重计、运输和终端结果仍由各自原合同处理。

<a id="service-transport"></a>
**物料运输。** 计划明确 original title/lot、source ledger、承运/保管责任主体、当前有效路径/边及容量、目的地接收/有限仓储/terminal、原费用与允许损耗。开始把同份真实可用物料转为唯一在途/托管，占用沿途容量，不能同时留源可支配余额。原 receipt 分别证明 dispatch、arrival、storage_acceptance、ownership/terminal settlement；只有原条款指定并完整满足的最终边界消费 service unit。当前 native MaterialTransitCompleted 的收到量和 tariff receipt 只消费其确实证明的维度，不能合成缺失接收/货权证明。

部分交付允许时，按原 lot 的不重叠区间记实际 received/accepted 量；整批 whole 时保持已发生 partial cargo、资金待决，直到 whole 谓词或原停止处置成立。原损耗为 sent = remaining custody + received + confirmed loss 的互斥分量，按原物理公式/receipt核对，不能由服务层重算成零。carrier、warehouse、货物所有人和资金 escrow beneficiary 分开。拒收不自动转货权，解除价款 hold 不等于返还货物。补运、改道、返程和合法保管各有新的 parent-linked work plan、实际资源/容量与费用，仍满足原未履行单元且保留原损失；合法返程到原接收 receipt 才计 returned。若当次正常路径缺合法后续保管/返程/停止方案，应在首次源 sink 前拒绝。

**原世界工作协作。** work_contribution 明确原工业 stage/join/bundle/terminal、制造工作或已有效 RD charter/work unit 的角色与 scope；joint_work_result 引用有限、无环的 required contribution 集。slot 消费原工业[身份/齐套](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-join)和[terminal](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-backpressure-terminal)，不能自行把 production 变交付。原 RD 的 Begin/Result/来源许可、永久 Agent provenance 和份额仍由[原 RD authority](rd-charter-provenance-and-share-transfer.design.md#rd-contribution-source)判断；服务不能追加研究结果、把负实验改正结果或修改 bps。合同若约定完成一次合法实验，真实阴性/失败结果可以满足该工作义务；若约定某个已可判定成果，则按原已接受谓词继续未满足，风险明确，不能承诺必然发现。

共同工作的贡献事实、整体完成、价款 earned 和责任分别建账。A 已交材料、B 未完工时 A 的 source 与 sink 不重做；joint_completion 价款按原取消/结算条款处理，不能自动指控 A。原不可分 join/WIP 必须整体安全处置；可独立分支才能单独恢复、结清或交接。维修、运输和协作混合在一个 agreement 时仍按同一依赖/单位/资金约束，不产生三套并行责任 ledger。

<a id="service-scoped-recovery"></a>
### 5.3 残余、最小保全与四种动作

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

检查点按原 canonical tick 读 due 与 R；时间读取不新增罚款或费用。剩余量来自原 unit partitions，不用总余额差或当前 asset condition 倒算。repair/reperform 的请求范围须全部在原未满足集合，并满足原 [due,R)、现行 domain 权限和真实资金；任一相交已完成/终结范围拒绝整项，不返回偷偷裁剪的结果。

ServicePreservationV1 保存原案/权利来源、精确 unit/asset/fund scope、受阻 effect kinds、理由和解除条件。计算最小冲突闭包时只增加：实际会消费争议资产/资金的原 operation，及必须共用该不可分 work node 的原 required parents/outputs；不把仅共享组织、payer 或相邻地点当冲突。对被保护 scope 禁止新 liability、不可逆交付/货权变更、提款和扩权；不相关的 due unit、已合法在途的安全完成、必要保管与只读核对继续。安全动作仍须原授权，不凭 stay 创建任意维修权。

保全不另开未计数命令：Register/Merge/Evidence/Appeal可在其原成功迁移内携typed preservation_request，受权service institution分别核直接原权利、具体原争点、准确资产/单位以及原source证明的冲突不可逆effect；全部成立才同根建立或更新最小stay。受理仅登记争点，并不自动满足这些保全谓词。无权限/无原权利/没有真实冲突的保全请求不产生stay；登记与保全各自结果明确，不能用一个accepted布尔混淆。

原withdraw/同版和解、phase结论/final、Terminate/Close或有权原work迁移在各自已计graph内重验并缩小/解除对应stay；不增加新的独立invoke或次数。客观source纠错由原writer追加，本服务任何有权效果前都核当前source version/validity与最小冲突闭包，不能以旧stay缓存放行；无完整当前证明则拒绝不可逆效果。服务投影修订附在下一次原已计case/work/最终处置迁移，不自动调用无限refresh。没有新合法迁移时只读标需重验，不收费、不刷新case窗口或创造新权限；原资金/物理保护继续沿原权源。每个原迁移的graph与受限资源计划必须容纳这一保全delta，不能把它藏为第二个调用。

TerminateServiceScope 停止选定范围新增 work，并为其每个未终结对象固定 disposition。采用[已签取消 schedule](../../game/gameplay/gameplay-continuing-service-fulfillment-and-remedies-contract.prd.md#continuing-service-recovery)：未开始释放未消费量；WIP 保留 sink、适用 finish/stop/rework；transit/边界货物保留 custody，执行真实返回/接收/保管；buffer/terminal 保留 recipient、容量与原 provenance。没有可支持的处置不能标 closed；原不可逆新开工必须预先具备至少一种可达的安全终结或合法保管状态。安全保管可能留下明确原权利待领，不等于已经退款/送达。

责任决定与业务停止是独立维度。已 earned 价款、有效 award、原有债权、已获许可的在途 effects 和费用不因 stop/close 删除。中性终止仅释放已证明不再承担义务的价款/collateral，不能释放仍可能抵偿原有效责任的量；原 owner 收款来源失效则原权利 return_pending，不能给当前新 owner。恢复或换签前先核原 request，业务 unknown 不形成新的 repair 授权。

<a id="service-parties-liability"></a>
### 5.4 主体、组织清单与可判定责任

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

PartyRoleBindingV1 绑定稳定 legal principal、个人/组织类型、服务角色、原 mandate/source/period、当时代表/controller/Agent 任期和签署正文。组织 current signing clause 由 CR001 验证；普通 continuity grant 仅证明其原受限 scope，不能替代正常业务签署。actual fund owner 单独解析，不把 org 标签附在个人 ResourceStock 上就改财产权。实际执行前按当前代表/物理 grant 重验，历史责任继续属于原已成立主体。

每 obligation 的 RiskAllocationV1 固定 risk_kind、cause predicate、责任人、受益人、资源种类、loss unit、cap/collateral、共同分担向量（适用时）和既有第三方权利。damage_id 引用原损害 source/lot/range；跨请求/不同被主张方累计已补救及已赔偿，remaining_claim = accepted/proved loss − 已互斥确认补救/赔付，不允许重复全额主张。没有共同承担条款按各自义务/因果分别判断；共同责任也不能超出每方已签 cap 或改变第三方权利。

LiabilityEvidenceCoverageV1 必须由原 source index 提供完整 before anchor、窗口内变化/尝试、owner/位置/接收/容量/可用性/保全、精确 end watermark 与不遗漏证明。host 核连续性和来源，不接受 complete=true 自报。formal default 只来自同版有效责任和解或 executable_final 的服务决定，并满足原条款、未满足、完整窗口、接收机会、可归责触发、因果和数额；信息不足记录未证，不从未回复/财富/声誉补证。

执行赔付时，authorized_amount 不超过 min(尚未补偿的已证损害、该方/风险/单元剩余已接受 cap、有效决定量)。实际 paid 另不超过当前合法 source/hold 的可付量；已成立其余为 confirmed_unpaid，不生成余额、不偷取其他价款/费用。账本分别记录 admitted claim、有效责任、paid、unpaid、原 collateral 及解除量。服务 cap 不覆盖其他原 authority 已确认的独立责任，不能由本案替其免债。

给组织连续性输出 ServiceObligationInventoryV1：按同一 root 的完整 agreement/slot/provider 段、hold 原权利人/负担、原工作/在途、earned/未付/award、未知结果、允许 disposition 和新风险 fence。原组织 case 的 ExistingContracts/ClaimsAndCosts/BusinessTransfer/Residual adapter 必须读取全页和 coverage，并把本服务的实际结清/保留证明接回原阶段；缺 coverage 显示 unavailable，不推进为零负债。组织维持自己的[顺序和分配算法](organization-continuity-and-disposition.design.md#des-wr-oc-001)，不在服务再实现一套破产清算。

<a id="service-local-review"></a>
### 5.5 有限案件、最终性与真实结算

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

严格消费[唯一政策](../../game/gameplay/gameplay-continuing-service-fulfillment-and-remedies-contract.prd.md#continuing-service-review)的 Q、R、半开时界、3+3 人、2/3、名册上限及两阶段各 16 引用；本文不另定一套到期。local-service.review 必须是目标世界明确授予事项/地域/原资产 cap 的 authority，signature/普通 passed/另域 panel 都不够。与原供能使用相同程序规则不代表既有实例已有该权限。

case key = (original agreement lineage, checkpoint id)，不含 current provider/terms revision。登记/merge/evidence/appeal 按原 party slot 和 phase 分槽；原 actor 及未来继受保留的 lifetime slots 由激活时预算覆盖。固定所有合法输入的 scope/evidence root 后才能有效投票；不得先冻结早到一方的子案。后来的来源客观更正追加原件/原因并触发当前消费者重验，不刷新普通有损复核窗口。

随机选择先持久固化完整 case/roster/conflict roots，再使用获准源的首个不可预选后续结果一次确定排列。初审和复核分别从原排列选前三个当前无冲突且没有参加另一阶段的人。替换只沿该排列，旧票保留历史但失效资格不能计票；同一 phase/candidate 的票或排除决定各最多一次。有效票必须绑定同一 frozen root、精确单位/损害/责任人/资源/amount 与 typed disposition；两张同义票不能由不同字段投影拼成结论。

初审只是 provisional。executable_final guard 在付款/取消权利的提交点重验：完整原申诉窗已关闭且无有效待决复核，或已有有效复核结果，或原复核截止后仍有效初审依法 final。有效申诉在半开端点前 canonical 登记就必须纳入该原复核；消息收到/排队不算登记，等于截止的输入不会先执行后补录。原初审是否被合法 invalidated 也是真实状态；无 review quorum 不自动撤销有效初审，更不能推定申请人胜诉。

在 R+8Q 尾部先核完整原 case/所有决定/和解/支付/request 与 source-use。存在有效 final/原仍有效结论，按其精确范围继续合法未付；已证明无有效改变原基线结论时解除程序保全，执行原中性停止/返还规则。任何 authority/commit unknown 转原件恢复，不能依缓存没见到决定退款。final、合法中性尾部与已执行 payment 共用原 resolution/disposition key；到界重复不再消费资金或再次收费。

受理不是 stay；stay 不是 final；final 不是 paid。无 panel/random/预算时不得激活承诺这种程序；运行后真实资格失效则按原有限尾部保留已确认责任。此程序不产生世界通用司法权，也不决定 R&D share、许可证来源或声誉评分。

<a id="service-succession"></a>
### 5.6 显式继受、撤权、组织退出和续期

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

PrepareServiceSuccession 只读列出全部受影响 obligation/fragment/work、current provider、旧责任、holds、待付/未知、必要接受人及计划改动。必需接受集合由实际权利/风险变化决定：退出与进入方、被改付款/收款/财产/配合条件的主体都在其中；旧未被改动历史主体不被迫为新范围重签，但仍有其原程序、债权和读权。CR001 支持最多 32 个必需 principal bundles；本族最多 16 个当前参与者、32 个 lifetime responsibility slots，单次替换可表达 16 个当前方加一个新方。

范围只能是原真正未满足且原 owner 证明可分的单元；共享不可分工序/托管物、尚可能恢复旧履行的争议范围或 unknown 的原 operation 不能拆交。新方的冲突关系、保留程序份额和当前适用panel资格也须成立，不能降低任何历史主体的原机会或把已经冻结的旧争点偷换当事方。资金、权限和 work readiness 全部成立时，同一根缩旧范围、激活新范围、新方 funding/执行绑定、相应 hold 调整、provider segment 与唯一 receipt 一起提交。旧方仍有损害/award/返还的量不释放；未开始旧请求拒绝，不继承为新方新请求。交接与最后一次旧履约谁先提交，后者重验原 units，不允许两个赢家。

控制/mandate 撤销或到期立即阻止其新的代理动作和扩大承诺；当前原权源 guard 生效不等待服务投影刷新或付额外费用。已成立服务债权和原狭义资金 disposition 按原合法 drain 继续，回写原暴露 key 不复活旧代表权。StopNewServiceScope 每 lifetime principal 留独立一次全范围 fence 槽和关闭费，不被普通修复/证据修订消费；已 stop 的 scope 不能以另一个 lineage/新 Actor 别名重开。合法新工作只能形成明确的新范围/接受，旧 responsibility/source-use/history 仍不清零。

组织退出先固定完整 ServiceObligationInventory，再沿原 ExistingContracts、custody、claims、business、residual 顺序。需要搬运/停工/返还的量沿真实原域动作，不把完成组织 case 当物理完成。续期由共同形成创建新明确范围，引用原谱系；已完成 unit/source-use、原 mandate D、固定截止与历史责任不移到新 agreement 后重置。

## 6. 接口、数据与状态合同

<a id="service-obligations"></a>
### 6.1 稳定身份与不可变原件

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

以下为目标逻辑 schema，不能当现有 Rust/HTTP/ABI。 正式共同family discriminator为continuing_service_v2；本地ServiceTermsBodyV1等V1是该family内的typed record schema版本，不能把family改成另一个未注册字符串。编码、域隔离及 canonical hash 沿 [WASM 执行合同](../wasm/wasm-interface.md#sr2-execution-contract)。金额、数量、tick、乘积、索引和序号一律 checked 整数，禁止浮点、隐式 saturating 和身份规范化。原物理兼容计算保持自身已批准语义；本服务不能改其公式来使新上界通过。

摘要顺序为：完整不可变 ServiceTermsBodyV1（排除自己的派生 ID、common digest、接受及 activation receipt）→ family terms digest → 引用并承诺该完整正文的 FormationTermsV2 → common terms/render digest → required acceptance bundles → request digest。activation 同时保留 family/common 摘要与原件。record_version 不是不可变条款，但 expected versions、完整命令内容及所有授权/接受引用都进入 request digest；换 CAS 后复用 request ID 必须冲突。

| 记录 | 必需字段/来源 | 不变量与消费者 |
| --- | --- | --- |
| ServiceTermsBodyV1 | world/branch、原 agreement/instance/policy/activation、party roles、原单位及依赖、价格/风险、Q 与绝对窗口、工作/取消/安全图、来源及预算、有限名册与随机规则、完整必要接受集合 | 条款中的真正 obligation、货权、风险、费用不能藏在未签附注。所有正文可回读；摘要不是权限 |
| PartyRoleBindingV1 / ProviderSegmentV1 | 稳定 principal/type、原角色、组织/代表/controller/Agent 任期、当前执行 mandate 及来源、适用单位、历史起止 cut、前/后继关系 | 签约主体、实际代表、资产人、费用 payer、责任人分开；历史责任不随当前 controller 字段覆盖 |
| ServiceUnitV1 / UnitPartitionV1 | 原 agreement、checkpoint、obligation slot、integer range 或 whole、quantity/quantum、required child 与结果谓词、最早/最晚时间、provider segment | 未开始/执行中/performed_unsettled/确认完成/终止处置等分量互斥且覆盖原范围；每原 effect/lot 的实际 source-use 唯一 |
| WorkPlanV1 / WorkAttemptV1 | 正常/修复/安全类型、parent attempt、原 unit scope、逐节点 artifact/instance/entrypoint/ordinal/fee source、物理边界、current grants、输入/容量、完整上界、原 request/outbox | 调用节点代表一次真实 invoke，不代表一套模块；相同模块的 start/complete/validator 是不同节点。业务重试不能换 ID 重新做未知动作 |
| DomainResultV1 / ServiceEvidenceOutboxV1 | 原效果类型/ID/role/lot、原请求、前后 root/tick、真实输入 sink、实际输出/损耗、接收/货权/terminal、coverage 与 correction、source-use reservation | 原 writer 同根产出；物理发生而服务未付为 performed_unsettled。outbox 重投只关联原结果，不再执行 |
| ServiceHoldBindingV1 / FeePurposeV1 | 原 funds/rights owner、账户/资源/amount/purpose/scope、狭义处分权、price/collateral/operational/closure 的精确子账、调用/程序/最终返还槽 | 沿通用 hold 的原四个 purpose；procedure 是 operational 的受保护子项，安全退出/最后费用属于 closure。不存在可自由转账的第五份余额 |
| PricePositionV1 / LiabilityPositionV1 | earned/conditional/paid/unpaid、original payer/beneficiary、unit/fee basis；逐损害 risk/cause/source、accepted cap、实际 remedy/award/paid/unpaid | 义务状态、责任、金钱状态相互独立；损害的修复与付款按同一损害单位互斥扣减；不能因合同 closed 抹去 unpaid |
| ServiceCaseV1 / PreservationV1 | 原 checkpoint、scope/各阶段 frozen roots、全部 party slots、名册/冲突/随机 proof、票与失格、初审/申诉/复核/final、真实 disposition | 固定时间和名册排列不重置；每次资金处置必须匹配当前 executable_final 或已签停止事实及原 scope |
| ServiceBusinessReceiptV1 | 原 operation/request/terms、unit/effect/provider、所有 touched source/root/version、资源/hold/fee/责任 delta、真实状态与下一步、ExecutionReceipt 引用 | 同一候选与原 finality wrapper 分开；无网络 ACK 替代实际 committed |
| ServiceOutcomeSourceV1 / ServiceObligationInventoryV1 | §11.2 的完整原结果、责任与受众；按原组织/主体索引的合同比较切点、全部未结物/钱/程序和 coverage | CR003 只消费来源；组织沿原清偿顺序使用清单。分页缺失不等于无义务 |

数量分割只能沿已签 quantum 和原物理 lot/range。一次拟提交的所有范围规范化、排序、去重并对原 partition 做全包含校验；任何交叠、空洞、越界、重复消费或变更 whole 都拒绝整项。价格使用条款的整数单价/定额和累计 basis；允许分割时 signed allocation 必须确定每个最小单位的净价与风险额，尾差采用签定的固定顺序，达到全量的合计严格等于原金额。不能每次向上取整一份完整价款，也不能把新拆分当新 fee leg。合法交易费沿原累计政策，当前差额为 total(basis_after) − total(basis_before)。

price、collateral、fee 与真实材料 custody 各有自己的原 source。Kernel 同根约束 gross_after ≥ sum(held_after) ≥ 0；reserve 不变 gross，release 不生成 credit，consume 同时减少原 hold/gross 并按合法 transfer/sink 进入精确去向。材料 sink/转保管依原物理 authority，服务不把通用资金 hold 扩展为任意 burn 或创建库存。选择一份受限资源计划时，institution 已计算完整业务结果；host 只校验该计划中逐项原权利、金额/quantity、旧版本、源和守恒，不重新解释服务 quality、价格或案件。

### 6.2 操作与结构化错误

写入统一带 exact instance、original lineage、operation key、完整 request digest、expected versions、当前 authority/accepted source refs。相同 key/相同 digest 返回原 committed 或 unknown 查询结果；相同 key/不同 digest 为 request_conflict。同一业务单位/资金/物理 source 的去重另跨请求成立，不能仅在 API request cache 去重。

| 命令族 | 成功前置与同根结果 | 失败/重试边界 |
| --- | --- | --- |
| ActivateService | 全部 common/family 条款、必要接受、current grants、真实 hold/费用/退出、合法版本/服务 jurisdiction；新 active 与所有原件/占用/暴露一次成立 | unbound、stale、缺接受/资金/adapter/退出或调用失败全项拒绝，零新业务费；unknown 查询原候选 |
| Begin/CompleteServiceWork | 完整原 scope、工作图、当前实际资源/权限和必要安全退出；原 work receipt/outbox 与 service 关联 | 未开始拒绝不变事实；已发生物理结果保留。只有已签的 accepted_attempt_result 可包含无生产的真实计算费 |
| Resolve/TerminateServiceScope | 原实际结果、取消 schedule 或合法决定、受保护单位/价款/责任；真实物理处置与合法付款分开链接 | stop 不能把 transit 标 returned；无合法收款路径为 return_pending，无足额合法支付为 confirmed_unpaid |
| Register/Merge/Evidence/Appeal/Vote/FinalizeCase | §5.5 的准确权限、本人槽、原根/半开窗口、原随机/资格/最终性；一个原案追加或有限阶段改变 | 重复、超份额、无新证据、过期、不足 quorum、无权者不收费不侵占其他槽；不是默认胜者 |
| ApplyServiceSuccession | 可分残余、所有被改变权利的当前同意、新方真实执行/资金、history slot 及原程序能力；同根唯一交接 | 不能转交 committed-status unknown、已完成/已处分或被原不可分 work 占用的范围 |
| StopNewServiceScope / CloseService | 前者只停本人全部新风险；后者核原 unit/物理/资金/责任与完整原请求，先最后费用再余量释放 | 普通槽满仍可 stop；仍有权利/原在途/unknown 时留下精确非终结子状态，不删整条记录 |
| Read/QueryOriginalRequest | 当前读者可见的完整对应 scope、source cut/coverage、原 operation | 查询不扣业务费、不刷新窗口；原件不可读返回 unknown/unavailable，不提交替代动作 |

错误至少区分 unbound、terms_conflict、acceptance_incomplete、authority_stale/denied、source_coverage_unknown、adapter_unavailable、unit_overlap、source_already_consumed、resource_hold_conflict、fee_budget_exhausted、graph_unlisted_node、encoded_bound_exceeded、work_inflight/commit_unknown、case_phase_closed、quorum_unavailable、return_pending。诊断不得泄漏不可见的证据或私密 mandate；公开原因仍须说明当前可合法做的下一步。

### 6.3 必须接入的完整消费者

| 消费者 | 真实接线要求 | 可核的现有入口/专业来源 |
| --- | --- | --- |
| API、Agent、native/direct/trusted/模块写入口 | 所有 new work 都校验 original binding/units、当前 authority、protected available 与完整 graph；不可用输入禁止退回旧 Service | [合同入口](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_policy_contract.rs)、[Agent scope](../../../crates/oasis7_runtime/src/world/agent_delegation.rs)及正式 Service/Client 目标入口 |
| 资源供能、维修、物流、工业/RD/制造各 producer 与 due/completion/reducer | 工作开始、无模块 completion、真实 effect publication、最终接收和安全 disposition 都发布一致 source/outbox/use；直接 raw publication 不能绕过 | [维护](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)、[transit](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs)、[工业完成](../../../crates/oasis7_runtime/src/world/industrial_completion.rs)与 §5.2 原专题 |
| 资金/物料的所有 debit、fee、转移、销毁、claim/回收和控制变化 | 同一通用 hold/custody 索引在 preflight 与 publisher 生效；保留原受益权及已成立责任，不能只守服务入口 | [资源接口](../../../crates/oasis7_runtime/src/world/resources.rs)、[metering](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)、原 world-state publication |
| governance/registry/module upgrade、deactivate 和原许可/RD hooks | 当前合法版本含 compatible drain；所有实际调用和原许可消费另有唯一账；不重发 provenance/实验/费用 | [module 生命周期](../module/module-lifecycle.md#sr2-activation-contract)、原工业/RD 权威 |
| 组织清偿/业务继受和来源消费 | 用完整 ServiceObligationInventory 严格接回原顺序，读全页；CR003 读有效原责任，不新增 service 结论 | [组织消费者](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_remedy_commands.rs) |
| root transaction、journal/outbox、snapshot/load/replay 和当前读 API | 同代持久绑定原件/索引/费用/authority；缺页 fail closed；三个表面同源呈现 | [执行和提交](../design.md)、[原持久合同](checkpoint-power-service-fulfillment.design.md#service-persistence) |

任一可写路径仍能绕过原资金/物料占用、重复执行/计费或清空未知结果，该服务族不得在该世界启用。接口名存在、单一 module 测试、某个 profile 成功均不能代替这份全消费者闭包。

<a id="service-persistence"></a>
## 7. 一致性、持久化、并发与恢复

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

### 7.1 一次准备与统一结果

ExecutionTransaction 借用同一 canonical generation/root，准备 exact instance states、合法物理 delta、Kernel hold/gross/custody、原授权累计 D/O/C、trade-fee basis、source-use/index/outbox、请求结果与 business/execution receipts；所有 touched source/version 和当前 manifest 在最终 publication 前重验。不把服务 state 安装后再尝试扣 fee，也不先修改物理状态再补写唯一 source-use。

同时争同一原 unit/lot/hold/work 时按已有 canonical 顺序只允许一个候选提交；后者按新根重验并返回原结果或冲突，不能用旧 proof 各自成功。跨实例/cross-region 只在原统一世界 authority 实际可提交的闭包内执行；本设计没有独立 shard finality 或跨世界原子桥。

- NotCommitted：仅在权威能证明该原候选没有发布时丢弃其 staged 业务状态/计量；保留按原协议合法独立产生的拒绝审计，不能伪造一次成功。
- Committed：只安装原 committed candidate/receipt 中的全部 delta；不得基于当前余额/版本重新计算同一个结果。
- StatusUnknown：隔离整个旧 World 的写入与 tick 推进，保留原操作 ID、完整候选、root/outbox/authority 查询线索；不能只冻结服务行而让旧 World 继续推进。按原 operation 恢复为 Committed 或已证 NotCommitted 后才解除。网络失败、ACK 丢失、查询超时不是未提交证明。

### 7.2 active attempt 的费用与零费边界

非绑定报价/接受只用有界协议存储，不触发可计费 institution。静态 admission 拒绝、无权/预算不足、未改变状态、精确重放及整项激活拒绝不发布业务扣费或占用服务的成功槽。模块可能为验证候选已经实际运行；它的 staged 计量随该拒绝一起丢弃，CPU/请求体/并发防滥用由非业务 admission 独立限制，不能借此产生游戏费用、排队顺位或责任。

active 后，条款可以对原领域明确允许的真实 work attempt 预签有限计费。开始前保护 exact attempt/节点的最大合法费用、输入与安全资源，原 admission receipt 固定一次 graph/fee source 和允许的 accepted result kinds。服务实例不能在看到失败后追认收费。真实开始/完成均计入这份完整 graph；如果协议需要两个 invocation，这就是两个节点。

accepted_attempt_result 必须是成功验证并发布的结构化结果：包含实际执行到的各节点计量、原物理成功或明示可计费的业务拒绝、唯一费用 delta 与未执行节点释放。业务未产出可保持 no_production；已接受并真实发生的计算费是一项明确的原费用效果，不能伪装“没有任何 effect”再收费。它消耗一次本人的 normal work attempt 槽；可归责/价款仍依原证据，计算完成不产生履约成功或 default。权限、encoded/graph 超限、普通 sandbox Trap/Timeout/OutOfFuel、没有合法可发布结果或原政策要求零费的拒绝不能套用此分支。

先收到真实物理 receipt 而服务核验尚未提交时，原 outbox 保留该结果/计量及已经付过的节点。下一步只做原件核定和未执行的已列节点；不能再次运行已完成节点或再次扣其 fee。任何一次 admission/实际结果处于 StatusUnknown 都查询原 operation，不换 ID 重跑、不猜实际计量为零。原已独立合法提交的费用不因后续子操作失败回滚；未提交的子操作不凭预算上界收费。原供能 v1 的确定拒绝零新费规则保持原样。

### 7.3 原件、snapshot/tail 与可恢复关闭

完整恢复闭包包括原 family/common 正文和渲染/接受 bundles、当时及当前使用的 source/mandate/policy/artifact、全部单位与 provider segments、真实物理结果/原 source-use reservation、全部 held/paid/unpaid/basis/费用节点、程序名册/随机/证据/票/排除/截止/有效纠错、pending work/outbox、原请求与候选、原组织清单索引及读权限来源。current state 中 hash/ref 只是索引，原件及其合法可读 proof 必须实际留存；无法回读不能用“hash 对得上”重建丢失内容。

同 generation 的 snapshot manifest 覆盖以上对象及水位；snapshot + canonical tail 按原顺序重放到同 root，双快照/备份选择遵守原 authority。重放只安装历史实际结果，不运行 WASM/LLM、重抽 panel、按当前价格重算、为新 owner 重定向款项或补发同一 source 的服务奖励。index/coverage 版本不一致则恢复受阻；禁止清空索引重启服务。

完整原件至少保留到所有执行/程序/账款/原外部依赖均可证明完成，且满足各源原本更长的历史/审计保留合同。仍有 unpaid、return_pending、在途、source correction 可能影响的有效权利或 StatusUnknown 时不能 GC。最终关闭后的稳定 lineage/request/source-use tombstone、责任与原件可验证定位继续随原历史保留；TTL、压缩或 schema 迁移不得恢复消费额度。按源既有合法归档删除较大正文之前必须有完整可回读归档和完整性证明；不可回读只报 unavailable，不当作无权利。

对源客观纠错只追加准确 replacement/invalidity/canonical cut 关系，并由当前有权 consumer 重验未执行效果和合法保全；不得删除原已发生物理结果/费用或用新 raw event 伪造旧 root。需要真正相反物理结果或返还钱必须另有合法实际操作。纠错不是重新开放无限普通争点或任意新罚款。

## 8. 安全、部署与运行依赖

启用前需同时具备当前合法 L2 artifact/instance/schema 和治理 activation、CR001 全部身份/接受源、四类完整物理 adapter、通用资源/物料保管保护、服务事项权限/名册/随机、每真实 payer 的合法费用来源、全部消费者与同代持久恢复。缺任一项明确 blocked/unavailable；不从设计文件、普通 passed、registry 名称或维护者身份推导生产授权。

source ID、amount、role、owner、controller、cap_ref、policy version、case tick 与 graph 节点均来自验证后原件，不能相信 Agent 自报。签名只证明该 signer 对该正文的表达，不单独证明其角色、资产权或服务管辖。禁止 source 别名重领、同控制主体占多票、从共享组织推导连带责任或把对方缺席变默认过错。

authority 与受众在每次新效果及每页返回前重验。私密报价、证据、mandate 和部分组织账不得进入公共 score、日志或公开 reason 文案；对读者不可见的来源返回受限状态/可核摘要及正确边界，不用编辑后的摘要当完整验签原件。独立管理日志只记安全诊断，不允许管理员补签服务结果。

部署不新增 native EconomicContract 语义，不代替 SC-32 的独立分类/激活。兼容 drain 是当前治理证明的有限执行，不通过固定旧 artifact 绕过当前 manifest。暂停服务只阻止新风险；已有保管、费用、权利和 root unknown 按原受权路径继续恢复。真实运行/跨节点/真人验收仍是实施后的独立证据。

<a id="service-resource-budget"></a>
## 9. 性能、完整费用与有界资源

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

### 9.1 一份合同选择的有限形状

以下是该服务族的工程准入上限，不是生产定价，也不是每份小合同必须预留的最大数。TermsBody 明示实际 H/K/S/F/J、原名册 N、逐类真实 graph 与 source/retention 上界；每个原执行与关闭义务必须在所选形状内正向可完成。不能以“其他服务类型以后支持”省掉某个原已采纳类型的 adapter。

| 维度 | 准入界与完整性要求 |
| --- | --- |
| 主体/检查点/义务 | 当前参与 principal ≤16；包含原方和未来后继槽的 lifetime H≤32；K≤16 个检查点；原 obligation slots≤64。必要 funds/asset/fee owner 也计入主体；panel 候选是程序来源，不冒充签约方 |
| 真实残余与继受 | 全部规范化 fragments F≤256；S≤16 次成功继受且不能超 H。普通拆分必须为剩余完整原 scope 继续保留关闭能力，不能用满碎片让最后范围无法表示 |
| 正常与安全 work plans | J≤256 个明确正常/修复 attempt；独立安全 plan 上界为 F，每个当前残余 fragment 一份。一个 plan 包含全部 begin/complete、等待后实际调用、原许可/validator/maintenance/terminal hooks 及最多64个真实物理边界 |
| 完整调用图 | 每 operation/plan ≤64 次实际 invoke；重复调用同模块仍占不同 ordinal。所有原承诺迁移的累计预留节点 ≤65,536，normal/procedure/closure 各自锁定；较小原领域上限继续生效 |
| 程序容量 | 每 checkpoint 一案，最多16案同时未结；每 lifetime party/phase 各16个 evidence refs；名册 N≤64。最大完整 evidence refs = 16×32×2×16 = 16,384，不能按“两人一案”预留 |
| 有界节点 | 每次完整编码 input/output 各≤256KiB；最多32 effects/16 emits。只有明确单 institution、至多1 effect/1 emit 的纯服务元数据迁移才用更紧报价；额外实际 producer/validator 调用必须逐节点加入 |
| 正常协议 admission | 每实例2个并发执行槽，队列最多32个已认证 exact request；每主体每 canonical tick 最多4次新 admission。查询队列/重复/无权请求不消耗业务权利槽；关闭/撤权使用保留调度通路，不被普通工作或 long-poll 占满 |

这里的65,536是已签原执行预算的总预留界；它覆盖该合同承诺的全部正常工作、程序和安全关闭。后文所述自愿新增资金清偿一笔已确认旧债使用单独、当前明确授权的有限付款操作和费用，不重新开放原服务、重置旧计数或要求原债权人重签服务条款；其新存储/费用由该真实新增操作承担，不能无偿追加进已释放的原预算。

每个图节点保存 exact artifact/instance/entrypoint、invocation ordinal、最大完整 I/O/effects/emits、真实 payer/source/resource、目的子账和原 authority。同一 original operation/节点/fee source 只计一次；license start 与 complete 若真调用两次便是两个节点。已经属于原许可/RD 程序的未来责任和关闭预算仍须完整保留，不能在 service 再预留同一份钱，也不能因只列当前两个调用而漏掉原终止/复核。以原 source-purpose-fee identity 求并集去重，不能按模块名称去重。

被声称为 one-invoke 的元数据迁移只验证已存在的权威原件和本服务政策，不后台运行第二个 domain/Agent/model 以“补证”。如合法规则实际要求额外 world producer 或付费决策生成，其调用、资源和真实 payer 全部列入该类 graph。参与者自己的可选思考不是默认的服务债务；但强制的正式 Agent/模型工作不能借此漏报。未声明节点、额外循环或不受控重试在发生前拒绝，并保持已发生原事实及安全出口。

### 9.2 程序和最后一次关闭的计数

令 E=16。每 checkpoint 的保留成功写入上界如下；批量减少真实调用可以降低实际费，不减少任何当事人的已保留机会。

| 程序项 | 最大次数 | 专属槽 |
| --- | --- | --- |
| 登记/并入原案 | H | 每 lifetime party 一次；未启用后继槽不能发言 |
| 两阶段证据追加 | 2HE | party×phase×原引用序号；重复引用、改说明、无新事实不占槽 |
| 一次程序申诉/回应 | H | 所有合格主张并同一复核；不得第一人独占 |
| 两阶段投票 | 2N | phase×原名册位置；不可改票/借 ID 重投，初审/复核人选仍不重叠 |
| 两阶段资格排除/顺延 | 2N | 原排列每位置每阶段至多一次；不反复改资格收费 |
| 原随机 proof、两阶段冻结、两次结论、final/中性尾部 | 1+2+2+1 | 系统槽；条件不成立不写入、不收费 |
| 当事方撤回/合格和解事实 | H | 各方至多一次有变化的原案事实；和解仍需全部被改变权利人的同版接受 |

因此 C_case = 3H + 2HE + 4N + 6。H=32、N=64 时 C_case=1,382；K=16 时全部程序迁移上界为22,112。即使所有16案同时待复核、每方初审证据已用完，各自复核证据和原名册/尾部槽仍独立完整。

其他元数据上界 M = 1（activation）+ S（继受）+ H（整范围停新 fence）+ K×C_case + F（残余最终资金处分）+ H（原来源费用/返还分组）+ 1（最后 close）。最大 M=22,450。已经在 work graph 中进行的正常价款结算不再计成第二次实际处分；这里的 F 为尚未结清的最终残余保留。真正同根合批可以少调用，预算仍容纳分开合法执行；不能把一个槽要求拆成未计费的无限调用。

本程序允许每条原剩余权利在一次最终计划中明确实际可付量和 confirmed_unpaid/return_pending，而不是假称全部清偿。原 hold 足额且收款路径暂不可用时不消费最终槽、保持原费用；可安全实际支付时才提交。以后原债务人的自愿新增清偿见 §10，不以再次抽签/重新判责来恢复付款资格。

元数据真实 graph 为 g_m 时逐类乘其完整 node 上界；不能在 g_m>1 时仍把 M 当 invoke 数。作为可核的最大形状：各 metadata 为一制度调用，J=F=256、每 work graph64调用，则总预留=22,450+512×64=55,218，低于65,536。若新增合法节点使总数超界，必须在接受前重新选择真实可容纳的范围/图，不静默删必需程序或安全动作。

### 9.3 当前计量公式、真实 payer 和费用目的

沿实际 [incremental_module_fee](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)，u(x)=ceil(x/1024)。当前 schedule 的每节点上界为 D=u(I)+u(O_max)+2×effects_max+emits_max；E=1+effects_max+emits_max+1。实际收费使用完整真实 I/O 和实际 effect/emit/new_state。新目标的合计用 checked 算术，不依赖旧兼容代码 saturating 掩盖超限。

在完整 I/O 各256KiB、32 effects/16 emits 时为592 Data /50 Electricity；单服务 metadata 的1 effect/1 emit则为515 Data /4 Electricity。最大 one-invoke-metadata 形状的 module-only 上界为30,960,406 Data /1,728,200 Electricity；仅用全局65,536节点及每节点通用最大形状给出的更宽硬界为38,797,312 Data /3,276,800 Electricity。它们是可验算的工程界，**不是收款金额**。

每真实 artifact owner、原交易腿 payer、物理资源 owner 分开建立精确预算。activation 对当前余额合并 price、collateral、所有 fee purposes 与同一 source 的其他 hold；未来到款不能融资本次。原始付款资源、运费与计量费不可相互代替。额外 Kernel/native 操作费、实际 route tariff、原许可/生命周期/正式 producer 的所有必需调用、保管/容量费、原交易费及合法随机/名册来源成本都须由 exact 合法政策列出；明确免收也必须有覆盖该 scope 的权威，不能留一个无来源的“其他费用=0”。

| 子账 | 必须包含的费用/资源 | 不能挪用 |
| --- | --- | --- |
| price / collateral | 原 signed 净价、风险 collateral 与既有真实责任 | 不作为生产、module、程序或最后返还费用 |
| operational.normal | activation、S次继受及J个正常/修复图全部调用/费用；实际原材料/能源/容量另列真实来源 | 不消费别人的程序槽、关闭预算或未实现收入 |
| operational.procedure | 每 checkpoint、每 party/phase、名册位置及系统槽按完整 graph 预留 | 一方初审满额不能占复核或另一主体额度 |
| closure | H次停新 fence、F份安全 work plan、F次残余资金处分、H来源返还分组及最后close的真实图/费用 | 不能为普通超支补洞；最后费先进入同一候选，再释放该原来源剩余 |
| 原域物理/保管资源 | 各 work 的真实 inputs/sink、route/storage/terminal容量、停止/实际返程或合法有限保管 | current funds hold不等于物料存在；在途不能同时保留源可用量 |

费用持有量是各原 source 的最大合法向量，不按“总资产够”合并成无主余额。共同授权暴露 D/O/C 沿 [共同累计暴露](agent-mediated-contract-formation.design.md#contract-formation-cumulative-exposure)：真实业务支出将对应 O 转 D，真实费用将 C 转 D，确认解除只减 O/C，退款不减少 D 重置累计消费。原 grant 失效拒绝新风险，但已成立责任的狭义结清及其历史记账不复活该 grant。共享 source-purpose 的原资产只 hold 一次，多个需要同时成立的授权 cap 则都必须包含该完整请求。

### 9.4 小合同的可执行正向形状

独立方法 CSF-05 使用实际选择的 H=4（初始3方，保留1个后继）、K=2、N=6、S=1、F=4、J=4。三个原工作分别是受权工厂维修、10单位物料运输、一个已明确原结果的协作工作，第四个 normal attempt 是原范围修复余量。每个 work/safety plan 的真实完整图恰有两次服务调用，原物理 adapter 为明确获准的 native 物理效果且没有被隐藏的其他 module/validator；若实际绑定需要其他调用必须重报价，不能冒充这个形状。

此时 C_case=170，M=355，完整节点预留=355+8×2=371。若沿用通用256KiB I/O、1effect/1emit保守界，其module-only上界仍为191,065 Data /1,484 Electricity；这只作为宽界对照。小合同实际采用下列更紧的完整编码配置，所选source必须证明整个合法生命周期都在界内，不能只验证成功路径：

| 节点类别 | 完整input/output上限 | 其真实实例数与完整来源约束 | 当前module单次上界 |
| --- | --- | --- | --- |
| activation | 32KiB /32KiB | 1；fresh state≤128B、完整已内层编码action≤15KiB、ctx/wrapper≤1KiB；全部3principal/4代表签名与原证明必须实际装入 | 67 Data /4 Electricity |
| 普通工作及一般程序/关闭 | 64KiB /32KiB | 正常work8、一般程序84、关闭21；热state≤12KiB、完整action≤16KiB、ctx≤2KiB，所有原party/phase/claim/关闭状态保留 | 99 Data /4 Electricity |
| 证据追加与一次继受 | 128KiB /32KiB | 证据256、继受1；完整action≤48KiB，其余同上。每阶段16引用仍可各自独立追加；case/appeal只引用已注册root，不能偷加未预算证据 | 163 Data /4 Electricity |

紧配置的output保留≤12KiB raw state与完整≤6KiB effect/emit/wrapper，按外层数组最坏扩大也不超过30KiB，仍须实际serialize≤32KiB。input按相同真实外层估算：activation≤31.25KiB、普通≤58KiB、宽证据/继受≤122KiB，加短编码头后分别留在32/64/128KiB内。全部原16引用/阶段、每party机会及安全退出数目不变；适配source的完整证明若超过所选action界，必须在接受前选合法更宽配置并重报价，不允许运行后裁证据。

按此配置，normal=67+163+8×99=1,022 Data/40 Electricity；procedure=256×163+84×99=50,044 Data/1,360 Electricity；closure=21×99=2,079 Data/84 Electricity；合计 **53,145 Data /1,484 Electricity**。实例的合法manifest输出32KiB、1effect/1emit，命令输入的不同上限由同version声明；这项紧配置和authority仍需真实候选证明，不能把数学值声称为生产部署。

该例另明确买方600 Data净价（维修200、运输300、协作100）、供方30 Data collateral；测试候选的有权交易费政策为100bps、累计价款 basis600，完整正常价款交易费6 Data，另保护最多4 Data的有权取消/赔付费；原域报价给 normal/safe 物理与保管资源总上界40 Electricity（包括10单位正向运输、同量安全返程各10 Electricity，其他原工作及安全能力合计20 Electricity），以及各原材料/容量。不是从 fee hold 制造这40电或货物。测试 manifest 对其余列明 native/名册/随机/存储费用提供明确适用的零费权威；没有这份权威的真实世界须加入自己的准确非零 quote。

为便于守恒，例中供方同时是本实例实际 artifact owner，明确承担全部module fee与40电物理上界；程序权利仍归每个原 party槽，供方不能撤回已承诺预算。买方承担600净价和6交易费，供方另保护30 collateral及4 Data取消/赔付费。故激活前供方最低可用量为53,179 Data /1,524 Electricity，买方606 Data，另有真实材料/货权与第三方资产人同意。无新 native fee 的含义仅是该 fixture 的有权政策，不是当前部署事实。

成功路径只运行 activation、三项各begin/complete、最终close，共8个明确调用；资源供能的独立变体由CSF-01正向验证，不把形成active当交付。这8个完整wire shape在手册中独立实算为47+7×32=271 Data/32 Electricity，仅是规范尺寸计量，真正业务候选仍按各实际完整编码和真实结果收费；其余363个预留节点没有执行就不能收费。实际输入/输出也通常小于256KiB；手册以独立 canonical 编码给出可重算的完整 shape 和实际费，不把上述保守预算当扣款。未使用 normal/procedure/closure/交易费和collateral只能在各原权利消失后原路释放，释放不会加一份gross。

### 9.5 完整编码、热状态与内存

256KiB限制针对实际序列化后的完整 ModuleCallInput/ModuleOutput：所有 outer wrapper、context、state、action/envelope、签名、proof/ref、effects/emits、new_state均计入。现有 [ABI](../../../crates/oasis7_wasm_abi/src/lib.rs) 的 Vec<u8> 不保证作为 CBOR bytestring；内层 envelope/payload 再被外层编码时可能再次扩张，不能用raw长度相加宣称能装入。

服务热state的已编码原字节上限64KiB。普通操作的完整 action/envelope **经所有内层编码之后**最多40KiB；正文8KiB、verified-source/view32KiB只是其子上限，合计达到两者上限不保证40KiB成立。activation 可使用最多96KiB的完整 action、最多4KiB的初始state；context及其外层字段的完整编码最多8KiB。按外层每个u8至多2字节估算，普通路径2×(64+40)+8=216KiB、activation 2×(4+96)+8=208KiB，加短数组/map头仍小于256KiB；这只是预检推导，**最终仍实际serialize并以完整长度拒绝超界**。输出同样实际核完整封套，禁止把64KiB new_state直接当64KiB output。

所有合法调用必须读到本次判断所需完整真值。热state保留规则的紧凑typed值、单位/fragment/provider、case roots/每阶段计数、工作/fee槽与原件索引；完整16384 evidence refs、每个proof、票、原工作输入输出及历史不能全部内联热state。每案的party/phase机会与未决状态仍存在可验证的完整索引，不能只留一个“case摘要”丢掉未读主体。内部Hash32必须使用明确canonical byte-string的32字节编码（实现需显式codec，不能依赖默认Vec或64字节十六进制字符串）；外层state Vec仍按其真实ABI编码。最大热索引尺寸反例和正向见CSF-05。大原件按原durable存储分块，单读页≤64KiB、≤64条；一次权威验证选择最多32KiB完整已编码source view/证明，必要的额外真实模块调用全部计图。无法用原完整索引/有界证明判定的请求明确 pending_evidence/over_bound，不截断后作决定。

单次事务最多64个invoke串行准备；每个VM线性内存≤64MiB，不同时保留64个VM。input/output canonical缓存各≤16MiB，当前decoded树/节点≤32MiB，touched source/候选索引≤16MiB，scratch/allocator余量≤16MiB；合计硬上界160MiB，另保留32MiB运行余量，总192MiB/槽，2槽共384MiB。byte cap不能代替decoded cap：decoder在分配前检查node、nesting（≤32）、集合/字符串长度与实际allocated bytes；Vec/CBOR Value膨胀计入32MiB，不把wire大小当内存。root为受pin借用视图，不复制整个World或全量历史。超限/超时在publication前拒绝；publication status未知转§7，不能靠释放内存当回滚证明。

### 9.6 持久容量、全案恢复和读取上界

原已签执行最多65,536节点，每节点完整I/O各256KiB，原输入输出上界合计32GiB。全部16,384证据引用各自的完整服务proof bundle最多64KiB，合计1GiB；最多32,768个物理边界的服务原件各64KiB，合计2GiB。policy/terms/接受与初始authority bundle≤4MiB，热state/紧凑索引的每页仍按前述上限；全部metadata原件、请求结果、snapshots/tail、source依赖及存储开销一起按**每agreement唯一可达原件集合≤64GiB**做capacity预留和归档保留校验。上述各项不是允许在64GiB外再偷偷保留的免费数据，重复内容可按原存储合同共享，但可回读责任不得被共享去重删除。

为完整历史或维护区间引用原来源时，activation必须同时取得其准确window/index/retention承诺及有限record/byte closure；不能无界追溯整个World。每source writer仍拥有原完整原件。只保留service摘要而允许其所需source正文被GC，不满足本界。单项更大的原件可由原有完整分块/内容根证明接入，所有chunk与可达依赖仍计总集合；不能因单页64KiB便宣称整个源也只有64KiB。若实际组合装不下，在接受前明确更小可行候选、重新接受或拒绝，不能先承诺再裁掉某个案件/当事人。

运行中预留容量进入关闭水位则停新普通输入，保留预先计算的全部程序和安全收尾空间；真实外部source/基础设施失效不能靠丢原件解决，转原受阻恢复。空间预算是协议资源准入，任何游戏内存储费用仍须真实fee authority，不由此发明新收费。完整归档可减少热内存，不能将未结原件变为不可读的摘要；未结16案可在同一snapshot+tail恢复出完全相同的所有原party/phase槽、case/final/hold和受众读权。

只读页按world/instance/source_root、稳定cursor、当前读者/audience与cut绑定；每请求最多4页，每页64KiB/64记录，后续页显式继续。部分页明确partial/next_cursor/coverage，不能给“全无争议/全已付”结论。客户端与审核者可以在同一pin root依次读完；root变化按原continuation proof继续或重启，不混代拼接。只读不收业务费，独立rate/queue限制不得阻断该主体第一次合法停新/撤权请求。

## 10. 兼容、迁移、续存与已确认债务清偿

旧 native AtomicExchange、拒绝 Service 的入口、原 snapshot/event/replay shape 及旧两人 power v1保持原义。新合同必须带 explicit service schema/policy/instance activation，通过新 family executor；不扫描旧 Service/Open 自动导入 obligation 或把历史物理 receipt 补成有偿服务。旧客户端不懂的新状态只能返回明确不可写/升级提示，不能降级为两腿即时交换或显示已完成。

升级/停用选择首次 canonical inclusion 所在的当前合法 manifest。新版本必须能够解释原完整条款、已用 source/fee/case/历史主体，保留旧付款/保管/程序并提供同界或更严但仍可兑现的合法 drain；不兼容费用、payer、artifact或读权限变化不得先激活再让退出无限blocked。原context/签名不授予绕过当前manifest执行旧binary的权利。迁移是同根、当前合法且无新增未同意风险的真实操作，计入明确图/预算，不洗掉数额/责任或将旧case当新案。不能满足时先依法结清或取得准确新增接受；读旧记录本身不意味着旧写入可恢复。

confirmed_unpaid 是已经成立而尚未支付的原债权，不是新贷款。初次有权最终处置支付当前合法可付部分、保留精确余量/原 liability cap/债务人/受益人；程序关闭不删除它。此后原债务人或依法有权处置其原责任财产的主体可提交 SettleConfirmedServiceClaim：引用原决定/claim、已付累计值和 expected version，声明当前真实资金及本次完整 graph/fee cap。金额必须为正且不超过原尚未清偿量；所用资金与本次费用须当前合法明确授权，不借旧失效 mandate。

该清偿不要求债权人重新放弃权利或签一个新服务，不重开原证据/期限/随机/履约单位。仍由当前受权服务 L2 drain 计算允许量，在同一事务核旧claim/source、消费当前真实资金/费用、增加原paid、减少unpaid、追加parent-linked settlement receipt/新source-use及原授权暴露更新；改变收款人或承担人仍须相应明确合法权源。原请求精确重放零新费；同债权并发两笔按当前余量串行核对，任一会超付则整项拒绝，不能从两个旧余额各付一次。

每笔新增清偿最多64个完整调用和一个确定的资金处分图，自己预检完整编码/资源/存储，费用由当前明确有权payer承担；旧债存在不产生无限免费调用或免费存储。其原件与旧claim链必须一同可恢复，旧聚合只追加不重写。原初始预算关闭后不能从其他原当事人的price/closure池扣这笔新费，也不能因为新费无人愿付便宣称债务已清。组织清偿沿原顺序调用同一adapter；不另造全局债务、成员连带责任或资产搜索权。

## 11. 验证策略与可读结果

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [义务模型](#service-obligations) | 原单位/来源/数量与实际支付独立守恒，换请求不重用 | [CSF-01](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-01-domain-results)；固定四类真实adapter候选，独立物理/财务基线及源去重反例 | 原source、partitions、hold/paid/basis及同一receipt | 未接线代码、生产能力或其他未采纳服务 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [主体与责任](#service-parties-liability) | 组织/多方真实权利、因果和清偿顺序；无隐式连带或成员资产 | [CSF-04](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-04-parties-succession)；多人/组织及当前来源对照，真实estate/claims adapter候选 | 全部必要同意、来源清单、逐方损害/paid/unpaid | 现有adapter_unavailable不能由手册代签 |
| [AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [领域证据](#service-domain-evidence) | 真供能、真维修/完整维护窗口、真运输及原协作结果，前序不冒充完成 | [CSF-01](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-01-domain-results)；正向/缺接收/缺区间/失败实验/重复source | 前后物理账、已发生outbox和完成边界 | 不证明尚无授权的物理动作 |
| [AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [范围恢复](#service-scoped-recovery) | 部分修复、真实取消/返程、最小保全，已做不回滚 | [CSF-02](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-02-scoped-recovery)；并行争议/无关义务及四恢复分支 | 原损失/费用、范围闭包、合法下一步及无关成功 | 物理不可达或基础设施失效不保证即时收尾 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [本地复核](#service-local-review) | 原权限/随机/完整证据、固定半开时界和final guard | [CSF-03](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-03-review)；有/无责任独立证据、初审/申诉/尾部组合 | 选择proof、票/排除、有效决定、paid vs pending | 不证明一般法院/其他领域panel管辖 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [继受与控制变化](#service-succession) | 当前完整必要接受、真残余/资金、旧责保留和一次交接 | [CSF-04](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-04-parties-succession)；17bundle/旧未付/未知在途/旧履约竞态 | source/acceptance/provider段、同根hold及拒绝 | 不自动转credential/成员资产/第三方责任 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [资源与预算](#service-resource-budget) | 所选完整调用/物理/来源、全H/K程序、关闭和编码可容纳 | [CSF-05](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-05-budgets)；小合同全正向、16案/最大形状、满槽与编码反例 | independent budget/CBOR/ledger、实际meter及关闭 | 不证明硬件吞吐、生产经济平衡或真实fee授权 |
| [AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [持久恢复](#service-persistence) | 三态根决定、全World fence、原物理/费用/责任一次且完整原件恢复 | [CSF-06](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-06-recovery-read)；各真实failure cut、双快照/tail/多案分页 | exact root/原operation/费用/原件、恢复后全部槽 | 静态推演不代签未运行故障域 |
| [AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [结果与来源读取](#service-read) | 三表面同源、有效责任/付款/当前scope与受众准确 | [CSF-06](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-06-recovery-read)；原方/后继/旁观者与真实用户解释 | 来源投影、当前权限及玩家对下一步的独立复述 | 无玩家/正式UI/API运行不能宣称体验完成 |
| [REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [激活接缝](#service-activation) | 完整共同形成同原L2调用激活，无未绑定费用/义务 | [CSF-04](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-04-parties-succession)；精确条款/所有必需方、漂移/缺资金、成功及重放 | family/common原件、完整graph、唯一activation | 不代替CR001独立方法或原governance激活 |
| [AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002)；Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d | [运输与托管](#service-transport) | 真实cargo守恒、接收/货权与资金分层，部分履行/损耗/返程不免费回滚 | [CSF-01](../../testing/manual/continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-01-domain-results)；固定真实运输adapter/route/仓储/terminal，按2.2独立核物理和资金，联动同手册CSF-02原取消/返程对照 | 原lot/route/custody/title/terminal receipts、单位账与价款/fee差分 | 原物理能力、正式跨区运行与故障域未执行不能由静态例代签 |

<a id="service-read"></a>
### 11.2 ServiceOutcomeSourceV1 与恢复后的表面

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

同一root的正式Service/API、Viewer与Agent返回相同核心事实：原条款与当前执行binding、各角色/主体/代表任期、原检查点时界、各unit的未开始/执行中/performed_unsettled/完成/处置、精确物理loss/custody/return、价款/hold/费用/paid/unpaid、case/初审/申诉/复核/当前final及其来源、保全闭包、原请求查询键、blocker、合法next_action/next_recheck。action建议只使用当前真实授权；客户端知道due不等于提交终止。

向CR003提供的 ServiceOutcomeSourceV1 必须包含：

- world/branch、exact instance/schema/policy/artifact与原activation/terms、canonical root/tick/finality；
- original agreement/obligation/checkpoint/unit/effect/receipt及lot/range、原producer/type和完整coverage；
- 稳定责任principal、service role、organization、当时representative/mandate/controller/Agent控制任期；原/后继provider的精确责任段；
- 原已完成/未满足/真实损害/实际修复或请求中修复、接收/配合/其他阻碍，避免把结果和因果混为一项；
- 初审provisional、有效申诉、review、executable_final/invalidated、中性尾部，及实际award/paid/confirmed_unpaid/return_pending；“有申诉”不自动删除有效事实；
- 原source修正/撤销/替代关系、current validity、源水位/分页coverage、对该读者的audience、授权用途与披露限制。

共同条款的 source_projection_use 是完整接受正文与activation receipt的一部分；它只许可其中明确情境/用途/受众的来源投影，不自动允许公开私密证据、跨情境评分或把组织责任变个人终身评分。来源权限与真实性分开：缩小披露不删除原债务/正式责任，允许读取也不使due、缺席、余额不足或申请指责变成default。CR003可以表明pending/范围未知，但不能重新裁决服务或生成服务没有的final。后继得到其新角色可读的必要事实，不继承旧个人credential或完整私人记录。

party读自己的原正文/付款/责任所需完整事实；原方即使已退出仍保留相应历史读权，不获得新方私有余额。公开读者只获得当前政策允许的有限来源摘要；一个隐藏的关键cause/coverage不能被摘要为“已确认其有错”。source index全页可读才可声称完整；有访问限制明确为partial/unavailable，不作全量负债清零证明。

### 11.3 证据分层与审读结束条件

静态正文/算术/编码形状核对只证明目标合同一致、上限可算和该shape可编码；不证明真实sandbox、收费、治理、panel、跨节点存储或玩家理解。实现验收固定同一source/构建/manifest/authority/environment，按手册记录真实输入、原件与差分。各方法必须含可成功正向和不相同的反例，不能用实现产出的布尔“pass”再证明实现。

## 12. 决策、风险与未完成边界

本设计采用共同有限服务状态加原领域真实结果的结构，原因是原 CR002 的维修、运输、协作及组织/多方都需要同一套单位、责任与恢复，而具体维修量、货权、R&D贡献和物理损耗已有主责。共用服务程序不取得其他领域权利；把费用/退路在接受前列全，可以在正常工作失败时继续处理已有权利。

| 剩余实施或运行依赖 | 主责与处置 | 真正结束条件 |
| --- | --- | --- |
| 四类完整 source/work/disposition adapter、raw/due/无module路径和组织清单 | Runtime及原物理/工业/RD/组织owner；缺项不可激活 | CSF-01/02/04的正向与绕行反例覆盖全部实际消费者 |
| 通用资金/物料保护、逐node真实payer与同根费用/暴露/原件 | Kernel/institution/CR001；没有完整来源不报价成功 | source合并守恒、满槽关闭、编码/内存/retention及实际计量证据齐备 |
| 有效local-service管辖、名册/随机及finite程序 | gameplay/governance/P2P；不借已有其他域panel | 同一真实authority下的CSF-03及tail/unknown结果可核 |
| 持久根恢复、完整source留存与正式三表面 | Runtime/Service/Agent/Viewer/QA | 16案全原件及双快照/tail/真实玩家复述完成 |
| 生产费率、硬件容量和可用性 | 对应合法owner；示例不代调参或承诺SLA | 真实候选的计量与经济/性能观察，不以保守数学界代替 |
| 新规则/全局法院/自由文本合同/信用或外部资产 | 不属本次原采纳范围 | 如未来采纳再经原产品/专业authority；不能偷偷塞进repair或源码旁路 |

本次设计关闭的是原采纳服务全范围的具体政策、执行接口、有限责任/救济、费用、继受与恢复缺口；旧实现状态和历史证据不被抬高。设计完成以四类+组织多方完整义务、CR001/CR003精确接缝及独立非作者审读成立为条件；实际业务完成还须真实实现、合法激活、运行和玩家证据。
