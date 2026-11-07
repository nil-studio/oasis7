# 预工作 R&D charter、贡献 provenance 与份额转让执行设计

- 状态：`active`；本文件是目标执行合同，不表示能力已实现、治理已授权或实际验证已通过。
- Owner role：`runtime_engineer`；`gameplay_designer` 拥有本 profile 政策，Agent owner 负责行为/训练来源，治理/P2P owner 负责合法身份、权限、随机与提交证明，`qa_engineer` 负责独立验收。
- 固定审读输入：`ebd9b8431f2dc084c4ef6be3bcea9ca054ab2a81`，2026-11-05；原个人首版审读输入 `c3862d479ba835bddde3539e8e8e97c5cb563166`、2026-11-04 保留为历史。政策为 [personal-rd-charter-v1](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#rd-charter-policy-v1)；方法为 [R&D 验收手册](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md)。

## 1. 问题、目标与非目标

现有产品要求预工作 charter、永久 Agent provenance 和显式份额转让；[工业许可 authority](industrial-license-and-royalty-settlement.design.md#license-authority)已要求消费可验证的同版分配来源，却尚无完整的来源生产合同。目标是让已采纳的个人、组织、多方合作及有限开放加入产生可核验原件，并使后续许可、付款和转让使用同一份原分配与当前权利事实。

原 personal-rd-charter-v1 的同一 world/branch、自然主体和单epoch解释保留。后继按[原专业完整政策](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#rd-charter-general-parties)接收组织代表及多方、顺序加入/退出/变更；不是另造小profile代签全卡。代表权限来自原普通组织/通用形成authority，本设计只验证，不自行创建。两版均不建立通用知识产权法院、科研价值评分或全局信誉；不重写工业四阶段、Agent经济转让、外部法律权利、原收入累计算法和许可争议程序。代码、正式入口、持久恢复、费用和玩家理解须由实际候选另证。

## 2. 上游约束与相关角色

R&D 必须服从原 Agent/owner 当前授权、来源使用许可和工作消费者的资源/退出规则。charter 只赋予其声明的合作权利，不自动提供工业准入、设施权限、模型预算或经济资源。冻结分配与来源消费 [GAME-018](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#industrial-license-policy-earned) 的稳定 slot/bps/累计 `E`；不能通过新 revision 重开池。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 全体必需方同版接受先于实质工作，未知/倒签/越界不激活 | [激活与工作时序](#rd-charter-activation) | 个人身份、Agent 当前授权、原工作 adapter | 不把 charter 当组织代表或工业准入 |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 贡献事实、来源使用权与经济分配分开，先存输入不倒签 | [贡献来源](#rd-contribution-source) | 来源 grant、真实工作 producer、gameplay | 不证明科研正确性或现实知识产权 |
| [AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | Agent 控制变化后仍保留身份、授权、训练/模块来源和关键 receipt | [Agent provenance](#rd-agent-provenance) | Agent 身份/控制历史、provider/module 来源、受众权限 | 不公开所有私有原件，不声称未知训练来源已验证 |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 工作关闭后按真实来源交接不可变原分配，缺必要权利不可付费发行 | [冻结交接](#rd-distribution-handoff) | 工业 lineage/admission、许可 reader | 不扩大许可硬开放或适用收费单位 |
| [AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 整槽转让保留旧账、固定比例和个人 credential 隔离 | [显式转让](#rd-share-transfer) | 当前 holder、受让人、许可池及原程序 | 不支持拆槽、合槽、追扣已付或自动继受服务责任 |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | receipt-first、本地随机无冲突 panel、有限程序申诉、最小保全和硬尾部 | [归因复核](#rd-review) | 原世界授予的 RD 管辖、名册、随机和完整结果 | 不借用许可/服务管辖，不增一般法院 |
| [AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 原提交未知、旧数据、转让与生命周期恢复不重放、不清历史 | [原请求恢复](#rd-recovery) | 原事务/请求日志、checkpoint 与受信 reader | 不用本地 marker 代替世界提交证明 |
| [REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | proposal 消费当时证据，付费许可/建池/付款消费完整原来源及当前 holder | [直接消费者](#rd-consumers) | creation、许可/收益池、PPE、正式 API/Agent/Viewer | 不给旧 receipt 追溯收费或改累计 E |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 接受前保护真实 payer 的完整程序和闭合成本，不挪贡献款或单方耗尽他人机会 | [资源与预算](#rd-resources) | 当前 metering、狭义资源 hold、源工作退出适配器 | 参数复算不证明吞吐、价格平衡或实际可用 |
| [AC-RD-08](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-08) | 组织/自然多方真实当前mandate、source owner与同版接受 | [rd-charter-activation](#rd-charter-activation) | CR001普通组织/共同形成、原身份与资金authority | 不从OC grant/名字自动创造代表或共有资金 |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 原采纳的开放加入/退出和未来条款变化有正向执行 | [rd-charter-membership-epochs](#rd-charter-membership-epochs) | 当前holder/真实工作来源、common形成与原RD程序 | 原v1不追溯升级；不改同lineage经济向量 |
| [AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 新epoch及组织变化后完整原件/永久provenance与旧权利不丢 | [rd-charter-membership-epochs](#rd-charter-membership-epochs) | 受权历史归档、source/rights/许可消费者 | 不以摘要或现代表代签历史 |
| [AC-RD-09](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-09) | 4epoch变更边界和独立新成果不洗旧权益 | [rd-charter-membership-epochs](#rd-charter-membership-epochs) | 原工业lineage与来源authority | 不改名绕过容量或另开旧池 |
| [AC-RD-10](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-10) | 完整canonical wire及每phase全程序/关闭成本可行 | [rd-capacity](#rd-capacity) | 原Module ABI/codec、metering及source proof、实际处理能力 | 不假定raw128KiB能直接装256KiB封套 |

## 3. 当前状态、目标状态与差距

| 对象/能力 | 当前状态（固定基线） | 目标状态 | 差距/假设 | 证据或 owner |
| --- | --- | --- | --- | --- |
| 产品/许可依赖 | CR-003 与 GAME-018 已要求预工作同意和 provenance，许可已有稳定份额累计设计 | 可核验 charter/贡献/权利 head 与直接消费 | 此来源生产与转让组合仍为目标 | [原产品](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)、[许可 authority](industrial-license-and-royalty-settlement.design.md#license-authority) |
| 组织 charter | `OrganizationContinuityCharterV1` 有继受名册及 residual_share_bps | 不混同个人 R&D 协议 | 组织剩余资产分配不证明研究贡献 | [现类型](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_types.rs) |
| 模块 provenance | host 注入 caller/origin；发布 artifact 保存 publisher 与 hash | 作为来源链的局部输入，补完整贡献和权利证明 | 既有字段不能独立证明训练来源或收益权 | [ABI](../../../crates/oasis7_wasm_abi/src/lib.rs)、[artifact 状态](../../../crates/oasis7_simulator/src/world_model/module_market.rs) |
| Agent 授权 | canonical owner/身份 generation、intent 和结果已有更窄合同 | 贡献引用当时授权，不追绑 current owner | 正式 R&D source adapter 与持久 provenance 尚需交付 | [Agent 认知生命周期](agent-cognition-lifecycle.design.md)、[间接控制执行](indirect-control-agency-execution-and-continuation.design.md) |
| 费用 | 当前完整 module invoke 按 artifact owner 解析 Data/Electricity payer | RD/许可两 instance 内的完整费用保护及同根迁移 | 新 hold、typed reducer 和全部扣费入口尚需实现/验证 | [metering](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)、[费用公式](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs) |

以上为静态源码/设计审读。检索目标 R&D/royalty/benefit-slot 类型未发现对应实现，不以检索结果证明全仓不存在任何相关能力；未运行测试、游戏、模型、浏览器或真实治理流程。

## 4. 边界与结构

charter、贡献/分配、权利 head 和归因案属于 L2 institution，遵守 [Kernel/Institution 与 native 分类冻结](../prd.md#native-surface-分类与冻结规则)。目标记录为带完整 world/instance/artifact/schema/activation 的 module-owned 原件和受保护存储；不为 RD 新增核心 Action、`WorldRuleCommandV1` 分支、WorldState 顶层字段或任意 `apply_rd` 旁路，不选择制度迁移试点。制度 reducer 只能返回自身合法 new_state、有界狭义资源候选和必要 receipt/notice emit；Kernel 按当前 `cap_ref/cap_slot`、允许操作、原 hold/受益范围、预算和守恒校验。候选 new_state、emit 或外部 ACK 不证明世界结果已生效，只有同一合法世界事务发布才成立。

一份稳定 charter lineage、其当前epoch工作/贡献目录、原分配、权利head及该epoch唯一归因案归一个明确的受权 RD instance；后继最多4epoch顺序执行，旧epoch保存完整可检索原件。不同 charter 用不同 instance，不把四份 charter 或所有历史内嵌进一份 reducer 输入。每份原存储硬上限保留128 KiB；本轮完整封套可执行形状另收紧current body至96 KiB（§9），完整条款/接受、说明正文和追加历史保留在原 journal/可审计内容原件，当前 state 保存已校验的必要字段、紧凑引用及完整目录/历史根；精确容量见 §9。身份、来源 grant、工业工作、模型调用、治理和经济账本仍由原 producer 负责，不新建通用工作流或任意裁决平台。

新增事实必须有真实 producer、受众、精确版本及可验证原件。调用者提供的是 intent/proof reference；Runtime 根据可信世界来源决定接受或拒绝。hash 绑定内容，不证明作者、权限、先后或合法使用。LLM、Viewer、索引 cache 和普通治理 vote 都不成为归因 authority。

许可 instance 独立拥有原 fee unit/notice、累计分配/支付及许可案件。一个 RD 原分配只接一个经原工业 authority 确认的 lineage/原池键；该池的所有许可和用途根共享原冻结分配。跨 instance 的权利修改只支持 **RD instance + 这一许可 instance** 的同根事务；来源读取用当前受信的有界投影/证明，不暗加第三次 module invoke。任一必需状态由第三个可变 instance 拥有、闭包超出预算或无法同根提交时，当前 profile 拒绝该新操作，不能先转一半再异步补另一半。

## 5. 关键运行流程

<a id="rd-charter-activation"></a>
### 5.1 同版激活与首次实质工作

`QuoteCharter` 只读返回完整条款、当前身份/授权、引用来源、作用域、政策、各 payer/cap、原治理管辖/随机来源、`Q` 及接受有效边界。`draft/offered/accepted_pending_validation` 是未生效表达，不预留合作 slot、资源或资格。签署 bundle 绑定同一 canonical terms digest、world/branch、每方原身份/控制任期、一次性接受 nonce、当前授权及 `[Q,Q+60)`；缺任何必需方不激活。个人别名及代理沿真实身份去重，不能只比较 Agent ID。

`ActivateCharter` 在提交点重新验证：完整本epoch自然/组织必需主体集合及零收益同意；原固定工作 scope/角色/单元与来源规则；正整数槽向量；每项必要新许可或既有 grant；相同原 scope 的首次工作索引；管辖/名册/随机资格；真实费用/关闭预算及容量。全体通过才同根消费原接受 nonce，登记不可变条款/`T`、工作索引和费用 holds，形成一份激活 receipt。失败全部无效果；授权漂移/过期须重新报价接受，不能自动续旧签。

scope 固定自合法非绑定提案或有权登记的具体工作对象，不能由客户端随意把旧 root 改名。登记不创造成果、独占想法或抢先权。受支持 producer 在首次生成本 scope 的新分析/实验/代码/训练结果之前，必须消费 `BeginRDWork` 的 exact charter、work unit、当时权限及时间 fence；工业/模型/模块自身的预算和退出条件同时成立才可开始。只读查询、非绑定商谈和工业提案登记不算新贡献；生成新研究内容则不能用“草稿”标签绕过 gate。

工作完成和激活在同 tick 时也比较 canonical 顺序。无法证明开始/完成时序的手工上传、旧结果或外部资料只能作为明确先存输入，不认定为新合作贡献。没有合法来源许可则不允许适用的使用。`T+600` 不开新单元；每项开始须能按原 source adapter 的有限正常/停止/未知结果合同在该窗口内形成合法处置。不能给没有 stop/late-result fence 或关闭预算的工作开工，再用 RD 到期假装结果消失。

每个epoch开工后不改该原名册、scope、角色分工或原向量。后继下一epoch的正向加入/未来工作变更仅按 §5.1.1 完成；撤回某人的后续授权只停止其新工作；已提交结果、已接受持续服务及责任仍沿原合同。新的独立 charter 不迁走旧工作，工业 authority 认定为同 lineage 时仍消费原经济来源。


**后继组织/多方的共同形成。** Quote/Activate 消费 [FormationTermsV2](agent-mediated-contract-formation.design.md#contract-formation-common-terms) 与 [current principals/mandates](agent-mediated-contract-formation.design.md#contract-formation-principals-mandates)。先对完整family terms body计算原RD摘要，排除自身派生ID、common摘要及接受/activation receipt；再把完整body和该摘要纳入common terms/render，最后才签接受及形成request。receipt关联两摘要，不能让charter terms反向包含其未来receipt而成hash环。原v1已接受字节按原codec解释，不重签或倒填v2。

必需principal并集按本epoch原政策计算，≤16；common的较大上限不放宽RD。自然人与组织、贡献者与真实source owner、holder与当前payer分别列明；一人多身份/多Agent去重只合并同一principal，不能删一个有独立权利的组织或个人。组织需完整原登记、ordinary signing clause、批准门槛、当前mandate与任期；其多个批准人的可验证签署可以聚合，但不以单个代表签名/治理passed/OC grant替代原门槛。退出成员、旧代表和新owner的authority须按当前原件重验，不从现名单倒补历史。R&D receipt另保存实际工作actor/controller/role-term和当时责任principal，不能把组织收益主体作为唯一作者。

全部未来执行/程序/闭合费用依 [common cumulative exposure](agent-mediated-contract-formation.design.md#contract-formation-cumulative-exposure)与原IA/各domain obligation逐真实source核算。实际已花D不因退款清零，尚存业务/风险O与未来完整费用C分开；closure为不可挪用子purpose，引用原账不建第二账。组织有cap并不创造资金；使用个人owner Agent或物品/资源时该人按真实source单独接受，超16方在新承诺前拒绝。

common验证/prepare是原RD family executor这一次invoke的部分；原common接受/授权只读证明及既有Kernel授权nonce/费用应用仍在原同根事务内，没有第二个可变formation instance或隐藏调用。组织采用真实Kernel可用source/受权证明的正向路径必须可执行；若某特定资金/授权要求额外mutable module invocation而不能在原两instance闭包完成，只拒绝该特定新来源组合并给准确缺口，不能据此把组织全类型设为不适用。原license存在时仍最多RD+license两次完整invoke。

<a id="rd-charter-membership-epochs"></a>
### 5.1.1 原charter上的有限开放参与与历史连续性

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。采用 [开放参与](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#rd-charter-open-participation)与[变更边界](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#rd-charter-change-boundary)。后继charter的第一次接受就绑定最多4epoch、同一原分配/lineage和不能重写旧工作；原v1单阶段没有自动升级路径。

| 操作/状态 | 必需完整输入与当前检查 | 原子结果或拒绝 |
| --- | --- | --- |
| 自己停止未来工作 | 本epoch原principal/当前授权、原work集合与source的安全结束边界 | 标记该主体未来Begin禁用；已有原结果/服务义务/收益槽不变。一次主体停止占其原16个停止槽之一，本epoch内不可撤销停止标记；重新工作只能下一epoch重新明确接受，换代表/nonce不清标记或重复耗他人份额 |
| 下一epoch quote/协商 | 已完成前epoch的原件目录、work/程序/费用明确终态、当前rights/来源和新必需方/未来scope | 只读完整差分及新报价；未激活无新hold/工作slot，不回写旧签署 |
| 加入/未来角色或scope变更 | 新全体≤16principal同版接受，包括全部当前holder、新贡献/资产source owner/payer；新角色可明确0收益 | 只能进入尚未激活epoch；旧名册、已发生贡献及前程序不变。缺人、旧签名、超人数/epoch、未知前终态拒绝 |
| 下一epoch Activate | expected stable charter/current epoch/history head/rights与原未来工作first-Begin索引；新current mandate/来源/程序/费用/容量全部成立 | 同一RD事务推进epoch、保留已验证前archive、消耗本次接受并建立新有限预算/新T；receipt同时绑定前head与新terms。不更改原经济向量/许可池E |
| 整槽转让后加入 | 转让先按§5.5合法同根完成，当前holder/rights/pool结果确定；随后新epoch采用真实当前holder | 转让不代替新epoch接受，不继承旧作者/责任；若两请求顺序变化，后者CAS重验，不消费旧holder签署 |
| epoch4之后 | 当前稳定lineage计数及完整历史可读 | 只拒绝第5个新激活，保留原结果/付款/更正/安全退出；换ID/实例/组织代表不重置。独立新lineage只能按原source/工业authority证据另案 |

前epoch关闭的充分条件是：每项Begin已有确定结果/终止或原合法晚到fence；完整贡献和必要来源目录已提交；原归因案/通知按原窗口和完整输入有最终/中性闭合处置；实际费用和closure已结清，原请求结果可恢复。不能将已过tick、无在线成员、ACK、隐藏未决案或旧source读取失败当作闭合。先前合法许可的后续付款/收益案可以继续，只在它实际影响本次current rights/使用许可时阻断依赖它的新激活；不强制把整个工业收入流结清才能新研究。

归档不是丢弃状态。原closing迁移保存完整闭合state/条款/接受/角色/来源/work/案/费用/rights/receipt原件及其可验证目录；这些原件留在原已计费的canonical journal/内容保留合同中。current body保留最多4个epoch的固定目录条目（各≤512bytes）和已验证prefix root/count、当前head，不将四份完整case内嵌进下一输入。每条可解析到完整受权原件，reader按原audience+当前读者授权取得；不可用显示精确缺源，不能拿hash冒充完整历史。上一closed原件尚未可靠保存/验证时不覆写current state。

旧自然人、退出成员和前代表的历史范围分别保留；当前组织代表只读其组织获准范围，新Agent owner不取得前任私人资料。当前角色变更不把旧贡献改名，原旧case仍可查及执行其原有权更正，不能被新case重审。原scope/lineage/slot/bps/累积E和旧已付不因epoch推进或归档变化。新工作未再经原工业admission，不能因旧epoch曾有handoff_ready便收费；它只能补充同一原来源的真实新增贡献/使用权证明，不另开pool或重新起算开放时间。

全部epoch使用同一稳定业务键前缀 (world, charter_lineage, epoch_index)。新Activate为该index唯一，work/result/case/notice键再含原epoch与原unit；换revision/request不增加epoch或重复work。每个新epoch当前费用、完整程序与capacity重新有权接受和保护，不借前阶段退款/未来版税。窗口按新真实T/N开始，但仅属于新工作；旧窗口保持原T/N/C。两个并发新epoch候选只能一个成功；未知结果先查原请求，不另激活下一序号。


<a id="rd-contribution-source"></a>
### 5.2 贡献记录、来源及客观更正

`RecordRDResult` 只接原已授权工作 producer 的完整 receipt：原开始/完成或安全终止、工作单元、当时主体/Agent authority、内容承诺、用到的 source/grant、实际结果、世界顺序和关闭义务。完成结果失败或无新发现也如实登记，不占用第二个工作单元重试同一次结果。每个工作单元只有一个原结果；合法改正追加 source revision/理由，保留此前原件及替代关系。

下列来源分别验证，不能把任意存在的 event ID 当作贡献：当前受支持的 native 工业工作结果；同一确定性模拟/试点产生者的合法结果；受原执行与认知合同约束的模块或 Agent 研究结果。每个 adapter 必须证明真实开工/输出、当前授权、范围和退出，不以 provider 输出字符串证明权利。新增 adapter 先定义 exact source/schema、proof 和失败/恢复，不自动解释历史 receipt。

贡献 fact、成果使用 grant、原收益 slot 和个人 credential 分开存储、读取和更正。按事前角色/结果判据记录事实，不估计“思想价值”、用模型分或按劳动量调整 bps。未完成角色可显示实际结果，但没收份额、补偿或责任必须有原合同依据；本首版不自动增加这些经济效果。

先存来源保留原创作/许可时间、权利人、版本、用途/地域/期限和派生条件；原 grant 已足够的事项不制造一份新的第三方签名。需要本次额外同意者计入初始必需方，超限在开工前拒绝。非参与第三方的原权利、撤销和争端由原 grant authority 处理；有权证明改变当前使用条件时重新校验相关消费，不能让未核实的陌生投诉建立无限 RD hold。

客观更正只依据完整来源纠正字段错配、重复、遗漏或失效投影；它不改写发生过的工作，不新增原接受、不重开 `N/C`、不裁定新经济责任。后续新证据也须保留旧记录。闭合后必要的更正是另一次有界、明确付费来源的原子操作；受信源已纠正而本地投影尚未更新时显示陈旧/来源变化，不能因没有索引费用继续声称旧投影有效。只改历史事实的更正不动 rights；触及可收款主体/其授权映射的有权更正必须进入 §5.5 同一 rights revision、当前转让链、付款 fence 与池同步路径，不能先改 RD 后等许可自行刷新。

<a id="rd-agent-provenance"></a>
### 5.3 Agent provenance 与受众

每项贡献绑定稳定 Agent ID、原 identity generation/控制主体、授权范围/有效顺序、原 intent 或工作请求、所用 module artifact/hash/版本和 source/grant。训练/微调实际发生时还要有训练输入/配方/模块来源及其许可范围；仅调用 provider 时记录可验证的 provider/model 版本、调用来源及其授权，不编造其未披露的训练材料。`verified`、`declared`、`not_disclosed`、`unavailable` 分字段表达，不用一个整体绿标掩盖缺项。

源权限要求哪个证明，就必须取得哪个证明；模型供应方的合法调用授权不自动代表全部输出许可或完整训练来源。unknown 不是自动全局失信，也不是使用权通过。旧 provider 别名、module ID 或当前 owner 不补成当时精确来源。身份轮换、Agent 转让和退休只追加历史，不重绑旧贡献到新 controller。

小型 canonical 贡献目录、接受/结果/更正/转让 receipt 及内容承诺永久保留在可验证世界因果链；必要原件按原权限和可审计存储合同引用。只给公开受众必要的范围、状态和归因摘要；原当事人、合法来源权利人和当期有权 panel 才能读取其授权证据。新 Agent owner 不继承前任私有 prompt、训练原件或签署能力。日志/公开截图不泄露签名密钥、原私有 goal 或不必要个人材料。

permanent 不承诺所有节点无限在线，也不能被“已归档”解释成可以删除证明。原件丢失或源不可用时保留原承诺与明确不可核验状态，禁止新付费交接/转让对必要缺项降级放行；恢复沿原可信来源，不从模型或缓存猜回内容。

<a id="rd-distribution-handoff"></a>
### 5.4 工作关闭、通知与冻结交接

`CloseRDWork` 仅在所有已开始单元已有权威完成/终止或其合法晚到 fence 已证明生效时关闭。未开始单元标为未执行，未知单元查询原请求；不能把超时当无效果。窗口结束按原条款自动停止新开工，实际闭合不依赖全部人重新在线签字；提前关闭须全体必需方同意。工作、失败/损失和来源的实际费用在各自原账上，不凭关闭自动退款。

关闭同根保存完整结果/来源目录、原向量、当前不完整项及按原权限可回读的 notice，首次提交 tick 为 `N`。这份 notice 的窗口不因读取/更正/转让重置。先收集完整已登记索引，再形成确定 root；不能以分页中第一批替代全部贡献或某个后到当事人的证据。

原 `RdAllocationSourceV1` 绑定 `(world, branch, exact charter/revision/digest, original distribution source, validated capability lineage)`，固定原稳定 slot、初始合法主体与 bps、接受/激活、贡献/使用权闭包、`N`、原案/更正及交接时的 rights head。原来源只对应一个合法 lineage；实际派生关系由工业 authority 决定，RD 注册不能创造新的收费期。其原分配由第一次激活固定，后续只追加证明/转让，不签新权重 revision。

后继来源保持同一原distribution_key/第一次激活terms digest、slot/bps和pool key，额外引用精确epoch及其完整贡献/使用权/程序闭合目录；current charter terms revision不是新的distribution revision。consumer须验证该有界后继schema及完整历史prefix；不能用新epoch摘要把许可初始化为另一池，也不能把旧epoch的handoff_ready当后来新工作成果的许可。原工业lineage及本次成果准入/current source仍独立验证；前epoch已验证不等于未来材料/训练/代码自动获权。原v1 reader不懂后继字段时拒绝依赖它的新付费路径并保留旧合法收费/读取，不按缺省吞字段。

在通知窗口结束且该次交接所必需的来源、原分配、程序状态均可证明合法时，才发布针对精确成果范围的 `handoff_ready`。只争历史署名、不影响该成果使用权/分配的事项不额外阻断；涉及必要原身份/授权或来源的待决范围不放行。一个首次付费向量需要其全部必要槽身份成立，不能删除争议槽后重分其 bps。已存在的其他合法许可/工业工作不因新 charter 的争议停用。

没有收益、未准入、明确零费、使用权缺失、成果失败和程序结果未知分别表达，不都叫 ready。与已有 lineage 原分配冲突时，新贡献历史仍可保留，但本版不得另发新经济来源。旧份额变更须另版治理政策及完整迁移；本设计不给该迁移预留任意绕过位。

已知来源/lineage 或其他交接前置不足，可以记录 `handoff_blocked` 的本次处置终态；工作、原案和费用均已明确结清后，允许退还未用预算并释放工作容量，不因还没有可收费成果永久占用四份 charter 名额。原提交、工作结果或资金决定 unknown 不属于这种关闭。后来补齐同一成果的合法前置时，首次 `handoff_ready` 是 §6.2 的一次新有界原子迁移，先取得当前真实 payer 的明确同意和完整费用 cap；不能复活已返预算，不能重开工作、N/C 或原案，不变原 scope/名册/向量/E。仍须核原业务键，已发布或未知的首次交接只恢复原请求，不再新发一份。

<a id="rd-share-transfer"></a>
### 5.5 整槽转让、建池与付款的同根边界

转让 bundle 精确绑定原分配/slot、当前 rights revision/holder、受让自然主体或后继版本合法组织principal、未付与已付账承诺、适用 lineage/池、原程序/风险、当前费用与双方同版接受；报价有效期沿 RD 政策。只支持一个原槽的全部未付及后续权益，不迁移 bps，不附加售价。新 holder 不进入历史工作名册；同分配中已持另一槽、缺来源/身份或只拥有 Agent 的受让请求拒绝。

`CommitRDShareTransfer` 按以下完整顺序准备并一次提交：

1. 先解析原请求已知结果；再核双方当前授权、原 holder/slot、完整转让链、必要来源及原 quote/rights revision。
2. 从可信池注册关系读该原分配的唯一池或可证明尚无池。枚举受影响的付款 writer/dispatch、程序 notice/case 和 current rights 索引；索引必须完整且与当前世界根一致。原付款结果未知、该槽归属正受合法保全或无法查全时，只恢复原操作，不新签转让。
3. 逐仍有效程序计算完整主体并集：原仍有权当事方、历次 holder 中仍有对应范围权利者及新 holder。原 R&D 工作案保留冻结名册，转让不增加历史贡献者或重开窗口；每个许可案仍受其 `m≤10` 上限。不能仅用 current holder 去重来删除前人机会。必要新 actor/phase 槽及其实际费用增量由真实 payer 在本次提交前明确接受、足额保护；不挪原方额度。无法满足上限、原期限或资金/写集时拒绝转让。
4. 新 holder 或其控制关系若使受影响原案的已选 panel/独立性失效，且不能按原固定程序保持完整机会，则拒绝该新转让；不借交易重新抽样、改管辖或消除原投票。未受影响的案件不被扩大冻结。
5. RD instance 写新 rights revision/current holder/转让 receipt；已存在许可 instance 在同一世界事务写同槽 holder/rights revision、相关程序增量和账本绑定。两边同时成功或同时无效果。所有已计费批次、`E`、原 slot/bps、尾差和旧已支付 recipient 行保持不变；应付以稳定 slot + 当前权利版本解析，不删除旧行重建账。

池不存在时第 5 步只改 RD head；后续 `InitializeRoyaltyPool` 必须读取并 CAS 同一当前 rights head，初始化每槽当前 holder，不能复制原 `initial_holder`。建池与转让同版本串行：谁先提交，后者重验相应当前状态；两份候选不能各保留一个不同 holder。多个许可/root 仍引用同一原池键。

付款必须读取当前已验证 holder/version。若付款先提交，其合法已付归旧人，转让只移动余额与未来；转让先提交则旧 holder 的新付款候选因版本变化拒绝。已 dispatch/提交未知的付款不能改收款人后重发；保留原 intent/receipt/nonce，恢复其结果再决定剩余权益。累计应得、已付、未付和未知付款分别记录，未知不记作已付，也不当作可以再付。

转让/同人收款授权重绑不引入第二套收入案。新 holder 明示接受仍适用的原窗口/风险，不能要求从取得份额起再开 120 ticks；也不能让转介或这份接受追溯删除已登记主张。历史主体列表永久保留；只在其原程序确实闭合后释放活跃机会容量。转让后的客观收款投影更正回到精确原 rights 证明；有争议的收入/未付身份由许可原程序按其仍适用窗口处理。

`correct_original_subject_mapping` 或后续有权客观更正若影响经济主体，必须核完整已验证转让 prefix，产生新的 rights revision，并与已存在池的当前 holder/收款映射、原程序权利和付款 fence 同根更新；仍受同样的两 instance、写集、完整费用和未知付款限制。纠正旧初始身份的文字/别名不撤销有效后续转让，不能用 `initial_holder` 覆盖 current holder。若原来源纠正使当前主体无法确定，先记录来源变化并按原有权范围阻断该槽新付款，恢复精确来源/原程序；不擅自选旧人或新人成为收款者。已付历史、bps、E 和尾差均不改。

<a id="rd-review"></a>
### 5.6 RD 专属管辖、程序与有限结清

激活要求原世界治理认可的 `rd-charter.review` artifact：world/branch、固定 region/事项、原政策、名册/资格来源、可验证随机及狭义资产处分范围。普通治理 passed、原 charter、服务 panel 或许可 panel 不具此权。名册、随机、预算任一不可验证则不激活，不能先承诺有申诉再临时找人裁定。

初始归因案只包含原冻结的全部必需方（含零收益人、承担本条款程序费用的主体），按真实权利/责任主体去重，不能因关联控制合并两份独立权利。原第三方 grant 的争议仍由其来源 authority 处理。直接受该 grant 条款影响而必须新同意的人在激活时漏列，属于激活前置不成立，不能用名册已满补为有效。

每 charter/epoch 一份归因通知、一案、一个随机排列、一次程序申诉；原v1只有一个epoch。登记在原窗口内明确受争贡献/来源/slot、事实与允许的处分范围；`C` 后的普通证据和程序申诉不扩大为新的经济争点。仅历史署名的登记不能在后续证据阶段临时升级为未登记的份额重分。案件承诺固定后再消费原授权随机源的首个合格结果；排列/证明持久保存。排除当事人、关联 controller、直接经济关系人及相关批准者，初审/复核不得用同一人；补位只按原排列，失去足够合格人不改成普通多数。投票绑定 phase、冻结 scope/evidence root 和类型化处分，2 个相同合法投票才成结论。

时间和人数只消费 [RD 政策 §6](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#rd-review-and-exit)。`N/C/P` 来自原 canonical 提交；对每方受理/证据/程序主张分别保护额度，全部截止前已经 canonical 登记的合格输入均进入冻结 root。网络收到、前端倒计时、host queue 和 ACK 不证明入窗。所有后到方保有同一原窗口，第一方不能抢先冻结自己的子集。

允许的 `confirm_baseline / append_fact_correction / correct_original_subject_mapping / source_right_not_established / release_procedural_preservation` 都绑定精确原范围/receipt。纠正主体映射须来自原已接受条款或合法显式继受，不能制造新受益人、重分比例、替第三方授予知识产权或追扣已付。维护责任、生产限制、补偿和处罚回原 authority，不从 RD 案推导。

程序保全只覆盖直接争议的未付权利、必要来源交接或将发生的不可逆效果，并记录原因、对象和解除路径；本案不冻结全部产业、他人材料或一般 reputation。已经验证且不受争点影响的结果继续；当本身缺少必要来源而不能交接时明确为 `source_not_established`，不能伪装成可由申诉方无限续期的 hold。

初审/复核/final 区分保存。合法初审无合格申诉，或复核截止前未被有效替代时按原政策 final；复核请求本身不撤销原结论。`C+300` 只在查全原案、初审/复核、有效和解、final 及账本后执行中性尾部：有权结论继续，已证明无改变原条款的有效结论时解除程序保全，按原已验证基线处理、原路释放未用预算。final 与硬尾部共用唯一 case resolution effect；已 final 且结清的案件到硬尾部只读返回原结果，不再执行第二次处分/结清。必要来源仍缺失则明确阻断对应新交接；未知提交/结果转恢复，不伪称无结论、重复开窗或罚任何一方。

RD 原案处理原贡献/使用权/初始分配证据；许可案处理其原 fee unit、数量、范围、应付/支付/开放等义务。许可只据已验证 charter/转让更正其本地未付投影，不能裁定新科研贡献或改原 bps；RD 不改许可原 `N/C` 或代签生产 receipt。客观 canonical 纠错可追加并使消费者重验，但不创建第二个有损重审或把已关闭原窗口刷新。转错程序返回原对象/管辖/截止，用户仍须在原期限内登记；转介不新增保全。

## 6. 接口与数据合同

下表是目标逻辑接口，不宣称已有 HTTP、Action、WASM ABI 或正式 surface。对象均有全限定 world/branch、owning instance、kind/local ID、精确 revision/digest；未知影响行为的版本拒绝，不能当 legacy 缺省放行。

| 接口 / 原件 | producer → consumer | 身份与顺序 | 成功 / 失败与兼容 |
| --- | --- | --- | --- |
| `Quote/ActivateCharter`；原 `RdCharterV1` / 后继版本 | 全体当前有权自然/组织主体 → RD authority | 原 terms、完整接受、quote、nonce、scope、`T`、程序/费用 manifest | 原子激活或无效果；旧组织 charter/草稿不转换 |
| `BeginRDWork/RecordRDResult`；`RdContributionV1` | 已适配的原工作 producer → RD 目录 | charter/work unit、原 begin/result、来源与当时身份、时序 | 一项真实结果/终止；未知旧结果不补造新贡献 |
| `CloseRDWork/ResolveHandoff`；`RdAllocationSourceV1` | 原来源/结果/程序 → creation/许可 | 原目录 root、`N`、lineage、原 slots/bps、必要来源/案状态 | 只对精确范围 ready；source/authority/unknown blocker 独立 |
| `Quote/CommitRDShareTransfer`；`RdRightsHeadV1/RdTransferV1` | 原 holder 与受让人 → RD + 许可同根 | 原 slot、expected rights/pool revisions、完整历史主体/程序预算、双方接受 | 唯一新 holder；无池保存 head；不支持部分写入/拆槽/合槽 |
| `RegisterCase/Evidence/Draw/Vote/Appeal/Finalize`；`RdAttributionCaseV1` | 原必需方/合法 panel/调度器 → RD authority | 原 notice/case、`N/C/P`、固定名册/随机、每方 phase 槽、证据 root | 有界结论/中性尾部/原请求恢复；不升级为许可案 |
| `CorrectRDProjection/ResolveRequest/ReadRD` | 完整来源/原请求 → 受权读者 | 原错误/新证据或 original correlation、当前来源/受众 | 客观追加或准确未确定/不可用；读取无业务效果 |

记录保留可读完整条款与原接受，不只保留 digest。source ref 至少能回到 exact producer/schema/world/branch/object/revision、内容承诺、提交/权限和受众。技术记录使用有界枚举状态；人的文案不成为 enum authority，未接通来源明确 unsupported。

<a id="rd-consumers"></a>
### 6.1 直接消费者与两 instance 闭包

| 真实消费者 | 必须消费的事实 | 禁止替代与组合边界 |
| --- | --- | --- |
| [工业 proposal/模拟/试点](industrial-creation-simulation-and-admission.design.md#creation-admission) | 声称 R&D 来源时绑定当时有效 charter/已有贡献；计入新研究的模拟/试点先经 Begin gate | 非绑定 proposal 不算开工；不能等待未来最终交接形成循环，也不能先模拟生成新贡献后倒签 |
| [许可激活](industrial-license-and-royalty-settlement.design.md#license-authority) | 精确 lineage、完整原分配/来源、当前有效交接及原程序结果 | artifact owner、Cargo license、普通 passed、自由 social claim 不替代；仍须满足许可自己的全部前置 |
| [许可收入/池](industrial-license-and-royalty-settlement.design.md#license-settlement) | 原 source/slot/bps 固定，首次建池读当前 RD rights head；后续收入只累加原 E | accrual 不为每批再调用 RD 模块；新 charter/revision/holder 不新建池或刷新开放 |
| 许可付款/转让/身份纠正 | 当前同根 rights/pool head、精确已付/未付/未知 writer、原程序当事集合 | 不以旧快照付款，不先改 RD 再异步更新 pool，不用新请求盖过未知支付 |
| [PPE 发布](../module/player-published-entities.design.md#ppe-publication-authority) | 保留其 current owner/role 和来源许可；明确它只能证明技术发布 | 研究原件不替代发布权限，发布事实不生产全部贡献权 |
| [四主体情境咨询](contextual-reputation-evidence-and-review.design.md#reputation-source-families) | 真实charter/epoch/work/当时actor与mandate/原程序/rights；贡献、使用权、收益及credential分开 | 不给研究或份额转让计入agent_acquisition单位，不把组织结果分给全体成员 |
| API / Agent / Viewer | 相同来源版本/受众下的条款、来源、工作、经济、案、恢复七类事实及下一步 | 不把 holder 变作者/credential，不把 pending 或 planned 变实际收入 |

直接消费者使用原世界同根校验下的有界已验证输入或同根 scoped 接口。完整闭包按真实调用图计费：RD + 许可各最多一次完整 invoke，总计最多两次；无需新 RD 写入的生产/累计读取既有冻结原来源即可。必须做第三次 invoke、访问另一 mutable instance 或额外费用项时先拒绝本次 profile，再由合法新 manifest 完整定额并重新接受，不能称“查询”而免费添加调用。原许可的两 instance 上界和每案主体/费用条件不被本设计放宽。

<a id="rd-resources"></a>
### 6.2 真实 payer、程序分槽与关闭资金

沿[狭义资源 hold](checkpoint-power-service-fulfillment.design.md#service-resource-holds)分别保护 `rd_operational_fee` 和 `rd_closure_fee`；工作生产/模型费用保留在其原来源预算，本设计不把未来研究收入或 royalty payable 当资金。每个真实 payer/resource 独立核算，当前 artifact owner 与调用者不必相同。新增费用主体须纳入它实际接受的条款，不能临时扣 panel 或申诉人私人余额。

激活前保护全部正常迁移、每原方/phase 程序和独立 closure 的最坏上界。原方一次受理/并案、两阶段各政策上限证据、一次程序主张分别留槽；一方的重复/无效请求不消费另一方预算。初审不挪复核，研究不挪程序，费用不挪成果或未来分配。两当前 metering payer 同源可合并，异源逐源证明；改变 artifact owner/费用 schedule 不继受旧 payer 的签署。

preflight 和实际 charge 必须消费同一个准确费用 hold。其他所有扣款入口不能吃掉该 hold，锁定后也不能让 preflight 错读为零或再从 hold 外扣一次。成功只扣 actual；相同请求、确定拒绝和无变化重试不重复业务费用。原协议反滥用/限流独立且有界，不变成隐藏申诉押金。

closure 单独覆盖工作结果核对/合法源 fence、完整 notice/案 final 或尾部、来源和原 slot 保存、原来源未用 hold 释放与最终 receipt。每次受保护关闭迁移至少终结一个原对象或推进一次合法阶段；不能通过任意拆片耗尽预算。最后一次释放先保留本次实际费用；没有合法到账账户只记录原权益/恢复，不收费轮询吃空旧款。

已闭合 charter 的转让、同人收款重绑、必要客观更正或原 `handoff_blocked` 后补齐前置的首次交接，各是另一次有限原子迁移，在生效前取得当前完整费用 cap/同意；成功原子扣 actual、释放未用量并留 receipt，没有新无限后台义务。后到首次交接沿原条款/成果/程序结果，不复活原已退预算、重开期限或承诺新研究。许可尚有原案时，新增主体所需的程序预算增量另按原许可公式/来源保护，不能包含在“一次转让 invoke 费”里假装已付。原权益的读取/原请求查询不以另交一笔世界费用为条件，服务资源仍按 §9 有界隔离。

## 7. 状态、事务与持久化

工作（未激活/active/停止新开工/关闭）、来源（已验证/缺失/未知）、分配（原冻结来源/可交接/具体受阻）、rights head、归因案和费用状态分别持久化。一个 `ready` 不掩盖其中其他维度。协议的原请求身份与业务 effect 键并存：激活为原 charter/revision，结果为原 work unit，notice 为原关闭，case 为原 charter/notice，转让为原 slot/expected rights revision/完整转让 digest；不同 request ID 不能重复相同 effect。

所有这些 authority 写入与相应费用、原件、索引、receipt 在原世界事务中提交。先准备全部合法候选再发布；任何子项已证NotCommitted时丢弃整个未提交候选；Committed安装原完整候选/receipt，StatusUnknown保留原候选并按§7.1隔离整个不确定旧World。持久化保留原顺序、nonce、随机证明、原 notice/截止、已付/未付/来源和历史主体。读 cache 不能覆盖 durable 原件；重建索引必须核完整原件闭包，不能用缺项缓存“修复”来源。

<a id="rd-recovery"></a>
### 7.1 原请求、未知付款与源恢复

提交前持久保存完整签署 payload、correlation、expected revisions、world/branch 和预算绑定。丢 ACK、断线或重启先查询原请求；committed 恢复原 receipt，pending/unknown 保留原件和受影响 writer fence，不重签新 nonce 或凭当前 holder 猜测结果。只有原受信协议明确允许的原 payload 重送才可重送；不把 HTTP 404 或本地 cache miss 当世界 Unknown。

RD/许可两侧先后 ACK 不证明部分生效；以同一 canonical commit 读回两版本、费用和原 receipt。仅 ACK、业务请求或某个受权投影未知时，保留对应原请求/slot 的转让与付款 fence，恢复精确结果；不把缺读回处理成退款或重复应用。若是 world durable generation 已为 `StatusUnknown`，则严格沿 [原 G1 writer fence](runtime-storage-footprint-governance.design.md#storage-generation-g1-design)停止整个不确定旧 World writer 的所有后续 mutator/save/依赖派发，核清精确候选的持久决定并使内存/索引与之一致后才恢复；不能只冻 RD slot 而让无关写继续。恢复沿原合法当前执行版本和原条款限定处置，不授权已撤销代码重放，不另造一份持久化协议。

Begin/完成、关闭/notice、随机/排列、final/费用释放及转让/池/付款分别注入 prepare、commit、publish、ACK 和 checkpoint 故障核对。时间过去不重置窗口；未知工作必须先沿其原工作 adapter 证明结果或有效 fence，RD 不重调模型、不重做实验。source unavailable 保留明确恢复入口，不能成为原 actor 无限追加 hold 的权力。

## 8. 部署、安全与运行约束

本 profile 必须在 canonical authority、合法治理/随机、完整 source adapter、受保护扣款和最多两 instance 原子提交全部具备后启用。只部署 Viewer、测试内 JSON 或一个可运行 validator 不构成激活。所有正式入口包括直接 API、Agent、自动消费者、模块和许可付款均需接同一 current gate；任何仍可绕过的入口使相应写能力不可启用。

服务对外验证签名/原主体、audience、字节/对象上限和原请求去重。公开 response 不包含秘密原件或前任私有目标；获选 panel 只在原案 scope/任期内取证。canonical 日志/存储所需原件遵循其原访问合同，不宣称普通 hash 或 ACL 提供密码学保密。永久因果保留依赖原世界归档/恢复合同；源不可用时诚实报告，不静默降低证明强度。

## 9. 质量、容量与可行性


玩法人数/工作/程序上限只取 [RD 政策](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#rd-charter-policy-v1)，后继每epoch仍16方/32单元、原64名册/512证据/8槽。原128KiB是current state原存储硬上限；它**不能直接作为可调用的raw预算**。[ModuleCallInput/ModuleOutput](../../../crates/oasis7_wasm_abi/src/lib.rs)的state/action/new_state当前是无bytes专用序列化标记的Vec<u8>，[canonical codec](../../../crates/oasis7_wasm_abi/src/canonical_cbor.rs)排序map而不把数组改成byte string；[实际route](../../../crates/oasis7_runtime/src/world/module_runtime.rs)先编码命令再把其bytes放进完整ModuleCallInput。单raw字节可编码成两个CBOR bytes；原“128+64+48+16=256KiB”忽略这层放大，不能继续作为容量证明。

本轮保留128KiB硬上限与全部原程序人数/证据义务，把可激活形状改为**完整current body≤96KiB、闭合compact head≤8KiB**；这改变的是错误的工程分配，不削减任何方证据槽。原件按原受众保留在已提交journal/内容目录，紧凑引用不是删除正文。以下分额含inner canonical结构开销：

| current body 部分 | 后继固定最大分额及完整保留 |
| --- | --- |
| 两阶段各16方×16证据，共512项 | 每项≤96bytes，共48KiB；actor/phase/ordinal、原件与登记receipt内容承诺、原顺序。原256byte ref和正文留原件 |
| 64位名册/排列/每阶段资格与投票 | 每位≤192bytes，共12KiB；完整资格/同意原件可验证引用 |
| 16方身份/角色/接受/actor-phase计数 | 每方≤256bytes，共4KiB；稳定principal和完整接受/mandate用精确内容目录，不截断身份或签名 |
| 32个工作单元 | 每单元≤256bytes，共8KiB；begin/result/state和真实source目录 |
| 8原槽/current holder/rights/原池 | 每槽≤256bytes，共2KiB |
| terms必要规则、最多4epoch目录、notice/case/nonce/费用/请求元数据 | 共≤16KiB；4条epoch索引各≤512bytes包含在内，不另加 |
| 合计与保留余量 | 90KiB +6KiB编码/演进余量 =96KiB，不再使用118KiB旧分配 |

紧凑body的32byte内容承诺在该新制度schema中明确编码为CBOR byte string，证据tuple为(actor,phase,ordinal,source_commitment,registration_commitment,canonical_order)；禁止默用Vec整数数组后仍声称每项≤96bytes。长身份和完整接受用受权可解析内容目录，目录键与原exact ID绑定。此inner编码由该制度的版本化codec实现，不能改变原host ModuleCallInput的Vec wire语义。

完整terms/render、接受/签名、mandate/来源正文、全部证据与已关闭epoch持续可取回；在激活/登记时与相应compact项核对，后续使用的是已验证完整prefix及当前有效性，不是任意自报hash。稳定ID过长时保存原exact ID的可验证内容对象，不以trim/截断/改名去重。当前case必须预留最后一方最后一个合法证据和全部关闭字段，不能先受理511项再拒绝第512项。一个合法source的proof若无法在下述完整命令预算中验证，须在该新承诺前解决可验证有限格式或拒绝具体来源组合，不能向原已受理当事方追缴容量。

**完整wire分配。** 定义A(N)为当前Vec整数数组的编码长度，N≤131072时有A(N)≤2N+5；这是保守传输界，仍须实际encode完整ModuleCallInput/Output，不能只算raw。普通调用只允许action/event之一携带命令，不重复两份；完整action bytes已经包含ModuleCommandEnvelope、typed命令、真正需要的terms/接受/proof/refs与全部内层编码，普通≤24KiB。host ctx、outer map/key/flags等全部另外≤15KiB，不能把未知外层开销当0。

| 完整调用形状 | 有界分配 | 不超过256KiB的可复算界 |
| --- | --- | --- |
| 普通工作/证据/投票/转让/关闭 | current body96KiB、完整action24KiB、完整ctx/外层15KiB | A(96KiB)+A(24KiB)+15KiB ≤261130bytes <262144 |
| 初次或下一epoch激活 | 空state或已验证compact head≤8KiB；完整action≤112KiB，含新common body/全部接受/proof；完整ctx/外层15KiB | A(8KiB)+A(112KiB)+15KiB ≤261130bytes <262144 |
| 完整输出 | new_state body≤96KiB；已编码effect≤32KiB、emit≤16KiB、输出其余字段/outer≤15KiB | A(96KiB)+32KiB+16KiB+15KiB ≤261125bytes <262144 |

ModuleCommandEnvelope.payload本身也是Vec<u8>，须再经过一次数组编码；上述24/112KiB是**已经编码好这个命令封套**的action上限，不是payload raw。可保证的普通payload≤11KiB、激活payload≤55KiB，命令namespace/name/version/hash及结构总额≤1KiB，分别经A(payload)+1KiB小于24/112KiB；其中全部typed条款、render、签名/nonce、原件refs和当前证明先合入payload真实编码。不得遗漏payload→命令→ModuleCallInput的两层扩大。

activation的family terms仍≤32KiB，common完整body/接受及证明还须服从112KiB的**全部action封套**，子上限不能同时用满再额外附签名。普通已激活操作不重复全部历史签署body；使用已验证的固定规则与本次完整相关proof。每份完整proof引用≤256bytes、说明≤4KiB、受权分页≤50保持；普通24KiB必须包含当前所需全部权限/来源验证，不能将大证明“异步补查”后先提交。原32KiB terms、64KiB command等旧子上限不是每次调用保证：本轮明确以该完整wire形状收紧原错误分配。

前epoch完整工作/程序已闭合后，在独立closure128次内保留一次有唯一原业务键的CompactClosedEpoch。先验证原已提交完整case/terms/证据/结果及最终决定可按audience回读，保留原历史根和收尾receipt，再同根将current state缩为≤8KiB的原分配/rights/epoch目录head并结清本次actual fee/未用hold。完整旧state及最终费用/closure原件仍在原可审计保留链内，不能只留不可解析root。没有可靠原件、未知旧提交或未完case不得compact/释放工作槽。compact是计费调用，不是免费后台清理；重复请求只查原结果。下一epoch Activate读这个已提交head，不能同时塞96KiB旧案和112KiB新接受。

合法完整编码正例以[RD10](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd10-encoded-capacity)规定的16方/64名册/512证据/32工作/8槽tuple目录及真实source引用、常规/激活命令作独立encode；每项按表的完整inner长度界，合计≤96KiB。独立codec形状算例含16方各16批准签名、64名册、512证据、32工作和8槽，得到body55271bytes、普通完整action8608/input121923/output104366；归档head1211bytes，下一激活payload45792、action90902、完整input183759bytes，均在边界内。它使用确定性合成内容承诺/证明字节来检验完整嵌套编码，未证明签名/真实source/业务实现；真实候选必须按同形状的合法原件另验。使用任意0xFF填充的128KiB raw state仅其数组就达262149bytes，连空ctx/命令都超上限，必须拒绝为该目标不可调用形状，不能谎称可装进256KiB。真实manifest/codec必须交付相同正向shape的编码和调用结果；静态界不是已实现ABI或性能证明。

完整输出总长由真实serialization复核，不能信module自报output_bytes或把new_state排除在计费/输出上限外。每次迁移变动项仍≤128，包含rights/案方/预算/source/fee/receipt索引；超限在新接受前拒绝，不截断旧历史或减少原方机会。source读取、两invoke、native apply、commit、原件保留和恢复都按实际合法manifest计费，不引入免费第三invoke。每个已接受case仍保留全量程序/原件空间与退出预算。

<a id="rd-capacity"></a>
### 9.1 程序、费用与集中输入的组合上界

同一 world 的本技术 profile 最多 4 份尚未闭合工作/程序/费用的 charter，另最多 8 个待决转让/更正/迟到首次交接权利写请求。关闭后历史/权益继续存在，不占原工作名额；已明确的 `handoff_blocked` 可沿 §5.4 结清关闭，未知原提交、未完程序和费用不得冒充关闭来腾槽。单 charter 当前epoch的归因案完整主体数为 `m≤16`、名册为 `R≤64`；原v1仅有此一个epoch。程序成功迁移的保守上界：

```text
K_case = m × (1受理/并案 + 16初审证据 + 16复核证据 + 1程序主张)
       + 2R投票 + 2R资格排除/顺延
       + 1随机证明 + 2证据冻结 + 2结论发布
       = 34m + 4R + 5 ≤ 805
```

这是完整原方程序预算，不是假定每案实际执行 805 次。批量证据可降低实际调用，但激活不能假定当事人会合批。每方有自己的 actor/phase 槽，零收益方也完整拥有；相同证据重试不再占槽。初审与复核各有一次原排列的合法投票/资格处理预算，不能用重新抽样无限延长。

普通成功迁移最多 `1激活 + 32开工 + 32结果 + 16各方停止后续授权 + 1关闭/notice + 1初次交接 = 83`。工作 producer 自身成本、停止/fence 与安全处置由 Begin 前核验的原预算承担；83 次不替它免费执行。正常及完整案共 `83+805=888` 次。关闭另预留 128 次，覆盖最多 32 个原工作终态核对、16 个角色关闭、32 个来源目录封存、16 个原 fee/source/purpose 释放、8 个槽保存，以及关闭/notice、case final/尾部、交接最终状态、最终 receipt 各一次，加一次完整原件核验后的CompactClosedEpoch（合计109，仍≤128）。正常已完成的同一 effect 不重复产生关闭效果，余量只用于原合法收尾。

一次转让/更正/迟到首次交接是一个完整原子逻辑迁移；自身最多两 invoke，实际许可程序增量单独预留。原 83 次中的首次交接若未能完成，关闭时可以释放其未用预算；后来首次交接由新签单次预算承担，不在原 83 次之外暗记一次免费写入。当前许可源/案件/预算不能在此迁移的 128 项写集、两 instance、proof/输入上界内闭合时不接受，不暗中异步更新。长期转让史按原件和完整 current-head 索引保存；新写入有自己的有限成本，不靠删除旧持有人或延长旧案获得容量。

采用当前算法可复算的完整 invoke 上界：input ≤262144 bytes、max_output_bytes=262144（含候选和可能 new_state）、max_effects=1、max_emits=1。每次 preflight 为 `256+256+2+1=515 Data`、`1+1+1+1=4 Electricity`；一个逻辑迁移至多两次，即 `1030 Data + 8 Electricity`。两次的当前 artifact owner 分别授权和足额供给，不能把调用者或新 holder 当默认 payer。原技术交易费/新 component 访问若有额外适用项目，必须按真实 schedule 加入 signed cap；最小 profile 需要该 scope/version 无其他未列收费的合法证明，否则当前预算不支持激活。

```text
一份 charter/epoch 的正常+程序 = 888 × (1030, 8) = (914640 Data, 7104 Electricity)
独立 closure             = 128 × (1030, 8) = (131840 Data, 1024 Electricity)
合计                     = 1016 × (1030, 8) = (1046480 Data, 8128 Electricity)
单次原子转让/客观更正/迟到首次交接上界 = (1030 Data, 8 Electricity) + 原许可需要的合法程序增量
```

后继最多4epoch顺序执行，每个新epoch重新完整接受/保护本次预算；生命期最多4×1016=4064个此类成功逻辑迁移，上界合计4185920 Data/32512 Electricity，按实际逐源分期保护，不从第1期免费覆盖未来。一次CompactClosedEpoch已在每期128closure内；新Activate已在该期83普通迁移内，不另造未计费过渡。四epoch不并发，因此同world的4个active charter/epoch与8个权利写请求组合上界不乘4；长期独立权利写仍各有自己的有限预算。

这些是最坏保留量，不是固定收费。以下1024bytes例仅展示费用公式的条件算术，不证明16方/512证据shape只占1024bytes；正向完整形状见RD10。静态例：同一合法 payer 承担两个 instance，真实 12 次成功逻辑迁移（含收尾）各使用两 invoke，actual 输入/输出各 1024 bytes、1 effect/1 emit 且有 new_state，则每 invoke 为 5 Data/4 Electricity；24 次共消耗 120 Data/96 Electricity，返还未用 RD 预算 1046360 Data/8032 Electricity。源研究工作自己的费用另算，例子不把它设为免费。不同 payer 必须逐源拆算。此保守预留可能远高于收益，尚无玩法承受或价格平衡证据。

处理容量单独保护：每有界批最多 64 个正常/程序逻辑迁移，独立保留 16 个 closure 迁移；一个可服务 canonical tick 须具备 64 批的已验证能力。最坏把 4 份 charter 的整个剩余正常/程序集中输入并加 8 个权利写，`4×888+8=3560≤64×64=4096`；全部 charter closure 为 `4×128=512≤64×16=1024`。这是比合法逐阶段同 tick 更宽的保守包络，不用窗口平均数证明末 tick 突发，也不表示当前 Runtime 已达到该能力。

读/无效/重复候选、新可选 charter 与已 canonical 登记的合法程序/恢复分别使用有界 admission/读取队列；每 actor/phase 隔离。攻击性流量不得耗尽原方程序与 closure 份额。截止按 canonical 登记判断，服务背压只能返回未受理，不能给未入窗请求补旧 tick。即使一个 tick 可有多个有界批，也不承诺现实时间 SLA；真实环境不能满足上述 worst-case proof/调用/写集/持久化/干扰组合时，不启用新 profile。关闭 capacity、执行版本或 fee 变化必须保留已接受义务的 drain，不能升级后发现旧预算无从退出。

## 10. 兼容、迁移与回滚

旧组织 charter、native 社交记录、artifact publisher、单独 model output 或没有本 profile 绑定的旧 receipt 继续保留原含义；不自动转为 R&D、初始 bps 或可收费来源。缺来源/授权/时序的旧数据可只读解释为 legacy，不补签、倒写或追溯收费。未知新字段/状态的消费者不得降级成 ready；不支持时返回明确 blocker。

启用按 reader/原件可取回、受保护费用/当前 gate、source producer、同根 RD/许可消费、正式表面及恢复顺序验证。旧许可未接当前 rights/version 时本 profile 的转让不可启用。停用先停止新激活/工作/转让，保留原请求、原工作安全处置、程序、读者、费用与权益 drain；不能删除 provenance 或以回滚代码改写旧已付。行为/预算升级须有新合法 manifest 和相应同意，不能削减旧程序或用新版零点重置 E/窗口。

## 11. 验证策略与追踪

验证按 [R&D 手册](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md)分别记录静态设计、真实 source/消费者、恢复和正式表面。现有 [许可手册](../../testing/manual/industrial-license-and-royalty-settlement.manual.md)作为直接集成前置复用；只测 RD validator 不证明激活、建池和付款。当前文档检查及下表是方法，不是业务通过证据。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [激活与工作时序](#rd-charter-activation) | 全体同版接受、开工顺序、拒绝零效果 | [RD01](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd01-activation)：同候选真实激活/源开工，边界与并发反例 | 原件/顺序/授权、前后 root 和 receipt | 旧草稿或独立 validator 不证明正式链 |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [贡献来源](#rd-contribution-source) | 来源、贡献和经济分离，客观更正不重写事实 | [RD02](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd02-provenance)：真实源/缺项/伪造/先存与更正对照 | 完整 source/grant/结果、断裂与拒绝差分 | 不证明科研结论或外部法律权利 |
| [AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [Agent provenance](#rd-agent-provenance) | 当时身份/来源永久因果及受众隔离 | [RD02](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd02-provenance)：同源 Agent 转让/退休、未知训练源和多 audience | 历史/当前 binding、授权读回和屏蔽 | 不以 alias 证明完整训练资料 |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [冻结交接](#rd-distribution-handoff) | 关闭与原 notice、完整来源、冻结分配 | [RD03](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd03-handoff)：同候选真实 proposal/付费许可/池初始化正负组合 | 原关闭/目录/来源/notice、许可/池原件 | 缺工业准入与收费 adapter 不能代签 |
| [AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [显式转让](#rd-share-transfer) | 同根 holder、旧权益、历史当事机会不丢 | [RD04](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd04-transfer)：真实许可建池/付款与转让竞争、未知及上界 | 双根同 commit、E/slots/已付/未付/主体与预算 | 不支持拆槽/合槽或第三 instance |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [归因复核](#rd-review) | 本地受权、随机无冲突、完整窗口与有限尾部 | [RD05](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd05-review)：原名册/随机/窗口/无结论及跨程序反例 | 原案/证据/随机/final/解除或恢复记录 | 不能以 fixture 证明真实管辖已激活 |
| [AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [原请求恢复](#rd-recovery) | 崩溃/重启/旧数据和未知付款不复制结果 | [RD06](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd06-recovery)：固定候选、各提交边界及持久 checkpoint 恢复 | original payload/receipt、前后 root、历史与费用对账 | 只内存重试不证明持久恢复 |
| [REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | [直接消费者](#rd-consumers) | 提案/付费/建池/付款各取真实来源、最多两 instance | [RD03](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd03-handoff)：正式全部入口及实际调用图同候选走查 | source→consumer 原件、调用/费用/当前版本 | 一个 handler 通过不证明所有入口 |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [资源与预算](#rd-resources) | 真实 payer、每方机会、独立 closure 和容量组合 | [RD06](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd06-recovery)：满额/缺一/异源/并发扣款/最大窗口突发 | 每来源守恒、actual/upper、调用/处理和 drain | 静态算例不证明性能或经济可用 |
| [AC-RD-08](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-08) | [rd-charter-activation](#rd-charter-activation) | 组织/自然多方真实当前mandate、source owner与同版接受 | [RD08](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd08-organizations)：真实组织与自然16方、换代表/共享source、无环digest/两invoke；同候选真实source/consumer | 签署与mandate原件、真实逐源资源及拒绝零效果 | 不从OC grant/名字自动创造代表或共有资金 |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [rd-charter-membership-epochs](#rd-charter-membership-epochs) | 原采纳的开放加入/退出和未来条款变化有正向执行 | [RD09](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd09-membership-epochs)：0收益与整槽后加入、退出、4epoch和并发/unknown反例；同候选真实source/consumer | 原新terms/接受、历史工作及原经济账连续 | 原v1不追溯升级；不改同lineage经济向量 |
| [AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [rd-charter-membership-epochs](#rd-charter-membership-epochs) | 新epoch及组织变化后完整原件/永久provenance与旧权利不丢 | [RD09](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd09-membership-epochs)：受众取回完整旧原件、同pool/E与第5阶段拒绝；同候选真实source/consumer | 原目录/正文/历史读、原case截止与付款对账 | 不以摘要或现代表代签历史 |
| [AC-RD-09](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-09) | [rd-charter-membership-epochs](#rd-charter-membership-epochs) | 4epoch变更边界和独立新成果不洗旧权益 | [RD09](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd09-membership-epochs)：同scope改名/真正独立lineage正反例、旧阶段未闭合；同候选真实source/consumer | 拒绝/原件与独立来源、实际world容量 | 不改名绕过容量或另开旧池 |
| [AC-RD-10](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-10) | [rd-capacity](#rd-capacity) | 完整canonical wire及每phase全程序/关闭成本可行 | [RD10](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd10-encoded-capacity)：16方/512证据/两invoke封套、compact再激活、最大集中输入与超限；同候选真实source/consumer | 真实编码长度/费用独立算表、全量程序/closure及原件 | 不假定raw128KiB能直接装256KiB封套 |

## 12. 决策、风险与复核触发

原v1选择固定预协商向量、自然主体和单epoch。后继补齐普通组织代表/多方及最多4个顺序epoch，正向加入/退出/未来变更沿明确新接受与旧完整历史，经济向量跨epoch继续冻结；不采用事后模型分配、按当前artifact owner归属或多数投票改权。合槽、附价款和改变同lineage bps仍不在本版，不能将它们伪装成开放加入，也不把已采纳加入继续推为未设计。

| 具体剩余条件 / 风险 | Owner | 解除或复核触发 |
| --- | --- | --- |
| 原来源、个人/组织当前授权、RD 管辖/名册/随机尚无完整运行证据 | Runtime、Agent、治理/P2P | 正式适配器/制度激活前按 RD01/02/05 验证真实生产链；缺失不启用 |
| 两 instance 同根写、未知付款 fence、程序主体/费用增量及全部扣款 gate 尚需实现 | Runtime、许可 owner | 接通任一转让/许可入口前以 RD03/04/06 证明完整闭包，不能分步上线弱化版 |
| 最坏费用显著高于潜在收益；容量与保留来源成本尚未实测 | gameplay、Runtime、QA | 用实际候选验证 §9 最大输入、干扰和 payer；玩法初值改变须新政策及旧权兼容 |
| 训练/外部原件可能只可声明或不能读取 | Agent、原来源权利 owner | 对该具体成果的必要权利证明复核；如实保留 unknown/unavailable，不补造训练授权 |
| 玩家可能混淆贡献、holder、到账和个人资格 | gameplay、Viewer、QA | 按 RD07 的正式三入口和目标玩家任务验证，失败修读面/文案后复测 |

这些是已明确目标规则的实现/证据条件，不是让后续执行者自行决定必要玩法政策。原CR003的社会预览、一般组织/岗位来源与机会接收由[完整情境声誉合同](contextual-reputation-evidence-and-review.design.md#reputation-source-families)及各直接专业合同组合；本设计完成其R&D已采纳目标选择，不单独代签整卡和实际实现。
