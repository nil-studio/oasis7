# Agent 经济转让、退休与报废系统设计

- 状态：active；本文是正式目标设计，不是已实现、已部署或业务运行证据。
- 主责：runtime_engineer；共同审读：gameplay_designer、agent_engineer、security_engineer、qa_engineer。
- 审读实现基线：`b3658f94a7755fc766fbefe95e83a24566e6068c`。原 AOS-005 与 AUTH-002 的范围不因制造首版、入口支持或当前数据形状而缩小。

## 1. 问题、目标与非目标

经济 claim 的创建和释放、技术 identity 的更新、任务 context 的退休、制造在制品的报废分别有自己的事实，不能拼成既有 Agent 的经济转让或处置。本文提供同一权威世界内、一个既有 Agent 的完整来源合同：由真实有权主体提出完整条款，在资格、容量、资源、治理、反滥用五门槛和全部必需接受同时成立后，一次交割指定权利；退休和报废沿原身份保留历史、资产去向及责任。未生效、已生效和结果未知可准确恢复，任何入口不得以换 owner、重连或旧 snapshot 洗除历史。

成功的交割保留稳定 Agent ID、注册/制造/取得来源、已生效决定和未结责任；只有明确取得的新经济权利及相应控制来源从生效点开始变化。新 owner 的目标、Prompt、委托和私有上下文仍须按原权限显式建立。本文不引入交易所、拍卖、租赁、跨世界账户迁移、新币、新税权或治理对私人资产的普遍处分权，不以自动补偿替代原子提交。

## 2. 上游约束与相关角色

原 [AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005) 拥有经济转让五门槛和连续性；[AUTH-002](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002) 拥有处置后身份、配置、责任与历史结果。玩法金额、冷却和库存处分采用 [Agent claim 取得用途](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#agent-acquisition-context-source)及[本次首版处置政策](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#agent-disposition-policy)，不由本 Runtime 文档另定价格或退款率；该政策是本次供原 Gameplay owner 评审采纳的明确提案，不是历史批准或已启用事实。形成使用 [CR001 完整条款](agent-mediated-contract-formation.design.md#contract-formation-common-terms)、[真实主体/mandate](agent-mediated-contract-formation.design.md#contract-formation-principals-mandates)、[完整接受](agent-mediated-contract-formation.design.md#contract-formation-required-acceptances)与[原 D/O/C](agent-mediated-contract-formation.design.md#contract-formation-cumulative-exposure)。既有服务/R&D/工业/组织 obligation 由各原域接收，不能由处置模块宣告履约。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | 真实经济主体、当前代表及完整同版接受产生可执行处分来源 | [来源与形成](#ad-authority-formation) | claim/注册/制造、CR001、当前 authority | 控制绑定、组织标签、任意 Passed 不构成资产权 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | 完整请求在当前五门槛内；失败/过期/待决不给部分交割 | [五门槛](#ad-transfer-gates) | 原取得用途、共享容量、世界/治理政策 | 不取权限交集后静默缩单 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | 原库存、押金、资助目的、已付/未付与旧责任完整保留 | [经济清单](#ad-economic-inventory-continuity) | 原 ledger/hold/服务/R&D/组织消费者 | 不把旧债、受托物或 restricted 资金默认交给买方 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | 唯一生效点发布新 owner/source/generation 和全部控制 fence | [owner 生效](#ad-effective-owner-generation) | IA011、真实 current owner/task/executor | 交割本身不签新 grant 或目标 |
| [REQ-AGENT-AUTH-002](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002)；Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | 退休/报废停止新运行，保留资产去向、身份和责任 | [退休与报废](#ad-retire-scrap) | 原玩法处置政策及领域 drain | 不把 task retirement 或制造 WIP disposal 当作 Agent 终态 |
| [REQ-AGENT-AUTH-002](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002)；Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | 已用配置、旧结果与新控制准确分开，私有 provenance 不丢失 | [owner 生效](#ad-effective-owner-generation) | IA 配置/目标/记忆消费者、原受控存储 | 不移交旧私有内容、账户、key 或 provider session |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | 所有相关 writer 接收同一当前权源和原负担，无法证明闭包即阻塞 | [writer 闭包](#ad-all-writers) | 各原 native/module/服务 writer | 不新增世界全扫描或万能协调调用 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | 原请求/业务键、一次效果、未知和同代恢复 | [事务与恢复](#ad-transaction-recovery) | Runtime 原事务、DWE、G1、正式 authority | 本地 Committed 不代正式 finality/serving |
| [REQ-AGENT-AUTH-002](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002)；Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | 处置恢复不洗旧配置、身份、结果和责任 | [事务与恢复](#ad-transaction-recovery) | snapshot/journal/原件/私有历史引用 | 不以空 snapshot 证明历史未发生 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | 有限完整调用、原费用与独立 capability 预算均可支付并保留退出 | [容量与计费](#ad-bounds) | actual ABI、manifest、原 payer/schedule/usage | 工程 cap 不是生产费率或真实收费量 |
| [REQ-AGENT-AUTH-002](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002)；Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | Viewer/native/API 可读同一生效点和有权历史，未知有真实下一步 | [读模型](#ad-read-model) | IA011 读者、原查询/认证 | 不把入口图稿或 SDK 能力当作已接线 |

## 3. 当前状态、目标状态与差距

| 当前真实来源 | 已有事实 | 本设计目标与准确差距 |
| --- | --- | --- |
| [claim action](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_gameplay_claims.rs)、[经济 publication](../../../crates/oasis7_world_state/src/world/agent_claim_economic_publication.rs)、[terminal publication](../../../crates/oasis7_world_state/src/world/agent_claim_terminal_publication.rs) | claim 要求目标尚未被 claim；upkeep 保留原 owner/资金来源；release/reclaim 移除旧 claim 并结算原退款 | 没有双方经济转让的完整事件/写集；release 后另 claim 不是本文事务 |
| [participant 原子绑定](../../../crates/oasis7_runtime/src/world/participant.rs)、[current owner resolver](../../../crates/oasis7_runtime/src/world/industrial_control.rs)、[WorldState 来源](../../../crates/oasis7_world_state/src/world/industrial_control.rs) | 首次注册 receipt 固定账户、Agent、generation；制造 owner 从 claim/control_source/root 推导 | 注册是控制来源，不能无中生有生成可出售经济权；新经济来源须由 resolver 明确接收 |
| [identity event](../../../crates/oasis7_runtime/src/world/capability_agent_identity_events.rs) | 普通 identity 单调更新；制造 identity 不能直接安装，root 变化会推进关联 checkpoint | 单独调用 helper 不能形成交割；必须包含经济与所有受影响控制依赖 |
| [制造交付](../../../crates/oasis7_world_state/src/world_rules/industrial.rs) | DeliverAgentOrder 在完整制造条件下建立目标 Agent/claim；未交付 WIP 的 Dispose 有独立规则 | 既有 Agent 的转让/退休/报废不复用 WIP 终态；已交付来源保持 |
| [AgentState](../../../crates/oasis7_world_state/src/agent_state.rs)、[AgentCell/intent](../../../crates/oasis7_world_state/src/agent_cell.rs) | 身体/slot/cargo、资源、mailbox、intent 分属真实字段 | 没有将这些字段全部安全交接/保留的现成生命周期合同；不能直接 remove Agent |
| [IA 配置与历史接点](indirect-control-agency-execution-and-continuation.design.md#agent-disposition-control-history-consumer) | 当前 task/identity 有各自 fence；Prompt 当前 ACK 不证明 durable 跨 owner 历史 | 新配置归档、控制切点与三端 reader 是目标适配，不能据现有本地 ACK 代签 |

## 4. 边界与结构

<a id="ad-authority-formation"></a>
### 4.1 真实来源、明确签署与正式命令

目标采用原 CR001 的一个 typed family `agent_disposition_v1`，每份条款只处分一个稳定 Agent；不是字符串解释器，也不复用窄版即时资源交换的签名。immutable body 固定 world/genesis/branch、Agent/原取得来源、处置类型、旧经济 principal/源 actor 和新 principal、全部 asset/obligation/control 集合及逐项处置、全部价格/claim/运行费用、原上限/期限/退出、policy/manifest/activation/schedule、预期 owner/generation 与完整必需主体。body → terms digest → 确定完整中英 render → acceptance → request digest 单向计算，派生 receipt/finality wrapper 不进入自身摘要。任何范围、责任、费用规则、payer、前提或政策改变须新 revision/重新接受。

旧欠费、退款和未覆盖upkeep期使用该精确policy的typed结算公式，签定原claim/rate/source、逐项最大数量/金额和可执行窗口，render同时给出公式、当前预测与最大暴露。Final的canonical c/实际paidThrough是公式输入，必须来自同一当前parent；这不是只签一句“低于cap即可”。只允许原公式和明确范围内的状态演进，不以自由文本执行新计算；slot rate、价款、source、policy或责任范围变化仍需新修订/接受。

这是按原形成[扩展决定](agent-mediated-contract-formation.design.md#adr-cf-01：先闭合即时形成与履行)与本次[处置政策](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#agent-disposition-policy)提出的独立新family候选；原CR001已列六族和旧即时交换排除范围不被静默改写。适用组织签约附件及mandate必须显式包含`agent_disposition_v1`的准确family/action/object/产权source/期间/累计范围；新增范围由原有权方按原附件修订规则完整批准，并取得本次所有必需方接受。旧即时/服务签名、旧family通过、wildcard标签或普通组织资格不代签新产权处分；未完成正式新版本采纳/接线时本family不可用，不借另一个已上线family执行。

卖方必须同时有真实经济权源和处分权。`claim_owner_id` 是原 claimer Agent ID，不是浏览器 player、组织、登录 session 或最终自然主体；当前 owner binding/稳定主体、当时取得事件和当前控制另行核对。具有可验证经济权的普通、制造及主 Agent 使用同一合同；只有首次控制绑定而无经济权的 Agent 不能凭绑定出售。该主体可依法停止自己的控制和配置，涉及实体资产退休/报废仍须真实财产处分来源；不把无权处置解释为没有历史。

组织沿 CR001 当前业务签约附件、真实治理 payload、批准 roster/任期和当前 mandate；个人源沿原 owner/signature。组织名、成员资格、任意 proposal Passed、普通治理投票权或 OC 余额都不授此处分权。资产、费用账户、受托物或未付债权属于第三方时，须保留其现有合法权利；本次修改其范围/风险/收款条件则加入必需接受方。从完整 write/hold/obligation/fee 集推导 required principals，不能相信客户端只列买卖两人。新受让方明确接受所取得权利、未来维护及每一项明确继受，不能用卖方签名或模型输出代签。

正式调用复用真实 [CapabilityAuthorityRecord/FinalityProof](../../../crates/oasis7_world_state/src/capability_authorization.rs)及 [proof-bearing install/revoke/rotation](../../../crates/oasis7_runtime/src/world/capability_authorization_admin.rs)。finality proof 先绑定历史 validator/epoch/proposal/manifest 与完整 issuer record，才可信其 issuer；该技术来源仍不替代上述业务产权和政策批准。原 issuer 签 [CapabilityGrantV2](../../../crates/oasis7_wasm_abi/src/capability_v2.rs) 时固定本 family 的精确 module/version/namespace、object_kind=command、operation=execute、命令名、subject/owner-generation、world/branch/instance audience、完整 entity/resource selectors、policy_class、payload 上限、expiry/revocation 与grant自身canonical body hash。业务payload/terms摘要另由原请求和完整接受绑定，不能把grant的body hash说成自动校验业务正文。parent只能同subject衰减，不能把卖方grant改给买方；旧无proof安装、validator-admission专用授权、node身份或模块安装不能替代这些来源。

### 4.2 组件职责与完整性证据

领域模块产生业务候选；各原 source adapter 产生其可验证资产/负担/控制差分；Kernel 负责当前身份、类型、资源守恒及受保护写集；原 ExecutionTransaction 唯一发布。Agent/Viewer/API 只提出请求并消费结果。新命令名称为语义示意，现有 wire 未因此获得入口。

建立按稳定 Agent 寻址的受保护 `DispositionInventoryRootV1` 索引，记录 catalog/schema、原域 root/revision、条目数量/摘要及完整性证明，不建立第二份余额或债务账。原 writer 在产生、修改、解除该 Agent 的 asset/obligation/control 关系时同根维护自己的原索引；聚合 root 仅引用这些索引。最终 prepare 从同一 parent 验证全部 catalog 项、全部分页和 absence 证明。第一页、空本地列表、未知 writer 或一个不可解引用摘要均不证明无负担。没有完整源索引的旧数据须经各原域授权迁移重建；本入口不能扫描全 World 猜一个所有权清单。

## 5. 关键运行流程

<a id="ad-transfer-gates"></a>
### 5.1 五门槛、报价、接受与转让

| 门槛 | 同一 parent 必须验证的完整事实 | 失败后保留与下一步 |
| --- | --- | --- |
| 资格 | 新方的当前主体/原取得用途来源、取得类型、原合法控制/角色；`agent_acquisition` 当前有效来源而非旧全局分或客户端 tier | 缺证明与已证不足分开；重新取得原证据/资格，不改旧持有或抹历史 |
| 容量 | 原 primary/已 claim/制造在途的去重集合，并纳入同根已占用的转让结果；精确目标只计一次，所有并发 writer 使用同一原容量 guard | 等现有占用依法结束或明确缩成新条款；不能因待接受报价提前腾 seller 或占 buyer 的 slot |
| 资源 | 价格、新方取得与未来维护、旧 bond/欠费、全部业务和费用 source、受保护返还/程序额度逐单位足额；提交前可用量和 D/O/C 均成立 | 给具体 source/purpose 缺口；本次退款、incoming 售价、未来收入不能先融资本次 outgoing |
| 治理 | 当时原世界/地域/组织制度和各 scope 的有效授权、当前 restriction、原 claim/工业规则、正式 module activation 与 source authority | 缺源/过期/撤销/保护均阻相应新处分；不是加一张万能治理通过票 |
| 反滥用 | 同一 Agent/原取得谱系/owner generation 只有一个 winner；当前真实控制关系、原补助/首次资格计数、冷却、冻结、原 nonce/业务去重和完整历史 | 同键冲突拒绝；关联主体不伪装独立来源，同源转手不刷新 first/bond/冷却/责任；不额外规定一般关联交易禁令 |

整个请求在各门槛的 scope、数量、期限、风险和费用上必须被完整包含；任何维度不足时整笔拒绝，不取交集、不静默少交库存或换 payer。授权截止取所有适用来源中最早者，合法区间用原 canonical `[not_before, expires_at)`；客户端时间、重连、重新报价不延长旧责任。quote 固定读取 root 与具体有效边界，仅供明示承诺，提交时重读全部来源；漂移后不以金额仍低于 cap 为理由继续旧签名。

退休/报废没有新受让方，资格核当前原主体的真实处分权，容量核Begin增量0/Final依法释放原active占用及所有保留依赖；不能要求一个不存在的buyer或新取得slot，也不能以owner资格下降为由没收已有对象。其资源门槛仍含原欠费、实际处分/保管/drain与关闭费用，治理和反滥用照常适用，不收费一笔并未发生的新取得activation/bond。原世界epoch与tick映射来自同一适用policy；Begin须证明原冷却结束与全部接受/授权截止之间存在可合法终结的窗口，否则拒绝，不伪造必然能在过期前完成。

1. Quote/Propose/Accept/Withdraw 沿 CR001 非绑定元数据路径。即使所有接受齐备，也没有付款、费用、hold、容量占用、owner 变化或 grant；完整接受nonce只在实际Begin激活计划的根消费一次。Final引用原plan/已消费接受及其明确允许的后继，重验当前来源，不第二次消费同nonce或把旧接受改为新签名。
2. 当前有权必需方显式提交完整 `BeginDisposition`；先查原件/业务 winner，再核同版全部接受、当前五门槛、原库存/负担闭包及全部费用图。成功 Begin 固定权威请求 epoch `r` 和精确 lineage，从原 `r+release_cooldown_epochs` 起才可最终交割；当前政策常数为3，checked overflow拒绝。未达到时为明确 waiting，不把三epoch时间本身当授权。
3. Begin 只发布该 Agent 的停止新风险 fence、有限计划、明确接受的实际 Begin 运行费用及终结费用保护；不收售价/新 activation/bond、不退旧押金、不腾原容量、不更换经济 owner。最终价款/取得资金未被本文先行 escrow，Final 必须以当时真实可用量重新证明；不得把条款期望当资金已锁。Begin 的已确认保全/计算成本与尚未交割经济状态分别显示，不能把非绑定 offer/接受登记收费。
4. 冷却到达且仍在签署窗口内，显式 `FinalizeDisposition` 再核全部当前来源，prepare 原域差分、旧配置留存引用、控制 fence、完整 claim/资金/容量/责任写集。未结风险不能合法隔离、接收方缺范围、资金/资格漂移或任一 adapter 缺失时不交割；原 Begin 的合法已发生结果按其范围保留，不伪称转让成功。
5. 同一最终根提交转让、经济结算、唯一新 owner/source/generation 和历史引用。只有其原结果能使新方成为当前持有人；原 grant/目标不能随 receipt 自动恢复。原 release/reclaim 和处置按同一当前来源竞争，Begin 不暂停原欠费/强制规则，后提交者必须恢复/重验先提交结果。

<a id="ad-retire-scrap"></a>
### 5.2 既有 Agent 的退休与报废

退休、报废均消费玩法处置政策及真实财产/控制来源，不能调用制造 WIP 的 Dispose 冒充。与转让共用有限两阶段和精确 lineage 的原3epoch冷却：`BeginDisposition` 只在完整接受/当前权源/有界关闭预算成立时写本 Agent 的停止新风险 fence、原清单和终结计划，状态为 `retiring` 或 `scrapping`；既有 claim、容量、资产和未结义务仍在。它不宣告已退休/已报废，不为新 owner 腾 slot，也不停止必要有权历史读取和狭义结清。

原有进行中工作按原域 cancel/settle/return/succession 处理：可中止的未生效动作明确终止；已经 committed 的动作、消耗和付款不回滚；不可分 custody、未确认在途效果和原结果 unknown 先恢复。原 obligation 可以在不再依赖该 Agent 运行的原来源/责任主体处继续，不要求买方清偿、不强迫历史债权人重签未变化的权利。若其执行或返还仍依赖该 Agent，必须先以完整必需接受迁移真实残余或合法完成，不能用墓碑逃避履行。

`FinalizeDisposition` 在原期限内重新验证完整清单、当前权限和关闭预算，按政策一次结束 active 持有/运行，保存 estate/返还/债权和不可运行记录。retired 保留可审资产及责任；scrapped 另写机体不可复用 tombstone，只能按真实原回收 recipe 产生其明确输出，不凭机体目录自动铸材料、退款或补贴。两者均不删除 Agent ID、来源、旧配置、已付/未付和责任。无已采纳恢复政策不提供 retired/scrapped→active/重新 Claim；原 stable ID 不成为可重复领取的新对象。

每份计划在已签有限 canonical 窗口内只有一个终结 ordinal：Finalize 或 Abort/Expire 互斥。到期或源失效时只能解除该计划确可释放的处置专用预留并撤本件guard，保留原债权/hold/责任、已提交结果及独立成立的原风险约束；不能自动dispatch/重新上线或恢复失效grant。状态明确 `disposition_blocked/closed_without_finalization`，保留 blocker、原件和合法后续重新确认入口。终结费/机会独立保留，普通调用不能用尽；在 root unknown 时先恢复原决定，不能并发关闭。后续合法新计划引用旧谱系与未解除清单，不清零历史计数或重收已完成结算。

## 6. 接口与数据合同

<a id="ad-economic-inventory-continuity"></a>
### 6.1 逐项资产、资金和责任去向

每项清单固定原 domain/object/source/owner/purpose、数量或作用范围、原结果/hold/obligation、当前 revision、关联主权和合法处理方式。完整清单只允许 `transfer_explicit`、`retain_original`、`settle_original`、`return_original`、`succeed_explicit` 五类；未知不是零或自由资产。使用同一源多处的量先按原 identity 去重并聚合，不能既随物转让又当旧债担保。

| 清单类别 | 必须作出的具体处理 |
| --- | --- |
| Agent 身体、模块 slot、cargo、材料和 Data/Electricity | 逐项区分真实所有权、仅保管/使用权和已承诺量。自由且明确签署的部分可转让；受托、抵押、争议、在途或他人模块保持原 source/权利。身体不因换 owner 重建默认 frame、初赠模块、满电或冷却；旧来源、库存及损耗连续 |
| 原 claim bond/upkeep/罚款 | 引用原 claim 及资金 provenance；按玩法政策结清到明确 epoch，原 activation/已付 upkeep 不回溯退款。old bond 不直接变 buyer 可用余额；restricted 部分只能按原有效用途/原退款 sink 返回，原过期或撤销处理也沿原规则 |
| 新取得成本、价格与税费 | recipient 的原 slot quote 与双方明确同world liquid MainToken/OC价格分列，可明确0价赠与；本首版的新activation/bond/upkeep及价款只用其合法liquid，不消费restricted，不触发自动首次补足或新增/重领首聊赠款。已合法入普通liquid的量保持原可转让语义，不因历史首聊来源另造受限标签；不新增转让佣金。真实运行费/许可费各引用原schedule和payer，没有原费用来源的项不能凭本文收费 |
| 原 restricted/资助/托管量 | 记原 beneficiary/purpose/issuer/有效期和未使用量；不转为自由本金、不因转让/退休重领 first grant，不把原 pool 的回收退款改给 buyer |
| 已成立服务/合同、工业工作、物流、R&D、组织 estate/claims | 已付、已消耗、真实贡献、原原因和旧 D 保持。未付合法债权、未结 case、原 source-use、程序机会与关闭费仍属原责任；只有真正可分残余经完整接受和新当前 funding/mandate 才可继受 |
| 身份、委托、治理/工业准入、研发份额及私有配置 | 不是默认随实体出售的 inventory。原角色/许可/权利转移各有自己的消费者和必要接受；不在本合同复制一条全资产授权。私有 memory/key/account/provider session 永不默认加入交割 |

保留旧责任使用原领域受保护 hold/estate 和当前合法收款/返还路径，不新建影子余额。原 source adapter 必须使保留量对新 owner 的一般消费不可用，并让原窄 drain 仍可核原 source/受益人/上界。现有 ledger 不能安全区分该保留量时，须通过原合法路径先返还/结清可分量或补该原域的受保护 source 适配，未完成则整笔处置阻塞；不能仅在处置 sidecar 标“保留”而实际 writer 仍能花掉。

同一 source 的 D/O/C 沿原键记账；处置费从其准确 C purpose 转 D，旧合法解除才减少 O/C，退款和换 owner 不降低 D。旧方未解除责任继续挂原 key；新方明确继受的未来 O/C 在同一 root 建立新真实来源、缩旧可解除范围。不得将已确认未付、unknown 或损害责任打包成 buyer 的默认 debt。原[服务继受](continuing-service-fulfillment-and-remedies.design.md#service-succession)与 R&D/组织的独立顺序仍控制其实际操作。

**原政策的可执行结算顺序。** 最终根固定当前epoch `c`，按原claim和实际支付事件核旧`paid_through`，算截至c真实欠费；本次政策允许以现有bond抵扣，沿原restricted-first/liquid算法，剩余不足由旧责任人的合法源补足，否则整笔blocked。原已成立处罚只来自其真实决定；主动处置不另加forced 2000bps。核对结清后的水位`e>=c`，保留已付期并不退款；新方原slot rate只预付`e+1`一次，新claim付至`e+1`、下次`e+2`。旧bond余量按真实原refund_sink拆回restricted/liquid或准确原池；新方在所有incoming/退款之前必须足付价款+新activation+bond+未覆盖upkeep。每个整数checked且每项sink进入候选，不能用当前epoch覆盖丢失的原期间。

这是本次处置policy的明确新映射：当前旧Release先由upkeep路径结清后才退全剩余bond，并不已经实现“转让用bond抵欠”。若原upkeep在Final前已把水位推进，必须重读原claim/event并按已签typed公式重算真实未覆盖期和bond余量；不能同时扣旧upkeep事件及同一期间的处置欠费。自然推进不重开冷却，超出原已签公式/金额上界、改变来源或费用图则重新形成准确修订/接受；旧较宽cap不替新同意，旧精确scope的等待不得用于换新受益人/对象。

独立原账截面沿玩法实例：旧bondR150/L50、欠25、旧可用R10/L80、buyer L800、价60、新slot2费用150/300/38。扣旧bond的R25后原退款R125/L50；buyer `800-60-150-300-38=252`、新bond300；seller L190/R135；原欠费sink25、新维护38、burn75、treasury75，合计`252+300+190+135+25+38+75+75=1090`，等于原`800+80+10+150+50`。此时c=e=12，新方付至13、下次14；过期restricted退款改回原池125、seller R仍10，其余账不变。实际Begin/Final模块Data/E/cap和其它既有责任独立计算，不能挪OC数代签。

<a id="ad-effective-owner-generation"></a>
### 6.2 唯一生效输出与当前控制

本 family 的输出是原 ExecutionTransaction receipt 内的 typed disposition payload/索引，不是另一份成功 token。它绑定原 request/business identity 和完整 digest、world/branch/parent/base generation、Agent stable ID、类型/范围、old/new economic principal 及 historical source actor、原来源/新 source revision、适用 policy/activation、旧新 owner binding/identity generation、全部 control/task/executor/grant fences、资产/资金/容量/usage 差分、原 claim/价格/费用、保留/继受 obligation、不可变配置/历史索引及当前可见性、准确生效位置和前后 commitment。finality/containing block 是外层证明，不能反过来进入自引用摘要。

`generation_next = checked_add(generation_current, 1)`；source revision、acquisition sequence、identity generation、task revision、executor epoch、Prompt version 各按自己的合同推进，不要求数值相同。溢出、前态缺失、重复来源或任何受影响 fence 无法确定时，生效前拒绝。制造 Agent 不经当前禁止的直接 identity install 绕过；其 acquisition/control source、派生 generation 与 root checkpoint 必须作为同一目标适配一起更新，原制造订单/交付原件不改。

primary 的首次注册/账户/starter receipt 是不可变原件。若被处置的 Agent 曾作为其他 Agent、资金或组织来源的 root actor，必须取完整依赖索引：新 owner 仅取得已签对象；保留对象的原稳定经济主体/owner binding、原取得 source 与责任仍独立可证，必要技术 fence 同根推进。不能以 `current_owner(primary)` 的新值反推旧子 Agent/钱包也被出售，不能用历史 claimer Agent ID 冒充新的自然主体。原账户认证来源仍可证明其未转让的权利；每个相关 resolver/writer 必须接收准确的新版 source tuple，不能回落到旧 root 名称。缺这种保留来源适配或完整证明时本次 primary 交割不可生效；不是凭空授买方关联资产，也不是自动给卖方新 primary。

Begin和Final的真实效果严格分开：Begin在canonical根提交本件`disposition-plan admission guard`，固定Agent/scope、允许的原窄drain、plan lineage、当前版本/有效窗和原receipt；原经济owner、identity generation、task_revision、executor_epoch、context与Prompt V保持。它是所有真实writer在effect-time必须读取的持久当前来源，不是客户端标记或仅停provider进程；其scope内的新风险拒绝/等待，已成立且获权的原窄drain及原件查询继续。原来源许可明确允许已签计划的固定准备/终结动作，普通生产grant不能越过guard。Final转让才在其完整根改变经济owner/source/identity及task/executor fence、中性配置；退休/报废则完成inactive终态，无新owner上下文。Abort/Expire只移除此件guard，不自动派发旧pending或隐式Resume；仍有效context是否可显式恢复沿原规则，已另失效者须新当前上下文，不能复活旧授权。Begin的实际已提交费用/保全结果不被后续未交割改成没发生。

**终结调用者与技术准入。** immutable计划明确拥有原窄关闭权的稳定closing principal、其实际caller Agent/current identity（组织仍需原真实业务来源），以及原issuer合法签发的独立、有限`close-only`技术grant。Begin前必须已有该当前有效grant、真实invocation context与其获权预算账户，且覆盖明确签定的关闭窗口；不能由Begin、业务hold或过期普通代表grant自动铸出。技术scope精确指本family的单一`CloseAgentDisposition`命令（typed payload只取Abort/Expire），或分别准确的命令grant；原selector/签名请求与业务plan共同核本Agent/plan lineage、current guard CAS、原source/refund sinks、有限费用/次数/期限。Grant的单一object_name或body hash本身不替代这些业务检查。该窄权不能执行Finalize、更换收款人、转让或创造新风险。

原 [trusted admission](../../../crates/oasis7_runtime/src/world/capability_authorization_command.rs)在guest前和commit前仍核当前subject/context、issuer、expiry/revocation与manifest。普通执行grant失效而独立close grant仍有效、原plan业务关闭权和全部窄条件成立时，当前caller才可一次Abort/Expire；close grant也失效/被撤、identity漂移或source不可核时返回准确authority_unavailable/recovery_required，保留guard/hold，先按原issuer authority恢复或取得当前合法fresh close-only授权再核原plan。新技术grant不改变原plan/terminal ordinal/World关闭预算或已花历史；它对应的真实CapabilityBudgetAccount须另有合法来源，旧账户及spent保留，不能通过换grant洗原责任或免费重试。没有当前技术来源就不声称自动已关，终结预算不充当调用授权；新授权若需独立原动作，其真实费用/责任先按原来源记录，不藏进本计划65次图。

同一根还包括 IA 的[控制与历史消费者](indirect-control-agency-execution-and-continuation.design.md#agent-disposition-control-history-consumer)：旧 grant/任务/executor/认知许可不能再批准其已失效范围的新效果；未生效 pending 标记需当前重验，已 committed 结果保留原 owner/费用/cause。旧已用 Prompt/goal/memory-context 的不可变 provenance 先受控耐久准备，根候选固定完整引用；旧本地 runtime-instance ACK、缺失 sidecar 或新空 profile 均不能代证明。Final当前Prompt按原版本checked递增追加中性profile（原私有overrides清空，原因是处置authority），不覆盖旧版本或伪称新owner配置；新方后续用当前binding/expected_version显式配置。

原私有正文、盐、key 不复制到公开 World/CAS；历史引用可被当前有权者解引用。目标执行上下文默认不继承旧私有策略/激活目标/memory/session。reader/sidecar 落后时只暂停该 Agent 的依赖新写入并追平原控制结果，不能重新交割；G1 提交未知另按 §7 隔离整个旧 World。

<a id="ad-all-writers"></a>
### 6.3 必须接收的 writer 闭包

| catalog | 原 writer / 真实来源 | 必须接收的同根规则 |
| --- | --- | --- |
| W01 | participant 注册、starter claim、主绑定、current owner/identity | 原注册/补助计数不重建；经济来源与账户来源分离，新源只改签署对象；所有 root 依赖纳入完整性和技术 fence |
| W02 | ClaimAgent、upkeep、ReleaseRequested/Released/Reclaimed | 与转让比较同一 claim/source/epoch/version；先提交者胜，后者重新取原件/重验；旧计费或回收请求不能扣新方、删新 claim |
| W03 | 制造 Confirm/Advance/Cancel/Dispose/Deliver | 共用原容量；只能处理原订单/beneficiary，已有交付对象不能再次注册；WIP disposal 不消费本处置终态 |
| W04 | owner delegation、signer delegation、task/goal/executor/认知许可 | 原 owner/source/generation 失效立刻阻新动作；旧 pending 按 IA011，原 task retirement 不被显示为 Agent 退休 |
| W05 | Prompt/目标/记忆修订、sidecar、provider turn、SDK/harness outbox | 配置 provenance 与新中性版本同根引用；旧私有 context 不投递新 owner；旧原请求只能查询/精确继续其原合法结果 |
| W06 | native Action/step/tick、Move/BodyAction、body/core/power/memory modules | current lifecycle 和授权作用范围在 effect 前检查；旧补能水位、battery/heat/cooldown/slot/cargo、原消耗不重置；退休后只允许原政策明确的保全/结清，不能后台继续生产 |
| W07 | MainToken/资源/材料转移、restricted grant/refund、Data access | 完整按 source/purpose/实际 owner 验证；retained hold 不可新方自由花费，原债权合法入账/返还仍可走窄源；新私有读权不从旧 data-access 条目自动继承 |
| W08 | slot/cargo、module item/artifact marketplace、install/upgrade/uninstall | 硬件/使用许可/代码 artifact 产权和私有状态分别清单化；不制造默认库存/新许可；未转移 artifact 的 payer/产权仍归原 source |
| W09 | CF、持续服务、R&D、组织 estate/claim/continuity | 原义务、贡献和案件保留；原 narrow drain/明确继受，不能因 stable Agent ID 相同自动换责任主体或重设 D/O/C |
| W10 | 工业生产/建造/维修、物流/custody、供能/保供/levy | 真正在途按原 slot/work/receipt 保留；变更 carrier/control 需要原完整交接；原 license/地域资格不默认随卖 |
| W11 | actual module dispatcher、artifact-owner 计费、CapabilityBudgetAccount | 计费 source 与 technical owner 不混同；每个原调用/费用 ordinal 唯一，旧 cap 不通过新 generation 重生；未承接付费来源不能用 ownerless 路径免费执行 |
| W12 | journal/receipt/outbox、save/restore/migration/replay、当前读面 | 处置全写集与 base generation 同代；重放原 delta，不新调用业务/模型/外部 effect；当前认证后读原件，不以 snapshot 或本地 ACK 推断未提交 |

这是原已采纳族的完整接收目录，不是只做 W01/W02 即宣称覆盖。每个准备候选携带实际 catalog/原 writer schema 版本，新增相关 writer 必须先声明原域索引与上述 invariant 的接收；未知类别拒绝本次处分。每条实际业务 adapter 及嵌套 invoke 进入 §9 图，纯通用签名/守恒核验可按真实无 invoke 路径计零，不能把制度执行藏进 host。旧入口和内部 timer 同样受此 guard，不能靠 UI 隐藏按钮维持安全。

<a id="ad-read-model"></a>
### 6.4 三端同源状态、历史与下一步

读面分别显示非绑定 draft/accepted_pending_validation、五门槛逐项当前状态、准备/待恢复、真实 transfer effective、retiring/scrapping、最终 retired/scrapped、明确未生效和 outcome_unknown；这些是原 domain outcome 的投影，不修改 Runtime 原 disposition enum。每条携带准确来源/时间/版本、当前 blocker、原请求查询键、实际 next_action/next_recheck。未支持的写或私有历史接口明确 unavailable，不显示假成功动作。

当前认证的旧当事方按原历史权利读自己的配置、决定和责任；新方读当前权利、原来源和必要公开负担，不因此取得旧私有正文/密钥。每页绑定同一 root、受众与权限版本，交付或解密前重验；知道 Agent/receipt ID、旧签名或旧 cursor 不授新读写权。公开链字段仍走原公开读取，不能反过来推私有 World 权。历史正当读取与当前控制分开，退休不导致所有历史不可读。

Viewer、native Launcher/Game Client/Harness 与 pure API 沿 IA consumer 和[正式入口映射](../../world-simulator/llm/external-agent-game-api.design.md#api-primary-entry-applicability)消费同一原件。query-only 不发新业务、不产生新的处置/业务费用、不消费 business nonce 或替原请求更新 owner；原合法读取的服务配额/计量、认证nonce/有限元数据及独立已发生费用继续，不称全状态绝对不动或读/网络完全免费。入口丢 ACK→查原件；权限丧失→当前有权历史/重新授权；报价过期→新报价/新接受；原件缺失→保留 unknown 并指向可用的原恢复路径，不用换请求试探是否已生效。

## 7. 状态、事务与持久化

<a id="ad-transaction-recovery"></a>
### 7.1 原请求、一个业务赢家与提交候选

request key = `(world, branch, family instance, requester principal, request_id)`；digest 绑定完整 command/terms/证明和 expected revisions。business winner key = `(world, branch, Agent stable ID, prior economic source generation, disposition lineage)`；credential/adapter/artifact/request revision 不形成第二份业务资格。Begin原子CAS同时比较当前economic source generation与`plan occupancy/guard revision`，只有`empty→本lineage`可成功，一个Agent至多一个active plan；Begin不推进economic generation，故不能只比该generation放行第二个lineage。Final/Abort还须匹配同一plan、当前guard revision和active阶段，对唯一terminal ordinal原子选胜；已关闭plan不可再用。Begin消费完整接受nonce一次，Final引用原激活决定且只消费自己的阶段请求/终结winner，不重复消费同接受。相同请求同digest返回原结果，同键异digest冲突；另一请求指同已生效source时返回原winner/过期，不执行第二次。

在 [ExecutionTransaction](../design.md#62-unified-executiontransaction-architecture-p0) 内一次准备 parent/time、所有权及取得序号、原/new claim/bond/upkeep、逐源余额/hold/treasury/价格、容量、D/O/C、完整原域候选/责任引用、控制 fence/中性配置引用、module state、原 nonce/业务赢家、费用及独立 capability reserve/settle、event/journal/sequence、receipt/outbox/commitment。当前 authority/source 与读集 generation 在 final install 前再次相等；不能先单独安装 identity、退款、注销任务或把 sidecar 改名，再补财务事务。

在 canonical 排序点由原 DWE activation/history 选确定版本，不能凭请求发送时间、客户端 artifact 或本地已安装版本选版。activation 的正式包含位置不晚于生效位置，迟取件不追溯更改已确认历史；pending 请求在新版本下按原 scope 重新核验，已固定原决定只恢复其原版本/结果。所有引用需要同 root/同代可验证，缺原 artifact/authority/finality 不能把今天的默认值补进旧 receipt。

### 7.2 失败、未知、恢复与回放

| 原权威与持久结果 | 允许的处理 | 禁止推断 |
| --- | --- | --- |
| 确认 `NotCommitted` | 仅丢弃本阶段全部staged业务/费用/identity/配置/nonce，保留该阶段parent；Final失败保留已committed Begin、其费用/guard及此前原义务。能形成稳定拒绝时仅发布同一无新增业务效果决定，重试返回它 | 不能先扣款再补偿，不能留下新owner/已退休标记，也不能退回整个计划Begin之前的旧World |
| 确认 `Committed` | 安装或恢复原完整候选；回复、镜像、读面或 GC 失败只标维护降级，幂等追平 | 不恢复旧 World 写能力、不重付、不把原成功改 rejected |
| `StatusUnknown` | 保留原 operation、base generation、完整 candidate 及证据；隔离整个旧 World/writer，停止后续 mutator、tick、save、依赖派发；按同一身份查精确决定后恢复匹配内存 | 本地旧 snapshot 健康、索引暂缺、本地未持久或“没看到结果”都不证明 canonical 未提交 |
| 请求超时/查询 unknown，未给 G1 结论 | 保留原请求按其协议查询；未知原效果/负担不得被新转让、继受或关闭越过 | 网络超时不自动等于 G1 StatusUnknown，也不提供新执行权 |

完整持久闭包包括原条款/render/接受及 source proof、所有权/原注册/制造/claim 来源、全部控制与任务/配置历史引用、资金/hold/义务/usage、费用图及原计数、请求和业务 winner、版本/activation、receipt/outbox/journal 与依赖 root。按 [G1](../prd.md#runtime-generation-persistence-contract) 和原恢复接受门处理完整 generation 与非空 journal suffix；ordinary Committed 只陈述持久结果，正式服务仍须对应 authority/finality/profile 和恢复校验。

canonical replay 可按原版本事件/delta 重建原状态和 outbox ledger；不再发新的实时业务请求，不重跑 sandbox/LLM/provider/dispatcher，不新算费用或创造第二份交割。原 external outbox 如有实际副作用，仍按其持久 identity 和 dispatch/ack 合同恢复，不能以重放 snapshot 再发送。旧数据读得出与当前可以继续写是两个结论。

## 8. 部署、安全与运行约束

启用先验证完整 producer、各源索引/writer、当前身份/签名、真实 module/manifest/activation、可支付费用 source、完整 root commit/G1 和三端读面；缺项仅阻相应目标入口，不以旧 release+claim 或本地 direct helper 降级。权限 proof 可先有界读取，但 effect-time 必须验证当前 revocation、source revision、实际 branch/instance、全部原资产权及读集；来源未接收不能由客户端自报 owner。

private history 先按原受控存储 prepare 后仅提交不泄密引用，orphan prepared 内容按原保留规则处理；不能为通过根校验把 key/Prompt/memory 上传公开 CAS。source current-generation 或当前 serving 无法核时停止新依赖写入，保留准确的有权原件查询；历史受众不足不得把结果正文公开给新 owner 解决恢复。

## 9. 质量、容量与完整费用

<a id="ad-bounds"></a>
### 9.1 有限工程 profile 与完整编码

`agent-disposition-v1` 是本次可评审的工程形状，不发布生产政治/经济参数。每根最多一个被处分 Agent；必需 principal≤32，每组织批准证明≤16、合计≤512；资产/责任/控制/source 索引条目总数≤256，签名验证≤512、独立原索引读取≤256。原件读取≤128KiB，按同一 root 最多16页、每页≤16条；超过需先沿原合法域处理/归档为仍可完整验证的原件，或采用有明确完整预算的后继适配，不能裁掉本次必要项。上述计数和字节上限取交集，不承诺512份签名、256个长记录同时装满仍可提交。

每个实际 invoke 的完整 canonical I/O≤256KiB、effects≤32、emits≤16；memory/fuel/call-rate 使用该 manifest 的真实正数有限上限，并取 world/executor/公共服务更严限制。原公开请求额度不被扩大；formal transport 无法承载时显示该 profile 不可达，不截签名/改用未认证路由。proof 可以完整嵌入，或引用当前 canonical 已验证原件；后者仍须取回并完整核当前 proof/root/权限，未验证 hash 不能充当授权。凡因此引入的 billable source adapter 都进入下节真实调用图。

本 producer 的内部 typed payload `F≤32KiB`，raw state/new_state各≤32KiB；内部固定 hash/signature 用明确 byte string codec，host [ModuleCommandEnvelope/ModuleCallInput/Output](../../../crates/oasis7_wasm_abi/src/lib.rs) 的 `Vec<u8>` 整数数组保持原 ABI。command 路径 `event=None`，payload先编码为command，再进入input.action，两层不能省。完整 command 非 payload 编码量≤1KiB，完整 input.ctx 及 map/keys/heads/options/其余封套≤16KiB。最坏 command≤`2×32768+5+1024=66565` bytes；input≤`2×66565+5 + 2×32768+5 + 16384=215060` bytes（210KiB+20），小于256KiB。有event或其它host字段时须重新计完整输入，不沿用此上界。

输出的 encoded effects 合计≤64KiB、encoded emits≤16KiB、所有其它字段/容器头/计数字段≤16KiB；含new_state最坏 `2×32768+5+65536+16384+16384=163845` bytes（160KiB+5）。`output_bytes` 是计数字段，不是另一份未计量 payload；实际序列化及原校验决定收费量，不信自报大小。超过任何子上限或完整 I/O 上限时在发布前整笔拒绝，禁止把 Vec 强称 byte string、丢旧 state 或分页多次交割凑成功。历史内容不塞入热 state；完整历史引用 root 与必要当前记录都在上述预算内，其原对象有受保护存储和当前读取证明。

上述128KiB原件读取是本次来源/接受/索引证明，不要求每次加载全部私有历史正文。本次依赖的新增私有配置准备最多8个不可变对象，单个完整存储编码≤256KiB、合计≤2MiB；当前小型投影返回≤16KiB并含所有框架字段，历史页沿同root最多16条且完整响应≤32KiB。hash/加密/耐久准备的真实对象数、bytes、source配额/费用必须随完整准备图给出；实际有billable invoke时占用下述63个adapter节点，不能因根只存引用就漏掉调用或费用。较大的已存历史通过原受保护索引/root复用；尚未耐久的旧history可先由有权原存储入口有界归档后取得完整root，再提交本计划，不能剪去旧版本。缺原授权、原件或原合法存储预算时block，不触发模型生成替代内容，也不造免费全历史扫描。

### 9.2 完整调用图、不同预算与最后关闭

每条款冻结实际 invocation DAG：每节点标 command/operation ordinal、artifact/instance/version、payload/完整I/O/effects/emits/fuel、实际 payer、原 source/schedule、接受 cap、预算类型和 purpose。相同 module 的两次 invoke 是两节点；原 native 账户/身份/守恒 prepare 无 invoke 时按真实路径记0，制度条款解释不能放进 host 免费执行。transfer/retire/scrap均为Begin最多1个producer、Final终结根最多1个producer+63个原source/领域invoke，最终根≤64、全生命周期新增≤65节点。Finalize与Abort/Expire为同一受保护终结ordinal的互斥选择，不两次收费。旧服务、物流、R&D等真实drain调用保留原有限图/预算，附在完整清单且去重合并，不因不属于新65节点便记零。

Begin这一有限形状只消费已存在的canonical产权/组织附件/mandate/proof与原生通用签名、当前权限和守恒检查；本计划的业务条件由该一次producer判定。它不顺便创建组织授权或调用另一制度producer取得同意。须先新增原业务授权的候选先沿其原有权入口独立完成、保留真实费用/责任，再形成准确本计划；没有前置即blocked。若要求Begin自身包含额外source invoke，本profile超形状拒绝，不能漏算、挪入未来Final或改名host校验。Final的63节点包含其所有真实私有准备/原域adapter；无invoke的受控存储和proof读取仍受前节实际bytes/原服务配额约束。

按实际 [World 计量](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)，每节点 `Data=ceil(I/1024)+ceil(O/1024)+2e+m`，`Electricity=1+e+m+s`，`s=1`仅有new_state。预检的O/e/m取manifest上界、I取真实完整输入。以256KiB I/O、32effects、16emits的保守报价为592Data/50Electricity；它不是强制实际preflight或实际收费量。真实 [artifact owner payer](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs) 与其合法资金 source 须存在且明确同意，不能利用现有无owner免计费分支替生产资金。

trusted command 另按 [CapabilityBudgetAccount](../../../crates/oasis7_runtime/src/world/capability_authorization_state.rs) reserve/settle：`C_reserve=ceil(F/1024)+ceil(maxO/1024)+2maxe+maxm`，actual把输出/e/m替换真实量。它与World Data/Electricity各自必须成立，不能当支付货币或双扣同一经济余额；native/旧Pure Domain不经过trusted调用的节点没有此账，不任意外推。producer F≤32KiB使cap上界368；其它实际trusted节点另限F≤64KiB且完整I/O仍合取256KiB，cap上界400。

| 新处置图 | Data保守上界 | Electricity保守上界 | 若全部节点实际trusted的cap上界 |
| --- | ---: | ---: | ---: |
| 最终交割/终结根：1 producer+63 adapter | 37888 | 3200 | 368+63×400=25568 |
| 完整两阶段transfer/retire/scrap：2 producer+63 adapter | 38480 | 3250 | 2×368+63×400=25936 |

逐实际 payer/source/purpose 求和并在提交前足额保留；原必要交易/许可费另按真实来源加入，不能把售价/claim资金当Data/cap补贴。原资源依赖相同时只hold真实资产一次，多个必须同时成立的授权各核完整暴露。Begin必须将最后终结费和机会独立保护，普通操作/原件查询不能花掉，源撤销不会释放未结义务或复活广泛执行权。狭义终结仅消费已成立计划明确允许的固定来源/上限；新风险或改收款人须新当前授权。

独立轻形算例：真实候选若为F=2048bytes、完整I=8192bytes、完整O=2048bytes、1effect、1emit、有new_state，则实际13Data/4Electricity，trusted actual=7。宽manifest（256KiB/32/16）下World预检344Data/50Electricity、cap reserve338；实际完成释放未使用预留，不把344或338当实际账单。F不是I，两个账不能相互替代。嵌套原adapter若存在必须逐节点另算；这些固定量仅供独立算术，完整合法签署/身份/源接线与真实编码仍需方法检验。

每认证主体普通协议在途≤4，另留1控制/原件槽；每60秒服务端单调窗口普通请求≤128、原件查询≤16，同一原请求合并；各原服务更严防滥用限制继续。每主体未关闭proposal≤16、每Agent至多1已生效未终结处置计划；quote/accept期限为原policy明确的有限canonical窗口。重复已决定请求不重跑guest或收费；确认未提交的业务候选费用一并回滚，真实机器成本仍受协议准入约束，不等于无限免费算力。查询/撤回不占业务nonce或最后关闭预算，真正结清沿已保护图；unknown始终先核原决定。

## 10. 兼容、迁移与回滚

新family/schema/activation使用新签名域，旧claim/release、制造WIP及已有协议签名不授权新处分。旧source缺稳定主体、控制依赖或资金provenance时保留原事实和不可处置原因，按原域有权完整迁移；不能从今天的owner倒填过去。primary/account与制造derivedidentity要同时完成consumer适配才可启用相应来源；某一owner helper改好不算整个闭包完成。

升级须保留当前source版本、历史阅读、稳定业务去重及原责任drain；不得移除新版字段后回旧代码继续写。回滚候选若不能读全当前generation和新保护量即只读隔离，不能重新开放claim或释放旧hold。退休/报废的tombstone、取得序号和原义务依赖不按普通metadata TTL删除；存储治理可归档不可变正文，但当前索引和合法历史/去重/责任所需原件必须可恢复。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游要求 | 本设计条款 | 独立验证目标 | 验证方法与场景 | 证据 | 当前边界 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | [来源与形成](#ad-authority-formation) | 实际经济权/组织mandate/完整同版接受，注册或任意治理不代替来源 | [AD01](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-authority)：个人、组织、第三方source及缺签/来源错配 | 原source/proof/terms/render/acceptance及零效果差分 | 本轮只设计，入口/签名和组织接线未由本文证明 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | [五门槛](#ad-transfer-gates) | 正向完整交割与五门槛逐项失败/过期/pending、并发只一个winner | [AD02](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-transfer)：固定真实前态及资格/容量/资源/治理/反滥用反例 | 每gate当前源、报价/拒绝/原件、完整余额与容量差分 | 单个claim/release测试不能替代 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | [经济清单](#ad-economic-inventory-continuity) | 价格/押金/资助、原库存和旧债逐源守恒，有限继受不洗D/O/C | [AD03](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-obligations)：逐源retain/settle/return/succeed，unknown与不可分原件负例 | 原ledger/hold/obligation/source与前后量 | 合同形成/控制变化不是履约 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | [owner生效](#ad-effective-owner-generation) | 经济与当前控制同根；primary依赖、制造派生generation、pending和旧结果分开 | [AD04](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-owner)：相邻顺序、旧grant/task/identity冲突与root依赖 | 原receipt、source/generation与全部fence | 不以单identity安装代签 |
| [REQ-AGENT-AUTH-002](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002)；Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | [退休报废](#ad-retire-scrap) | Begin不腾槽；终结保存原资产/责任，晚到结果/旧命令不复活 | [AD06](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-retirement)：退休/报废/未知/到期/终结费用保护 | 原计划、源清单、资金/容量/墓碑和历史 | WIP报废或task retirement非此证据 |
| [REQ-AGENT-AUTH-002](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002)；Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | [owner生效](#ad-effective-owner-generation) | 原V/R与中性版本和新V2因果不混，旧私有内容不默认转让 | [AD05](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-history)：配置原件、sidecar缺失、角色/读权变化 | 不可变引用/实际受众/原结果与当前配置 | 当前instance ACK不证明耐久历史 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | [writer闭包](#ad-all-writers) | W01–12逐项接当前源、原负担与关闭guard，漏writer拒绝 | [AD03](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-obligations)：完整索引/absence、旧timer/native/module与并发 | 真实catalog/每源root、读写集合和拒绝差分 | 不以support标签或单入口覆盖未知族 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | [事务恢复](#ad-transaction-recovery) | 原请求一次效果、G1三态及正式authority边界 | [AD07](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-recovery)：precommit/丢ACK/unknown/混代/非空tail | 完整candidate/basegen/原决定/所有余额与控制差分 | 静态审读不代业务故障注入 |
| [REQ-AGENT-AUTH-002](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002)；Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | [事务恢复](#ad-transaction-recovery) | 原历史/配置/结果和责任恢复后仍可受权核对 | [AD07](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-recovery)：原版本replay、私有引用丢失与投影追平 | 原件、同代索引/历史受众与无第二效果 | 健康旧snapshot不证明新候选未提交 |
| [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)；Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | [容量计费](#ad-bounds) | 完整双层CBOR、真实图、World与cap分账、退出预算独立复算 | [AD08](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-bounds)：轻形/最大图/原source费用、单字节超限及actor不付费反例 | 完整编码、manifest/真实payer、逐调用预算与actual | 上界不当actual；未实跑不冒充性能通过 |
| [REQ-AGENT-AUTH-002](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002)；Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | [读模型](#ad-read-model) | 三端新控制/旧历史/当前受众/unknown同源，query-only零业务效果 | [AD05](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-history)：旧方/新方/旁观者及实际入口不可用 | 同root原件、身份/读权与可操作下一步 | 静态页面/SDK/mock不代真实三端验收 |

### 11.2 接收层级

独立文档审读验证来源、五门槛、完整写集、容量/算术与方法相符；未来同候选业务方法验证实际producer、每个writer、并发与故障恢复；实际玩家/平台/正式服务另取其原要求证据。三层分开报告。本文未运行Rust/WASM、Runtime、游戏、模型、Provider或真人验收，不以方法存在提升实现状态。

## 12. 决策、风险与剩余实施

本稿选择一个Agent/一次完整交割、当前来源完整包含、显式两阶段停运、完整原清单/责任和独立关闭预算；复用原形成/授权/费用/事务，不新增万能组织权或另一套财务真值。工程上限为可执行有限profile，其计数和完整编码必须同时满足。玩法价格、押金来源、upkeep切点、冷却及退休/报废的经济取舍由原玩法处置政策拥有；本稿只消费准确政策版本。

剩余是实现并验证新的typed family/event/source resolver、完整原域保护索引与所有writer接收、正式source/grant/manifest、一次经济与控制提交、配置受控耐久引用、G1恢复和三端reader；现有release/claim/identity或单个staged路径都不能代签。历史source缺失、unsupported adapter或超出有界profile须准确阻塞并给原恢复/合法原域处理路径，不能把它们解释成用户无历史/无责任，也不能把本目标设计说成现有运行能力。
