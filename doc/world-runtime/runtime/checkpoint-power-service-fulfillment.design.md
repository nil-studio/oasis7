# 检查点供能服务履约与有界恢复系统设计

- 生命周期：`active`；本文定义目标技术合同，不表示实现、治理激活或运行验收完成。
- Owner role：`runtime_engineer`；玩法、责任与争议政策由 [供能服务玩法合同](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-scope)唯一维护。
- 固定审读基线：`nil-studio/oasis7@0a967845ba8e0bb33331a91008d2e7471eec1ece`。§3 是该基线的静态事实；下述类型和命令是逻辑目标，不是现有 Rust、HTTP 或 ABI。
- 上游：[CR001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001)、[CR002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)；验证入口：[供能服务验收手册](../../testing/manual/checkpoint-power-service-acceptance.manual.md#ps01-formation)。

## 1. 问题、目标与非目标

即时交换在一次提交内完成；跨时间供能会留下未履行范围、资金占用和责任。本文为 `owner_checkpoint_power_service_v1` 明确一个可实施的有限切片：两个不同个人当前 owner 的两个 Agent，在同一 canonical world/branch 约定 2–16 个检查点，以整数 Electricity 单元供能、以 Data 付款并提供 Data 抵押。价格、数量、范围、期限和费用上限均经明确签署。未来供电不预支，不把获得电力的预期当作当前余额。

每次交付可以只完成检查点的一部分，但该部分的供电、付款、对应抵押释放、全部费用与 receipt 必须原子提交。未完成部分可以修复、终止、最小争议保全或显式更换供方；同一单位不能因换请求、修订、继受或重启再次交付、付款或获得抵押。

本切片部分承接 CR001/CR002，不覆盖组织签约、多人合同、物料运输/目的地保管、设施维护、信用借贷、外部付款、一般法院或所有持续服务。旧 native `Service` 不因此变得可用；不选择 SC-32 pilot，不新增制度专属核心 `Action` 或 `WorldState` 顶层字段。

原采纳的组织、多方及维护、运输、协作已由[完整持续服务合同](continuing-service-fulfillment-and-remedies.design.md#service-obligations)承接，普通形成复用[通用合同形成](agent-mediated-contract-formation.design.md)。本文仍是独立的双人供电版本，保留原 terms 域、费用图、单位账本、裁决与继受上限；新服务 family 不得通过该版本绕过所需接受、实物来源或责任条件。旧版本的历史和未结义务按原条款继续，新版本采用须有其本身的当前能力、授权与同版接受，不按文件更新自动迁移。

## 2. 上游约束与相关角色

业务政策来自 [条款](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-terms)、[检查点](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-checkpoints)、[恢复](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-remedies)、[争议](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-disputes)和[继受](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-succession)。本文固定这些规则的身份、权限、资源、事务与恢复接缝，不另设价格、罚金、期限或抽样规则。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) | 提议和同版接受无业务效果；显式激活才建立有限义务及足额占用 | [形成](checkpoint-power-service-fulfillment.design.md#service-formation) | gameplay 条款、当前 owner、零费用协议登记 | 组织签署、通用持续服务；不将旧 Open 当作 offered |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002) | 本金、抵押和费用占用真实余额；仅有权消费者可处分，其他扣减不能侵占 | [资源占用](checkpoint-power-service-fulfillment.design.md#service-resource-holds) | Kernel 通用资源适配器、所有权与受保护 schema | 当前尚无完整通用 hold 实现；不创造第二份余额 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002) | 明示实际 payer、所有费用和有界次数；操作预算耗尽不消费闭合预算 | [费用预算](checkpoint-power-service-fulfillment.design.md#service-fee-budgets) | 合法 fee authority、WASM metering 与治理 drain 兼容 | 不新设生产费率，不许缺 owner 免费执行 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002) | 部分检查点按不重叠单位结算；due 无罚款、每单位一次效果 | [检查点结算](checkpoint-power-service-fulfillment.design.md#service-checkpoint-settlement) | 世界时间/位置、签署价格、Data 权限和原子资源发布 | 非共址输电、物料/外部交付 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002) | 修复仅剩余范围，终止区分已履行、未履行和已判责任；续期不洗掉历史 | [恢复动作](checkpoint-power-service-fulfillment.design.md#service-recovery-actions) | gameplay 恢复/责任政策、完整 canonical 证据 | 无证据的自动 default、任意补偿或隐式续期 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002) | 保全精确争议单元，未争议义务可继续；有权裁决及固定尾部处置不清除有效责任 | [争议权威](checkpoint-power-service-fulfillment.design.md#service-dispute-authority) | 有效本地名册、可验证随机来源、最终性、程序复核 | 当前组织救济/社交裁决不自动成为该 panel |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002) | 原供方、买方、新供方同版接受；只交接可证明剩余范围，旧责任与资金不静默转移 | [显式继受](checkpoint-power-service-fulfillment.design.md#service-succession) | 当前身份、合法执行/fee binding、组织连续性边界 | 买方替换、组织/多方继受或 credential 转让 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002) | 原请求/业务单位去重、同根持久化、unknown 和重放保留占用及责任 | [持久化与恢复](checkpoint-power-service-fulfillment.design.md#service-persistence) | 统一 ExecutionTransaction、G1/G2/G3、authority profile | 本文不代签实际故障域或跨节点运行 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002) | 各入口同源区分已交付、待履行、待裁、待付款与可做下一步 | [可读结果](checkpoint-power-service-fulfillment.design.md#service-read) | Service/Viewer/Agent 受众投影 | 实际 UI、正式 API 和真人理解度 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) | 未激活/缺适配器时不建立服务义务；旧协议、原件与合法退出版本可恢复 | [兼容与启用](checkpoint-power-service-fulfillment.design.md#service-compatibility) | Native 分类、governance activation、受保护资源适配器 | 不选择 SC-32 pilot，不增加原生 Service 语义 |

## 3. 当前状态、目标状态与差距

以下仅核对固定基线的直接源码和设计，未运行 Rust、游戏、故障注入或玩家验收。

| 对象 | 当前事实 | 目标及具体差距 | 依据 |
| --- | --- | --- | --- |
| native EconomicContract | `Open/Accepted/Settled/Expired` 与单腿结算字段；Open/Settle 生产入口拒绝 Service，expiry 只处理 AtomicExchange | 本文的检查点、占用、裁决和继受是新目标，不能给旧 enum 补语义 | [状态](../../../crates/oasis7_world_state/src/gameplay_state.rs)、[入口](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_policy_contract.rs)、[到期](../../../crates/oasis7_runtime/src/world/gameplay_loop.rs) |
| 即时合同形成 | 已有明确双签、零效果接受、两腿同次结算与原件恢复的目标设计 | 复用形成/摘要/恢复规则；即时 profile 仍不承接未来义务 | [即时形成设计](agent-mediated-contract-formation.design.md#contract-formation-terms) |
| 资源转移 | 普通转移检查正数、Agent、canonical 共址和 Data 方向授权；余额为 Agent ResourceStock | 新 hold 必须约束全部扣减、费用与所有权变更，不能只在服务入口检查 | [资源入口](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)、[余额接口](../../../crates/oasis7_runtime/src/world/resources.rs) |
| module 费用 | 从 artifact owner Agent 的 Data/Electricity 余额扣除，入世界 treasury；预检与实际计费分别计算 | 必须同时支持受保护可用量与精确 fee hold 消费；缺 owner 的跳过分支不是免费授权 | [metering](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)、[算术](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs) |
| 可复用恢复/争议 | 跨区设计已有里程碑去重；组织救济有特定权利与无冲突校验 | 它们不提供本服务的随机本地 panel、价格/抵押政策或通用 hold | [跨区结算](cross-region-delivery-and-escrow-settlement.design.md#des-wr-cr-002)、[组织救济校验](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_helpers.rs) |
| module namespace/组件 | 目标允许 versioned instance/component；完整通用 component 存储和部分命令实例寻址仍不是已证能力 | 需要受保护 schema、索引、校验与恢复，不能把任意 module state 数字当托管 | [WASM 专业合同](../wasm/wasm-interface.md) |

## 4. 边界与结构

| 组件 | 负责的真值和写入 | 不能代替的权威 |
| --- | --- | --- |
| gameplay policy | 供能义务、单位价格/抵押约束、恢复窗口、责任条件、panel 和有限退出规则 | 不直接更新资源或判定当前代码可用 |
| 服务 instance | 条款修订、接受、单位履行/处置、case、明确继受及其因果关联 | 只能提出受限候选，不能自行铸造余额、grant、费用权或最终性 |
| Kernel resource adapter | gross、可用量、受保护 hold、狭义处分权、全路径守恒与 typed delta | 不把 module 写的金额视为资产；不裁定一般服务质量或外部法律 |
| World Service/admission | 身份、完整输入/签名、有限协议记录、同一执行入口和原件查询 | HTTP ACK、缓存或客户端签名不代替世界提交 |
| registry / panel adapter | 验证本地事项授权、名册/抽样、回避、签署结论、完整案件版本 | 不能从普通组织角色、社交声誉或操作者身份推导裁决权 |
| 持久化与 P2P | canonical 顺序、generation、恢复闭包与既有最终性证明 | 本地文件落盘、module 执行返回不单独证明 committed |
| Viewer/Agent/API | 经受众授权的同源条款、事实、费用、责任和下一步 | 不生成缺失裁决，不把排队、due 或 unknown 显示成 fulfilled |

资源占用属于 Kernel 的通用资源守恒/权利约束；供能条款、履约和责任属于 L2 institution。按 [Kernel/Institution 边界](../prd.md#native-surface-分类与冻结规则)通过受限 intent 和原有根事务连接。目标记录可使用受保护的 versioned instance/component schema；它不是当前已有的通用存储，也不是制度专属 `WorldState` 新字段。模块一般状态写入和升级均不能绕过 Kernel 对 hold schema 的验证。

## 5. 关键运行流程

<a id="service-formation"></a>
### 5.1 非绑定形成与一次激活

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

1. **只读比较。** 返回当前身份、位置、费用来源、每检查点价值、未预存电力的履约风险、全部占用和无合同替代。只读查询、草稿、offered、两方 accepted_pending_validation 不预留、扣款、收费、占用游戏额度、改声誉或建立义务。
2. **同版接受。** 复用 [不可变条款/确定渲染](agent-mediated-contract-formation.design.md#contract-formation-terms)和[当前签署授权](agent-mediated-contract-formation.design.md#contract-formation-authorization)。使用本 profile 的新版本正文；`record_version` 不进入 terms digest，签署后改数量、价格、期限、预算、处置或执行绑定均使接受失效。
3. **显式 activate。** 任一当前当事 owner 提交两份接受。提交点重验 world/branch、instance/activation、身份来源与 generation、双方完整条款、时间、数据权限、实际 fee payer、所有可用余额和治理依赖。一次建立检查点、各 hold、费用预算、狭义处置权、原请求结果和 active receipt；任一失败全体无业务效果。
4. **履约与观察。** 供方主动提交原单位区间，按 §6.4 原子交付。到期条件只由 canonical tick 派生；`checkpoint_due` 不单独扣款或判违约。不存在按本地时间执行的自动付款。
5. **恢复与终结。** 已完成单位保留；只对剩余范围执行修复、终止、争议或继受。闭合须核清全部单位处置、已判债权、占用和费用；终止服务不等于未付债权已付款。

新的绑定键为 `(world/branch, institution instance, agreement_lineage)`。激活一生最多一个赢家；修订和换 request 不重新建立足额占用或重置时限。形成协议的无业务费用路径必须实际成立，普通收费 module 命令不能代演接受登记。

## 6. 接口与数据合同

### 6.1 版本、字段与原件

编码、Hash32、签署域隔离和无环摘要顺序沿用 [WASM canonical 合同](../wasm/wasm-interface.md#sr2-execution-contract)。本 profile 分别使用 `oasis7/power-service-terms/v1`、`oasis7/power-service-acceptance/v1`、`oasis7/power-service-request/v1`。先由不可变 TermsBody 得到 terms digest，再渲染正文/计算 render digest，随后产生接受证明，最后计算请求摘要；各对象只排除自己的派生 ID、签名及会造成循环的 containing-block wrapper。整数全部 checked；不 trim/casefold 身份，不接收任意文本义务或浮点金额。

`record_version` 仅不进入不可变 TermsBody/terms digest，使登记接受不改变条款。命令的 `expected_record_version`、其他 CAS 前置、完整操作内容和接受/授权证明引用必须进入 request payload digest；修改 CAS 条件后复用原 request ID 必须 `request_conflict`，不能用条款摘要相同吞掉不同请求。

| 记录 | 必备逻辑内容 | 约束与 consumer |
| --- | --- | --- |
| `PowerServiceTermsV1` | world/branch、profile/schema、instance、agreement lineage/revision、两方 owner/Agent/current authority refs、服务地点、2–16 个检查点的序号/窗口/`q_i,p_i,c_i`、共同 `Q`、时基说明/来源及全部绝对截止、费用与执行绑定、政策/模板 hash、有效期、狭义处置权、争议名册和随机来源绑定 | 双方读取完整确定正文；条款和政策共同确定所有经济后果；动态 CAS 不改变摘要 |
| `ServiceUnitRangeV1` | `(agreement_lineage, checkpoint_index, [start,end))`，`0 ≤ start < end ≤ q_i`；交付/未履行/责任状态、原 receipt | 单位索引不会因修订、case、继受或续期复用；所有提交范围必须规范化、排序且不重叠 |
| `ResourceHoldV1` | 见 §6.2；原资金主体、账户/资源、purpose、单位 scope、remaining、version、授权来源和允许处分 | Kernel 验证的占用，不是 module 可随意改写的余额；引用必须在同一 canonical root 可回读 |
| `ServiceDispositionAuthorityV1` | 激活时双签的 hold 集、最大金额、原受益主体、允许触发事实、操作种类、政策版本、不可转授范围及历史 authority | 只可处分已明确占用的资产；不能追加扣款、使用普通余额或生成一般 Data grant |
| `ServiceFeeBudgetV1` | 逐费用项的合法 payer/source：交易费适用政策及 fee leg、实际 artifact owner 的 Kernel/module 项、artifact/instance/schedule、每方操作/闭合各资源上界、命令上界/剩余次数、程序阶段及 actor slot、hold refs、drain 兼容条件 | authority 验证真实来源和实际费用；artifact owner 不自动成为所有交易腿的 payer；个人程序份额不可被其他 actor 消耗 |
| `TradeFeeAccumulatorV1` | `(agreement_lineage, checkpoint_index, fee_leg)`、适用 policy/hash、累计净业务 basis、已计交易费、原 receipts 和 rounding 状态 | repair/拆分/换 request/继受不重置；一个新版本标签不能清零旧累计量 |
| `CheckpointDecisionV1` | 单位范围、完整证据 cut、结论种类/理由、作出主体和授权、case/version、最终性/程序状态、可执行资产处分 | `due`、请求 ACK 或外部叙述不能作为责任结论；付款事实另记录 |
| `SuccessorAgreementV1` | 原/新供方、买方、原单位范围和全部旧责任、三方同版接受、新执行/费用/抵押来源、旧留存范围、expected versions | 同一 lineage 中明确交接；不移动个人 credential 或清理历史 |
| `ServiceBusinessReceiptV1` | 原 request、terms/policy、unit ranges、事实 root/tick、authority refs、全部 gross/hold/treasury delta、费用、结果/责任、下一步、execution receipt 引用 | 业务 core 与最终性 wrapper 分开；同根提交和历史读取保持一致 |

<a id="service-resource-holds"></a>
### 6.2 受保护占用与狭义处分权

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

每个 Agent/资源仍只有原来的 gross 余额。`available = gross − Σ active_hold.remaining`；hold 是该余额的受保护占用，不能把它再加进资产总量。按 Agent/资源建立同根承诺的占用索引，索引只是派生量；来源缺失、混代或无法证明完整时返回 `hold_coverage_unknown`，不能假定占用为零。

`HoldRef = (world/branch, instance, agreement_lineage, purpose, checkpoint/unit_scope, original_fund_owner, resource, hold_id)`。`purpose` 只能是 `price`、`collateral`、`operational_fee`、`closure_fee`；记录原账户、原 controller identity/source/generation、初始量/remaining、版本、消费者及可处分谓词。不同 purpose 不相互借用。价款与抵押按单元可分配，费用按执行绑定分开；交易费也必须有显式来源和足额占用。

| Kernel 操作 | 原子 delta | 所需授权 |
| --- | --- | --- |
| `reserve` | 增加 hold，gross 不变；同账户其他 hold 与本次费用合并后仍有足够可用量 | 当前 owner 对准确资金/金额/purpose/scope 的明确接受；合法 activation |
| `release` | 只减少 hold，不增加 gross，不伪造 refund credit | 原单位完成或合法终止/处置事实；释放只能恢复原资金权利人的可用量 |
| `consume` | 同次减少指定 hold 与其 gross，向准确受益方或 treasury 增加同量；本 profile 不支持 burn | 已提交狭义处置权、未消费单位和当前可验证触发事实；不得扩大到其他 hold/普通余额 |

同一 root 的 preflight 和最终 publisher 都必须使用受保护可用量。普通消费不得先按 gross 准入，再把预期的占用冲突变成整个 tick 的提交失败；应沿该操作原有拒绝/审计规则报告可用量不足，不取得新的队列优先级，也不侵占受保护金额，其他合法操作仍可继续。本适配不删除原审计/事件或改变普通操作已有的合法 attempt/metering 费：若原政策适用，该独立费用仍只能来自其已授权可用量并保持原 scope。本服务 profile 的确定拒绝、非绑定和精确重放才适用 §6.3 的零新业务费用约束。最终仍强制 `gross_after ≥ Σ held_after ≥ 0`，验证每条 hold delta 的授权、scope、前版本和累计量；此层发现不一致属于不变量故障，不能悄悄丢弃 hold 让提交通过。

适配范围覆盖普通/native/direct/trusted/模块转移、通用执行费、直接扣减接口、所有权转让、删除和 claim 回收。回收 Agent 资格不抹去原资源账户、费用来源或已成立权利；有未结占用时保留可验证结算身份，不能复活 Agent、转移个人权限或修改既有 MainToken upkeep 政策。无法保留原权利的资产处分不得发生。模块一般 state 写、升级、deactivate、快照恢复或丢弃索引均不得使占用消失；缺任一可绕过的扣减/处分通路时禁止激活本 profile。

普通 Data transfer grant 仍可撤销，并约束普通新转移及服务的新输入。它不取消激活时已成立的 `ServiceDispositionAuthorityV1`：双方明确授权 Kernel 在指定单位交付、有效责任裁决或中性处置条件成立时，从已占用价款/抵押向指定受益主体结算。该权利不能由 module 自授，不允许扩大金额、收款主体、世界或触发条件；撤销普通 grant 不使已确认债权消失，也不授予新的普遍 Data 访问权。

新交付仍重验供方当前控制、位置和签署范围；价款/抵押处置检查原狭义权利及当前事实。控制权或 credential 变化不把旧权利移给新 owner。通用 ownership adapter 必须先证明负担及原受益权能够完整保留；本 profile 不支持的控制权处分原子拒绝，直到先完成合法结清或 §6.7 的显式继受。同一人的合法 credential 轮换按身份连续性证明恢复读取/请求权，旧密钥不能继续新签署。

若已合法发生的控制变化仍使本次原受益主体的可用账户无法安全解析，释放/付款保持 `return_pending`，保留对应 hold 与债权，不能把解除占用后的资金暴露给新 owner；无效果重试不消耗闭合预算。恢复原主体合法结算路径后才按同一原件提交。

<a id="service-fee-budgets"></a>
### 6.3 操作预算、闭合预算与 drain

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

fee quote 必须包含适用交易费及强制 Kernel/module metering，分别给出权威来源、资源、接收账、实际 payer、schedule hash、命令上界和 signed cap。实际 module payer 必须由当前 governing manifest 所选 artifact 的 owner authority 解析并属于该范围已签当事方；各交易腿的 payer 由其合法 fee policy 独立决定，不能一律改成 artifact owner。不得指定未签第三方、使用缺 owner 的免费分支或把抵押当执行费。当前 [预检/计费入口](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)尚未支持本节完整 hold 合同。

激活按每个合法 payer 分别建立 Electricity/Data 的 operational 与 closure hold，内部分开记交易费和 Kernel/module 计量的来源与上界。对每种可接受命令，按合法 [metering schedule](../wasm/wasm-interface.md#sr2-metering-contract)、input/output/effects/emits 和适用访问计数的确定上界计算 `fee_upper_bound`；乘以允许成功次数并 checked 求和，再加入该类交易费的累计最大额。条款披露该向量及命令次数，费用 quote 不可只报本次单个调用。总持有量须覆盖激活本身、全部允许的交付/恢复/争议操作，以及保留的终止/判付/释放路径。实际成功按真实计量消费，余量不按上界直接收费。

#### 6.3.1 程序机会与费用分槽

operational 不能是一只所有人共用的次数/金额池。程序子预算键为 `(agreement_lineage, checkpoint_index, phase, actor_slot)`，另绑定实际 payer、资源和合法费用项；actor slot 是程序权利人的稳定身份位置，不等于付款账户或当前 provider 字段。下表的次数是每个检查点的最大合法成功写入数，重复、无变化、无权或超份额请求不收费、不占他人槽。输入字节、证据引用和每个合法写入的计量上界仍按 §9 校验。

| 程序子槽 | 有界成功次数和消费者 | 隔离要求 |
| --- | --- | --- |
| 当事方受理/争点 | 每个有责 actor slot 至多 1 次登记或把自己的具体争点并入原 case；全部指向同一 case | 只消费本人份额；不得建立第二 case、重抽或改变固定截止 |
| 当事方证据 | 每个有责 actor slot 在初审、程序复核各最多 16 个不同权威 receipt 引用，各按最多 16 次追加写入独立预留；批量提交可少用次数 | 引用只追加去重；相同引用、无新权威事实或仅反复改说明不产生新写入/费用；case/申诉内引用分别计入对应阶段总量，不能借换命令突破，初审用满也不侵占复核机会 |
| 程序申诉/回应 | 每个有责 actor slot 至多 1 次合格程序主张；本案只建立 1 轮复核，后到合格主张并入同一轮 | 不以先提交者独占复核；不延长原窗口，不动另一方的机会或预算 |
| 初审/复核投票 | 各阶段分别按固定名册的 `N` 个位置预留至多 `N` 个不可改写有效投票，`N` 为该案获准名册长度 | 同时在任席位、回避与门槛由玩法政策决定；某候选的退出/失格不能消费另一阶段或其他候选的投票槽；同人不能借新 ID 重投 |
| 选人/补位 | 原 case 的随机证明接受至多 1 次；初审与复核各按最多 `N` 次候选资格排除/顺延写入预留 | 一个名册位置被合法排除后不在本案重新入列；资格未变化不写入；不得用请求者可选重抽补位 |
| 程序冻结/结论 | 初审 scope/证据 root、复核程序主张/证据 root 各冻结 1 次，初审结论和复核结论各至多 1 次发布；final 与资产处分归受保护 closure 槽 | 不允许当事方直接消费这些系统槽；不足 quorum 不制造结论，也不通过重复尝试收费 |

上述 `N`、证据数、单案与单轮约束均来自[唯一争议政策](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-disputes)，并非增加成员或申诉机会。manifest 的结构上界必须容纳完整合法程序，不能把较小通用调用 cap 用来删掉已承诺的举证、投票或申诉机会。激活必须按每个分槽的次数乘以对应真实计量上界足额保留，供能、repair、另一 actor 或另一阶段均不能借用；任何一个总额或分槽不足都拒绝激活。资金仍只是原余额中的 `operational_fee` 受保护分项，不另建资产。

provider/actor 槽总数由原 manifest 与双方签署的有限继受 cap 固定。初始预算按获准最大槽位和写集计算；尚未启用的继受槽不能自行发起程序。继受只有在该已有槽位内、以新的同版授权补足或替换准确合法 payer/source，并保留旧有责主体分槽时才可提交；不能用换人增加槽、重置案例计数或挤占买方和旧方预算。新的资金/执行绑定不能降低已承诺的阶段机会或闭合能力。

#### 6.3.2 交易费与执行费用

交易费必须有覆盖本 profile/业务腿的合法适用政策；明确免收也要有 scope/version 依据，不能因缺 policy 或旧公式不适用默认零费。`TradeFeeAccumulatorV1` 在每个检查点/fee leg 保存累计净业务 basis 与已计费量。本次交易费为 `fee_total(basis_after) − fee_total(basis_before)`；获准的 bps 政策使用 `floor(checked(basis × bps) / 10000)`。供电腿 basis 是已交付 Electricity，价款腿 basis 是已付 Data；抵押赔付若适用费用，使用独立 leg 和 closure 子预算。所有交易费在净价款/抵押之外收费，不缩减收款方已签净额。

同政策下拆分、repair 和整段交付达到相同净数量时，累计交易费相同；真实成功调用次数不同可使 metering 不同。换请求、修订、继受或执行版本不能重置累计 basis/rounding。未来不兼容费用政策变更必须先给出合法且明确接受的迁移/结清方案，保留旧累计量和已付事实，不能以新 policy ID 从零计算或追溯补收历史费用。

| 路径 | 费用来源和约束 |
| --- | --- |
| query、quote、非绑定提议/接受、due 观察、确定拒绝、精确重复请求 | 零新业务费；不因读取/拒绝调用收费 sandbox，不消耗游戏额度。协议 CPU/存储防滥用限制独立且不产生游戏权益 |
| 激活、合法交付、原范围修复及有界 case 操作 | 只消费准确 operational fee hold；实际 fee 与业务写集同一次根提交，失败全体不收费 |
| 最终责任判付、合法终止、未消费占用释放和终态 receipt | 仅按已声明闭合动作消费 closure fee hold；上界覆盖最大合法剩余状态，不能借给交付、扩约或重试 |
| operational 不足、调用次数已满或未披露费用 | 停止新增相应操作并显示明确 blocker；保留闭合预算与既有债权，不转扣 price/collateral |
| 最后一次闭合 | 先计入准确 terminal fee，再在同一候选释放未使用费用 hold；不先解锁预算，再另起事务试图收费 |

所有消费按提交前余额和指定 hold 校验；供方不能用本次将到账的 Data 为此次供电、费用或抵押融资。同一个 Agent 同时作为供方/费用 payer 时，合并电力交付与全部费用后的扣减校验，不能各项分别足额却合计超支。发出同内容新 request 也不能重计已成功单位或计量费。

闭合可用性是 activation 的强制条件，但合同不锁死未来世界执行版本。每个新履约/处置 intent 依[首次 canonical inclusion 所在当前 governing manifest](../module/module-lifecycle.md#sr2-activation-contract)选版；旧合同签名、旧 artifact 或原 snapshot 不提供旁路执行权。新合法 activation 必须显式支持原合同的有界 settlement/drain，保留可验证的旧条款/责任、合法 payer/source 和足额闭合上界，或者在切换前先依法结清/完成明确接受的迁移。支持旧合同的执行 artifact 可以更新，但其适配、费用与授权必须由当前 manifest 证明。

升级、停用、费率或 artifact owner 变化如果破坏上述条件，不能直接激活该破坏性变更。该约束不复活已撤销的普通新操作权限；当前合法 drain 只执行已承诺的狭义结清，不接新供能或新风险。没有真实 adapter、当前兼容执行和治理迁移能力时，不能启用服务后再依靠无限 blocked 保全资金。

<a id="service-checkpoint-settlement"></a>
### 6.4 单位交付与检查点提交

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

每个检查点的单位为 `[0,q_i)`。请求给出不重叠半开区间，实际数量 `n = Σ(end−start)`；本次 Electricity 为 `n`、Data 价款为 `n×p_i`、释放供方抵押为 `n×c_i`。乘加使用 checked 整数，金额不能越过原 ResourceAmount 的合法范围。区间是记账身份，不表示可以复制或分裂 Agent。

按同一 parent/root 顺序处理：验证原请求结果与单位状态 → 当前 owner/由当前 governing manifest 选定的执行绑定/服务窗口 → 原 signed 条款与狭义处置权 → 当前 canonical 共址和适用输入权限 → 未受保全的未交付区间 → 提交前供方可用 Electricity、各 hold、累计交易费差额/真实计量费及接收方/treasury 溢出 → 准备全部 delta。电力转移、Data 价款消费/支付、对应抵押释放、全部费用及 fee accumulator、履行量、原请求结果与 receipt 必须同根提交。

全部同版同范围的已提交重试只返回原件；请求区间部分重叠已完成、已处分或待裁单位时拒绝整个请求，不能默默缩小后执行。客户端查询原结果后才能显式提交准确剩余区间。单位账本优先于 transport request；不同 request/revision 不能再次消费同一单位。

一个检查点的保全不自动冻结其他检查点。未争议、处于原可履约窗口、权限有效且资源足额的单位按原条款继续；保全的直接影响须记录原因和范围，不能由其造成的新缺供反过来罚供方。窗口或输入条件不满足产生 blocker，`not_colocated`、普通 grant 撤回或权威缺失本身不是 default。

<a id="service-recovery-actions"></a>
### 6.5 修复、责任判定与终止

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

修复窗口、固定尾部时界和责任条件按[玩法恢复规则](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-remedies)消费。共同 `Q` 为已签正整数 canonical world ticks，随当前世界时基说明、原 `due_at_i` 和全部绝对截止一同进入完整正文；不知道时基、Q 不可读或任一 checked 推导溢出时拒绝绑定。所有截止只按唯一玩法公式从原检查点导出，开案、补证、换 ID 或继受不延长；测试用 Q 不成为生产时长。`checkpoint_due` 只读，不能产生新的债务或抵押扣减。

| 动作/事实 | 可执行结果 | 不能发生 |
| --- | --- | --- |
| `repair/reperform` | 在原修复期限内对尚未交付、未保全且具备条件的区间按原单价再履约 | 重做完成单位、补发奖励、静默扩大数量/价格/期限 |
| 完整责任证据 | 仅形成可供审查的事实/疑似缺供候选；正式 default/赔付只接受双方同版责任处置签署，或按原程序成为 final 的有效 panel 决定 | 把计数、时间、完整性标记或运行时自称判定升级为单方罚没权 |
| `terminate/settle` | 根据明确同意或有效政策/裁决，对准确剩余单位分配价款/抵押、保留既有债权，并产生终止/待付款结果 | 退回已付款、清除违约记录、取消其他检查点或释放仍负担有效赔付的抵押 |
| 固定尾部中性处置 | 只有完整 canonical 账本证明不存在有效责任结论的未交付单位，才按签署前披露规则解除对应占用 | 把 unknown 当无裁决；推翻有效初审/final 或待付款债权 |
| 续期 | 重新报价、接受和激活新合同，引用旧历史；新服务有独立数量与预算 | 复活旧单位、继承旧优先级、清零旧 case/失败或自动转移责任 |

责任输入为由 canonical 来源重建的单位账本、相关请求/交付结果、原条款、身份/位置/授权变化与适用服务可用性证据。`EvidenceCoverageV1` 绑定准确 tick 区间、起止 root、完整日志覆盖承诺、纳入的事件/receipt 和适用入口 availability 来源；host verifier 从已验证连续 canonical 日志重建相关对象及 gate 的变化，不能由调用者或 panel 填 `complete=true`。在激活起点固定初态，按当前有权的身份/位置/权限 producer 与服务 availability 事件归约；缺初态、窗口段、变化来源或最终性时就是 incomplete，不从当前快照反推整段历史。

完整性证明只说明证据范围，不授予裁决权。双方责任处置必须绑定确切未交付单位、已确认责任、各净金额、原保全/case 和同版正文，并重验两方当前签署来源；panel 必须按 §6.6 验权与原责任谓词审查相反证据。缺窗口覆盖、事件尾部或来源版本时，不能以未查询到记录证明供方不履行；原始证据不能获得则保留未判结论，按政策走固定尾部，不新增凭空赔偿。

<a id="service-dispute-authority"></a>
### 6.6 最小保全与有权裁决

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

执行 [panel 与程序政策](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-disputes)，不把现有组织救济者或社交事实 adjudicator 当成本服务的默认法院。每个 `(agreement_lineage, checkpoint_index)` 只有一个 case；请求重试、换 proposal/供方或补证只能关联该 case。保全记录准确未结算单位、所涉 price/collateral/必要不可逆效果、原因、证据、原窗口和 receipt；不冻结其他检查点、无关资产或正常查询。

`PanelSelectionV1` 固定本地 jurisdiction、治理授权和名册 root、排除后的控制主体、原 case commitment、允许的随机来源/轮次、证明与抽样排列。case/名册先固定，再接收该政策允许且事前不可预选的可验证随机轮次；不能以客户端 nonce、可挑选旧 block hash、本地 PRNG 或换 case ID 重新抽签。当前若无合格随机 authority，标记依赖缺失，不能假造抽样成功。替补与一次程序复核沿原排列和排除规则进行。

验证每名投票者的 jurisdiction/事项 scope、当前资格、控制主体与被审效应相关受益人回避、名册/轮次和签名。只有政策要求的不同合格主体对同一 typed disposition 达到门槛，才构成有效裁决。结论 payload 包含原 terms/case/单位范围、事实 cut、理由类别、责任归属、各 hold 可处分数量、支持的补救/终止，以及 expected record versions；数量和收款人必须被已签处分权覆盖。自由文本、财富、组织地位和多数无权签名不授予资产处置权。

初审投票只能在原 `evidence_cutoff` 后，消费统一冻结的全部已受理单位范围与证据 root。截止前各方合法具体争点并入原 case，不能用提前投票或局部结论耗尽唯一程序机会；冻结后普通补证或扩大 scope 拒绝。复核投票同样在原申诉窗口关闭、全部合格程序主张/证据 root 固定后进行，不提前终结另一方仍有权提交的机会。初审与复核的不同候选只能消费自己阶段/名册位置的有界槽位，重复投票或无状态变化不收费。第二方的合格程序主张并入原有一次复核，不重新选择成员、开启新一轮或续窗。

申请或申诉登记不撤销已有有效结论。政策规定的程序复核只能由有权主体确认可验证程序错误并留下关联决定，不能靠申请人自述使初审 invalid。固定复核期限结束后，依原规则将仍有效初审或有效替代复核确定为 final；事实 unknown 时不伪造 final。最终处分走同一 Kernel hold 消费和去重路径，裁决记录与付款记录分别可查。

固定尾部处置先核对完整 canonical case/初审/申诉/复核与资产账本，再判断对应单位是否确无有效责任决定。没有合格 panel 或未形成有效结论，不允许无限重抽/重开或无限延长本程序；仅符合政策中性条件的单位可以解除未判占用。有效 final、仍有效初审最终化后的赔付及已确认未付款责任不被尾部退款覆盖。缺席、下线、撤普通 grant 或拒补非必要材料不会阻止既有证据裁决，也不会自动获得中性退回。

<a id="service-succession"></a>
### 6.7 显式供方继受

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

继受只允许[玩法合同](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-succession)指定的双边交接：一次必须覆盖该合同全部仍可继续履约的未交付范围，不能保留原/新供方的并行新增供给入口。原供方、买方、新供方必须各自当前有效并接受同一完整交接条款；新供方使用不同自有 Agent。proposal/accept 本身无效果。提交时重验三方、全部剩余区间、旧 case/责任/费用、新资金与原继受 cap，在同一根事务中建立新抵押和费用占用、交接允许范围、释放可证明无旧负担的占用、消耗一个已签 provider 槽位、记录 lineage 与 receipt。部分接替、超 cap 或任一不满足时旧合同保持原状。

若仍有 stay/未决范围在解除后可能恢复交付，或者无法完整确定哪些单位属于仍可履约范围，则本 v1 保守拒绝整个交接，待原程序给出明确处置后再请求；不能把这些范围暂留旧方而在解冻后恢复双供方。已经合法终止且只有已判未付款责任的范围可留给旧方，其入口限于原资金/证据/救济/结清，不再产生新交付。普通解除保全不改变已提交的 provider 归属。

已完成单位、过去成本、原违约、待裁 scope、有效赔付和旧 drain 义务仍归原责任，不随 provider 字段变化消失。旧抵押只有被明确证明不再承担任何旧责任的部分才释放；新抵押不能抵销尚未确认的旧债务。一个单位不能在两个 provider 段同时可执行，业务键和各固定程序时界不变。

新未履行范围必须有属于新双方的实际 artifact-owner payer、由当前 governing manifest 允许的执行绑定和重新签署的 fee quote/预算；供电等各交易腿的合法 payer 另行分项验证。旧供方作为旧 artifact payer 时，不能继续向其旧预算扣新服务费，也不能直接改向新供方扣费而跳过 artifact authority。缺少可验证的分范围新执行绑定时返回 `successor_execution_unavailable`，不先改派或释放资金。旧义务保留当前 manifest 明确兼容的原有界 drain 条件；新旧执行者共享同一 canonical 单位/责任/fee accumulator，不建立第二个合同身份。

可验证的正向继受使用持续由买方拥有的 artifact：Kernel/module payer 不变，但旧结清与新剩余服务的预算明确分开并重新授权；新供方的 Electricity 交付交易费和抵押仍由其合法来源足额提供。此例不推导所有供方/payer 组合都能无条件交接。

### 6.8 命令与稳定结果

| 接口 | producer → consumer | 身份/顺序 | 成功与拒绝边界 |
| --- | --- | --- | --- |
| quote/propose/revise/accept | 当事 owner → 统一 admission/instance | 原 request、revision、terms/render digest、expected record version | 非绑定/零业务费用；修订清旧接受，不改原已绑定协议 |
| activate/deliver | 当事 owner → runtime | agreement、单位范围、当前 proof、expected versions；按 canonical 顺序 | active 或单次 delivery receipt；未支持/不足/漂移原子拒绝 |
| repair/terminate | 有权当事人 → 原业务路径 | 原未完成单位、固定窗口、原责任和处置依据 | 按原范围履行或终止；不创建新 scope/优先级 |
| open_case/evidence/decision/appeal | 有权申请人/合格 panel → registry/runtime | 唯一 case、原固定窗口、精确事实/签名/名册版本 | 合法登记/保全/裁决；未知无权结果不能处分 |
| replace_provider | 三方 → 同一根提交 | 原 lineage/剩余 scope、三份接受、新旧执行/资金绑定 | 原子交接或旧状态完整保留 |
| query_original/read | 受众允许者 → canonical read view | 原 request/业务键及所查 root；有界分页 | 原结果/receipt 或明确 unknown；不以空缓存推定未提交 |

拒绝至少区分 `unsupported_profile`、`terms_stale`、`request_conflict`、`record_conflict`、`authority_stale/denied`、`window_closed`、`not_colocated`、`data_grant_missing`、`unit_overlap`、`unit_preserved`、`hold_coverage_unknown`、`hold_scope_denied`、`fee_source_invalid`、`operational_budget_exhausted`、`procedure_slot_exhausted`、`insufficient_available`、`numeric_overflow`、`evidence_incomplete`、`panel_unavailable`、`decision_invalid`、`successor_scope_unresolved`、`successor_limit_exhausted`、`successor_execution_unavailable`、`profile_unavailable`。提交未知独立表示为 `outcome_unknown`，不能放进无效果拒绝枚举。

## 7. 状态、事务与持久化

<a id="service-persistence"></a>
### 7.1 独立维度与一个根提交

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

| 维度 | 语义 | 闭合判据 |
| --- | --- | --- |
| 形成 | draft/offered/accepted_pending_validation/active/closed_uncommitted | 只有 active receipt 建立占用与义务；未绑定关闭无罚款 |
| 单位履行 | 未交付/已交付/受保全/按明确处置终止 | 所有区间互斥，已完成不可复活；partial 是聚合读面 |
| 责任 | 未判/有效初审/程序复核/final/已执行处分 | 已判责任与是否付清独立；终止不消灭有效债权 |
| 资金 | 各 purpose 的 held/consumed/released 与 paid/return_pending | 累计量与原 hold 对账；零 remaining 不能靠删除记录伪造 |
| 请求/观察 | 未验证/原决定已提交/客户端 unknown/根提交 StatusUnknown | unknown 不允许消费者换业务身份猜测重做 |

所有业务状态、资源/hold/treasury delta、案件/授权、原 request 结果、单位赢家、事件序列与业务/execution receipt 进入同一 [ExecutionTransaction](../design.md#62-unified-executiontransaction-architecture-p0)。使用原 [authority commit seam](../design.md#runtime-authority-commit-recovery-target)和 [generation 持久化](../design.md#runtime-generation-persistence-design)，不独立写合同数据库后再补偿余额。

prepare 只改暂存对象；所有派生索引和 counter 亦不得提前发布。候选达到实际 authority profile 规定的持久决定和 proof 条件后才返回 committed。只有确定未提交时可以保存稳定 `rejected_no_effect`；该拒绝本身无法可靠提交则返回 unknown。提交后镜像/GC 失败不把成功改成退款或拒绝。

### 7.2 去重、崩溃与重放

传输键为 `(world/branch, instance, requester_owner_identity, request_id)`，payload digest 绑定完整命令、revision/CAS、scope 和 proof refs。相同键相同内容返回原决定，相同键不同内容拒绝；新 request 仍受单位与处置 effect key 约束。交付 key、抵押处分 key、费用消费 key 和继受 key 明确关联原 lineage；case/manifest/owner 变化不重置赢家。

恢复闭包包含原 terms/接受/处分权、各 provider 段、单位分区和累计量、所有 hold/索引/预算/程序分槽与交易费 accumulator、当前和必要历史政策/artifact/schedule、case/名册/随机证据/冻结证据 root/裁决、请求结果与 receipt。重放只归约当时已提交事实，不重跑 WASM/LLM、不重新抽 panel、不按当前费率/owner 结算，也不补扣离线时间；恢复后新 intent 仍按当前合法 governing manifest 选版。

客户端超时或落后读面先查原 request/业务键，显示所查 root；`not_found_at_root` 不能排除在途提交。根 generation 的 `StatusUnknown` 则按 [G1](runtime-storage-footprint-governance.design.md#storage-generation-g1-design) fence 整个旧 writer，停止后续 mutator/save/依赖派发，核对精确候选和持久决定、安装匹配内存后才恢复；不能只冻结此服务而继续写不确定 World。

正常恢复不得把“索引缺失/unknown/未知裁决版本”解释成无占用、无债权或中性退款。去重、零余额 hold、原裁决与责任历史按[存储治理](runtime-storage-footprint-governance.prd.md)保留；普通 TTL、模块禁用或客户端清历史不删除仍可合法重放的事实。

## 8. 部署、安全与运行约束

只能在满足合法 institution classification/activation、完整 instance 寻址、受保护 hold、fee/drain、当前服务闸门和可查询权威的 world 启用。本 profile 不创建后台无界收费循环或外部 effect；可由有权当事人/调度器提出到期处理候选，但任何处分仍须同一条件提交。

模块/客户端不能提供自证明余额、当前 owner、随机数、资格、完整性或裁决终局。所有 proof 绑定 world/branch、源版本、对象和精确 scope。拒绝跨 instance/跨世界重放、未知 schema/未声明效果、数值溢出和不完整资金来源；拒绝不产生收费或部分 state。私钥、私有 prompt 和他人无关证据不得进入日志或公开投影。

有界退出依赖权威可服务与真实持久提交。正式权威不可用时只返回原件/unknown/return_pending，不伪造退款；恢复后按原固定时界和已确认责任处理，不能借停机延长服务价格、重抽或删除资金占用。

## 9. 质量与容量

v1 固定 2 个个人主体、2 个当前 Agent、2–16 个检查点；继受命令暂时涉及 3 个签署主体。条款 canonical bytes ≤32 KiB，完整命令含 proof refs ≤64 KiB，每命令规范化单位区间 ≤32，每 agreement 的履行/处分区间分片总数 ≤256，分页 ≤50。证明通过有界内容引用读取，不能把无限原始证据嵌进命令。panel 名册、证据数量和程序次数沿用唯一玩法政策，不在本文增加值。

激活 manifest 必须提供正数有界的 proposal/active agreement 数量、每类已支持命令成功次数、proof 大小/解析预算和候选保持量；对应 fee 上界参与双方接受。继受成功次数另有已签有限 `successor_cap`，禁用为 0；provider 段最多 `1 + successor_cap`，对应 actor 槽总数在首次激活固定，不因换人、修订、重试或升级增加。新方只能占用原有空槽，超 cap 原子拒绝；新预算在原有槽位内验证，不重置原累计程序次数。

普通成功交付次数不得超过区间分片上界；程序分槽必须保留 §6.3.1 的每个合法最大机会，不能被交付或通用命令池消耗。剩余合法 scope 不能因主动把区间碎片耗尽而失去已预留的闭合路径：终态批次按 canonical 顺序消费既存完整未处分分片，每次至少使一个分片获得不可重复的终态，不凭调用者任意拆小；释放余款和 final receipt 的次数独立预算。最坏剩余分片、全部 fee/source/provider 槽、最终结论/返还的有界写集共同进入闭合上界，无法在命令字节和计量限制内分批完成即拒绝激活。容量耗尽拒绝新工作，不清理旧去重或有效债权。所有结构/数量上界先检查再分配或执行。

| 验证场景 | 输入/负载 | 判据 | 方法 |
| --- | --- | --- | --- |
| 部分交付 | 同一固定候选，2 个以上检查点、重叠/非重叠区间 | 单位一次；每资源 gross/hold/treasury 差额精确守恒 | PS02 |
| 占用与费用 | native/模块/费用/owner 操作竞争同余额，预算不足和 overflow | 未授权路径无法消费 hold；closure 预算仍完整 | PS03 |
| 争议/继受 | 一方用尽证据槽；截止前另一方合法争点；继受 cap 与 cap+1、部分接替 | 对方/投票/复核/closure 仍可用；统一冻结后投票；全剩余交接且不重置 case、时限或旧债权 | PS05/PS06 |
| 故障恢复 | prepare、持久决定、响应、snapshot/journal 边界 | 原效果/费用一次；未知 writer 不继续发布 | PS07 |
| 结构容量 | bytes/ranges/counters 的上限与上限+1，最大闭合写集 | 确定拒绝且零部分写入；仍能在预算内闭合 | PS01/PS03/PS08 |

本文不新增吞吐或延迟 SLO。访问应按 agreement/检查点/hold owner 的有界索引，不为一次请求扫描全世界合同；源 root 不完整时宁可报告 unavailable，不能返回默认空集合。实际硬件、并发与跨节点容量由实现候选测量，不由文档存在推出。

<a id="service-compatibility"></a>
## 10. 兼容、迁移与回滚

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

旧 native EconomicContract 和即时 `owner_atomic_resource_exchange_v1` 保持原义。旧 Open 的到期声誉、旧 Settle 单腿付款及历史 replay 不能转换为本服务的 offered、免费接受、价款占用或 partial 交付；旧 Service 类型存在不表示允许激活。新 profile 只能在独立版本的 module-owned 路径接入，不双写旧 native 合同。

启用前必须完整交付：同版形成与无业务费登记、Kernel 全路径 hold/狭义处分适配器、实际 metering/预算与 drain、精确 instance/codec/当前身份、单位与 case 账本、合格名册/随机/裁决 producer、同根事务/恢复、正式消费者及适用验证。缺任一前置返回 `profile_unavailable`，保留只读条款和旧原件；通用 handler、普通组织救济或旧 transfer 不能冒充缺失适配器。

升级必须保持 lineage、原单位/case/fee accumulator、狭义权利和恢复闭包，并在新合法 activation 中明确原合同 bounded settlement/drain 的兼容 binding；不凭旧合同签名继续执行旧 manifest。未提供兼容、合法结清或明确迁移的破坏性 activation 必须拒绝；未知版本不可写。禁用停止新增承诺，但保留已有义务的当前合法读取/结清和预算；不能通过停用没收抵押或切断争议出口。若回滚版本不能完整读取验证已写 generation/占用/责任，则保留数据进入验证只读/隔离，不能删字段、恢复旧 snapshot 后重做旧请求。历史赔付纠错只能通过有权、足额、关联原 receipt 的新义务，不能改写已交付电力或旧世界历史。

## 11. 验证设计与可追溯性

以下是目标验证计划。实际 source/build/artifact/manifest/authority、合法费用输入、运行结果与未覆盖范围归执行 PR；手册和旧回归定义不证明新服务已运行。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) | [形成](checkpoint-power-service-fulfillment.design.md#service-formation) | 接受零效果，当前同版双签与足额资金才一次 active | [PS01](../../testing/manual/checkpoint-power-service-acceptance.manual.md#ps01-formation)；固定已具备前置候选，核正文/签名/摘要、漂移和激活故障 | 原请求、hold/gross/费用差分、唯一 active receipt | 当前 API、消费者和 governance activation |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002) | [资源占用](checkpoint-power-service-fulfillment.design.md#service-resource-holds) | 全路径占用、狭义处分与控制权隔离；无第二份余额 | [PS03](../../testing/manual/checkpoint-power-service-acceptance.manual.md#ps03-holds-fees)；同 root 竞争扣减/转让/普通 grant 撤销、篡改 hold/index 与 scope | 逐账户守恒、原权利、拒绝和无效果差分 | 未适配 native/direct/trusted 入口不能由局部成功代签 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002) | [费用预算](checkpoint-power-service-fulfillment.design.md#service-fee-budgets) | 所有实际费用与合法 payer、程序 actor/阶段与闭合隔离、最大退出成本 | [PS03](../../testing/manual/checkpoint-power-service-acceptance.manual.md#ps03-holds-fees)；真实计量、累计交易费、个人份额耗尽/对方机会、升级停用和 terminal 最后一笔 | schedule/meter、各 purpose/程序槽、treasury 与关闭 receipt | 不证明生产费率合理或尚未激活 authority |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002) | [检查点结算](checkpoint-power-service-fulfillment.design.md#service-checkpoint-settlement) | 部分交付的电力/付款/释放/全部费用一次提交；其他检查点可继续 | [PS02](../../testing/manual/checkpoint-power-service-acceptance.manual.md#ps02-checkpoints)；非重叠/重叠区间、due、接收阻碍、并发和失败点 | 单位账、完整 receipt、聚合净额及当前条件 | 非共址输电、通用运输/服务 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002) | [恢复动作](checkpoint-power-service-fulfillment.design.md#service-recovery-actions) | 修复原剩余范围；完整归责、终止和中性条件不清有效责任 | [PS04](../../testing/manual/checkpoint-power-service-acceptance.manual.md#ps04-repair-termination)；原窗口边界、缺证/接收障碍、有效赔付未付和续期 | 原时界、coverage、责任/资金处置及新旧 lineage | 不推导所有持续服务的归责 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002) | [争议权威](checkpoint-power-service-fulfillment.design.md#service-dispute-authority) | 有界抽样/回避/统一证据冻结/裁决；申请不洗责任、unknown 不退款 | [PS05](../../testing/manual/checkpoint-power-service-acceptance.manual.md#ps05-disputes)；固定合法名册/随机 proof、冻结前后/回避/迟案/缺席、初审/申诉/final/unknown 组合 | 选择证明、typed disposition、保全范围、原时界和付款结果 | 未实现随机/名册 authority、一般法院 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[REQ-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002) | [显式继受](checkpoint-power-service-fulfillment.design.md#service-succession) | 三方同版、新资金/执行绑定、旧责任保留和一次原子交接 | [PS06](../../testing/manual/checkpoint-power-service-acceptance.manual.md#ps06-succession)；旧供方仍有责任、未签/授权漂移、payer 不合、交接/履约竞态 | 三方原件、新旧 holds/provider 段、旧 case 与唯一 receipt | 任意主体/组织继受、个人 credential 转让 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002) | [持久化与恢复](checkpoint-power-service-fulfillment.design.md#service-persistence) | 原请求/单位/费用去重，完整闭包，根未知 fence 与历史重放 | [PS07](../../testing/manual/checkpoint-power-service-acceptance.manual.md#ps07-recovery)；固定候选注入 prepare/commit/回复/snapshot 故障，恢复准确原键 | durable 决定、root、writer fence、holds/责任/receipt 一致性 | fixture 不代签未执行故障域/节点 |
| Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；[AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002) | [可读结果](checkpoint-power-service-fulfillment.design.md#service-read) | 各 surface 区分交付/责任/资金/unknown，受限读不泄露 | [PS08](../../testing/manual/checkpoint-power-service-acceptance.manual.md#ps08-read-compatibility)；同 root 双方/旧方/旁观者和 stale/reconnect | 原件、各受众投影与可执行下一步 | 未接入 UI、玩家理解度 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) | [兼容与启用](checkpoint-power-service-fulfillment.design.md#service-compatibility) | 旧 Service/即时合同不改义；缺依赖不激活，升级回滚保留退出与旧原件 | [PS08](../../testing/manual/checkpoint-power-service-acceptance.manual.md#ps08-read-compatibility)；旧数据、缺 adapter/随机 authority、破坏性升级及不兼容回滚反例 | legacy 差异、拒绝、原件/占用与 drain 可用性 | 不证明 SC-32 pilot 或整体迁移完成 |

<a id="service-read"></a>
### 11.2 可读结果的验收对象

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。

同一 canonical view 至少返回条款/政策与执行绑定、双方/各 provider 段、原检查点时界、每单位已交付/未交付/受保全/已处分范围、各 hold 的原量/剩余/消费/释放、实际费用与剩余闭合预算、有效责任及付款状态、case/初审/申诉/final 的来源、原 request 查询键、当前 root/新鲜度、blocker、合法 next_action 和 next_recheck。按[玩法预览](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-preview)呈现机会成本和安全替代，不能用一句“合同已结束”掩盖有效未付款责任。

只读观察者得到受限摘要；当事方可核自己签署和净结果所需的完整事实。旧供方可以读取与其原资金/责任有关的原件，不获得新供方私有余额或新写权。`request_received`、`active`、`partial`、`final_decision`、`paid`、`return_pending` 与 `outcome_unknown` 明确区分；真实提交前不能用 eligibility、排队或客户端渲染表示已返还资金。B24 的可写状态须由真实 consumer 与验收另行更新。

## 12. 决策、长期风险与未决问题

采用“有限检查点、单位原子结算、原余额受保护占用”的方案，可复用现有资源与统一事务，明确已完成与未履行责任；代价是必须先交付全路径 Kernel hold 和可保持的合法 drain，不能只实现一份服务 module。两种费用预算使正常操作有界，也让合同保留明确退出成本；它们不免除 mandatory metering 或保证基础设施始终可用。

| 风险或剩余实施依赖 | owner 与处置 | 解除/复核条件 |
| --- | --- | --- |
| 某个扣减/owner/upgrade 通路绕过 hold，或把 module 数字当资产 | runtime/Kernel；本 profile 不激活 | 覆盖全部相关 publisher 的侵占、混代、转让、恢复反例与资源对账 |
| fee source、合法 schedule 或 drain 被升级破坏 | gameplay/governance/runtime；拒绝破坏性变更，保留当前原件与权利 | 有足额可执行的旧 drain 或明确同意的新绑定；最大闭合写集计量通过 |
| 本地名册、可信随机或案件事实不完整 | governance/P2P/runtime；不伪造 panel；目标写入口依实际前置禁用 | 原政策/证明与有限程序的真实实现；固定尾部不能清掉有效债权 |
| 服务可用性与责任证据不能完整重建 | runtime/QA；不从缺失推 default | 原世界日志/coverage producer 和边界验证齐备；记录实际证明范围 |
| 正式 Consumer、故障域和玩家验收未交付 | Service/Viewer/testing | 同一真实候选按 PS01–PS08 验证；文档与静态源码不代签 |
| 其他维护、运输、组织与多方服务仍需设计 | 产品及对应专业 owner | 另按已采纳义务补完整 profile；CR001/CR002 保持部分覆盖 |

本设计的具体技术身份、索引、上界和事务算法属于专业实施选择。正式 institution 分类/激活、生产费用适用权和实际可信随机来源仍须原有 authority 具备；本文既不代签这些运行前置，也不把未知接口当成已经可用。
