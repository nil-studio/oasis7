# Agent 中介的一般合同形成政策

- 状态：target-contract；本政策是完整目标玩法合同，不表示当前实现、治理激活或真实验收完成。
- Owner role：gameplay_designer；签署、事务、费用、恢复由 Runtime 主责，正式呈现由 Agent / World Service / Viewer 消费，独立方法由 testing 维护。
- 核对日期与基线：2026-11-05；输入基线 ebd9b8431f2dc084c4ef6be3bcea9ca054ab2a81。仅静态设计与直接源码审读，未运行世界、模型、业务 fixture、跨节点或真人验证。
- 产品声明：[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001)；验收：[AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001)。Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。
- 系统设计：[一般形成合同](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-terms)；独立方法：[CF09–CF17](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-09-organization)。

## 设计适用性与生命周期闭合

- 生命周期：`active`（目标政策）。
- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 理由：本分册唯一维护签约主体、接受、授权暴露及退出的玩法选择，不另设呈现结构；技术正文、精确接口、事务/费用/恢复及验证由原 [一般形成系统设计](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-terms)承接。这一产品体验设计豁免不免除专业系统设计，也不表示实现已完成。

## 1. 范围与玩家选择

本政策承接个人和组织、直接签署和受托 Agent、多方及跨时间合同的共同形成条件。它允许玩家把实际已授权的重复签约交给 Agent；每一份实际形成的合同仍须有完整可读条款、全部必需方对该版条款的明确接受，以及权威世界的一次生效结果。聊天、草稿、建议、意向、报价和待校验接受不产生资产、债务、资格或优先级。

适用履约族为即时原子交换、有限持续供能、维护/维修、物料运输、一般共同工作，以及受原 R&D charter 约束的合作研究。形成政策统一解释谁可以承诺、承诺多少、何时成立；各领域继续拥有实际工作、交付、责任、救济和结果。原 owner_atomic_resource_exchange_v1 及个人供能 v1 保留原范围和历史，不被本政策静默改版。

玩家可比较修改完整条款、缩小尚未签署的范围、补足合法授权或资金、选择其他伙伴、自己执行、等待或不成交。无合同替代须是真实可用路径；不得为促成签约虚构当前需求、费用、恢复手段或收益。新合同的版本选择由明确协商决定，不能将旧签名转换成新版同意。

<a id="formation-policy-principals"></a>
## 2. 组织、主体与普通签约来源

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

合同主体 principal 是可追溯的个人或原已登记组织。代表、执行 Agent、设施、付款账户和受益人分别说明；同一人通过多个 Agent、密钥或岗位签署，不增加主体数、组织内部票数或授权额度。组织内部合法多数只能表达该组织的一份接受，不能替其他合同方接受，也不能擅自处分成员个人财产。

**普通业务签约附件。** 组织使用原稳定组织身份、当前 charter 及治理来源登记明确的 contract-signing 条款附件。首次登记同时需要：组织存在且未处于禁止新增承诺的状态；当前 founder 的可验证确认；完整当前成员名册中去重后的个人控制主体全体明确同意附件；原治理对准确组织、charter hash、附件正文、适用范围、批准人及生效边界的决定。当前首版初始化至多 64 个成员控制主体；超限属于有界结构拒绝，不截取一部分成员代签。

附件选择 1 至 16 个稳定个人批准主体、1 至人数的明确签署门槛、可委派的业务动作/对象/资产来源、累计资源及责任限额、代表任期、更新及撤销规则。批准人的合法凭据轮换须证明同一主体连续性，不能因 Agent 换 owner 自动换人。附件的后续修订及批准人变更需要上一有效附件的全体批准主体明确同意和准确治理决定；增大任何现有资金来源的可用权还须该来源有权者明确同意。某批准人不可达不产生替代授权；组织仍可处理原义务或按原连续性政策进入保护/处置，不能由缺席推导新签约权。

已存在组织的附件不追溯改写旧 charter 或旧义务。组织/charter、批准人来源或组织风险冻结发生变化后，旧附件暂停新增签约，按当前有效来源重新核定；旧合同与资产负担保留。后续成员加入不自动成为附件批准人，也不继承个人 credential。只把已核准的业务附件作为当前组织授权来源，不将名称、成员关系、通过的任意治理提案、连续性接替 grant 或普通角色称号当签约权。

组织对其明确接受的义务和利益归因负责，代表保留当时授权及任期记录；这不是把 Agent 变成责任承受人。组织使用个人 owner 的 Agent、材料账、费用账户或其他主体资金时，实际有权者须另有足够的已接受来源授权，并按本次是否新增扣付/权限列入必需方。组织标签不创建余额、转移成员财产权或免除原 owner 责任。已有合法使用 grant 已足够时不再虚构一份新权利。

<a id="formation-policy-acceptance"></a>
## 3. 完整条款与全体必需方接受

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

必需方集合由实际义务和效果推导，至少包括每个新增义务承担者、实际新增付款/费用承担者、需要本次额外同意的权利人，以及原领域继受/修订规则要求的受影响方。被动接收且已有原合法权限的 treasury 或既有许可权利人不因被引用而被强造为签署方；新增付款、期限、用途或权利必须重新按来源判断。全部来源和需求须完整，不按客户端提供的签署名单猜测。

一般形成至多 32 个必需 principal；每个组织按其附件门槛产生一份组织接受 bundle。领域可以更严，例如服务初始人数和 R&D 工作名册；服务继受可容纳原 16 方和一名新主体，历史未被改动的责任方不被迫重签，但其责任、资金及程序机会继续保留。不能用多数接受覆盖缺少的独立必需方。

受签正文必须明确：实际主体/代表/Agent、履约族、原义务与单位、资源/资产/权限来源、金额和最大扣付、实际各类 fee payer、时界、先决条件、真实交付判据、可违约风险、证据/救济及有限退出、控制权变动、原合同关联、无合同替代及报价状态。每位接受者能读取其形成同意所必需的全部共同条款；隐私裁剪不能删除其风险或他人的实质影响。无读权时不能要求其接受一个摘要。

各方接受必须指向同一不可变条款及确定正文。增加/删除必需方、改变义务、来源/付款人、费用规则或上限、期限、风险分配、执行绑定及救济边界，均产生新修订并重新获得该次全部必需接受。不得把非空授权交集当作对完整请求的授权；只要请求有一项超出范围，原请求整体不成立。可提议更小范围，但它是一份重新明确呈现和接受的条款。

<a id="formation-policy-mandates"></a>
## 4. 长期授权与实际累计承诺

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

长期 mandate 是原 [Agent 有界授权](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) 的具体合同签署范围，不是另一种资产或独立信用账户。个人通过当前 owner authority，组织通过第 2 节业务附件的有效批准程序，明确授予代表/Agent 可签的义务种类、对手/对象、来源、方向、数量、最大风险、起止时间、累计上限和撤销权。不得继续转授；要换代表由同一来源正式修订，原额度和已成立义务不重置。

代表必须对每份实际条款产生明确、可验证的接受；原 mandate 不是未来任意条款的空白签名。授权内的合法具体接受无需每次再询问 owner，超出任何维度、新增责任或授权来源变化则停止并请求准确的新授权。授权安装/修订本身是独立权威动作，不能伪装成免费的 offer；普通沟通仍保持零业务效果。

每个原 grant / resource-or-right source / object scope / effective period 分开核对三类量：D 为已经实际花费或消费；O 为尚可能按原已成立业务义务合法扣付、损失或使用的最大残余；C 为未来已授权执行、程序和闭合费用，其中闭合部分独立保护。新合同在权威形成点必须满足逐资源或权利单位的 D + O + C 不超过原累计上限。费用与风险不能换算成一个模糊分数，也不跨独立 grant 自造总账户。

付款或消耗将同一原义务的 O 转为 D；实际费用将相应 C 转为 D；真实合法解除才释放未用 O/C。持有只是保护原余额，不再扣一次本金。退款或后来收入不减 D、不自动补回曾经花费的授权；已发生责任未付、原结果未知、仍可触发的判付或返还不能提前释放。拆单、并发、换请求、代表/Agent/owner 变更、期满或重签都不能让原承诺消失。

撤销或到期停止新的代理承诺，不撤销已成立债务。原领域可以按激活时明确成立的狭义处分权结清既有资金和程序，并把实际消耗归原授权键；这不是复活已撤销 mandate。新的生产、移动、权利使用仍须当前领域授权。原方失联、组织保护或退休不能把未结责任自动转给新 owner；没有安全继受时停止新工作、保护原结果并走原有限退出。

<a id="formation-policy-families"></a>
## 5. 履约族与世界承诺

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

| 已采纳族 | 本次可以明确接受的义务 | 世界生效与原领域接口 |
| --- | --- | --- |
| 即时原子交换 | 多方明确的 Electricity/Data、原直接转移条件成立的材料、同一世界内原 MainToken 可自由转移 liquid balance；实际 source/权限/nonce 与全部费用明确，全部资金来自提交前余额 | 同一次事务完成全部腿及费用，或全部无该次效果；有限类别及原权威见 [履约族接收](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-family-handoff)。不含外部 OC/桥、受限 starter 支持、归属权或 credential 转让；不能靠多次旧 Settle 拼接。 |
| 有限供能 | 原个人供能 v1 的有限电力检查点、Data 价款及既有完整救济；组织/多方供能按一般服务的对应资源义务及来源校验 | v1 原限制不变；一般方采用 [服务义务](../../world-runtime/runtime/continuing-service-fulfillment-and-remedies.design.md#service-obligations) 和原资源交付事实，形成不表示已供能。 |
| 维护/维修 | 明确设施、维护操作/输入/范围、预定窗口、接收与验收事实、失败责任及费用 | [一般服务激活](../../world-runtime/runtime/continuing-service-fulfillment-and-remedies.design.md#service-activation) 接受本政策完整形成 bundle；原维护生产者实际提交才证明维护效果。 |
| 物料运输 | 指定 cargo/数量/来源与目的地、保管和所有权阶段、损耗分配、支付里程碑、返回/救济 | [领域证据](../../world-runtime/runtime/continuing-service-fulfillment-and-remedies.design.md#service-domain-evidence) 消费原物流、目的地存储及商业 receipt；发出、到达、接收、所有权与最终付款分开。 |
| 一般共同工作 | 有限工作单元及其原领域可观察完成谓词、依赖、各方成本/贡献和明确责任 | [服务责任](../../world-runtime/runtime/continuing-service-fulfillment-and-remedies.design.md#service-parties-liability) 与 [有界恢复](../../world-runtime/runtime/continuing-service-fulfillment-and-remedies.design.md#service-scoped-recovery) 拥有原工作及未决范围；不能以“合作完成”的文本证明。 |
| 合作研究 | 原工作范围、贡献角色、成果使用权/分配及其明确变更/转让条件 | [R&D 激活](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-charter-activation) 与 [成员 epoch](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-charter-membership-epochs) 同根消费形成校验；实质工作前 charter 仍必需，后续贡献/份额不由形成层生成。 |

一般服务族的 [玩法与救济政策](gameplay-continuing-service-fulfillment-and-remedies-contract.prd.md) 主责归责条件、时间窗口、赔付/保全、有限审查及退出；原供能 v1 仍由 [原供能政策](gameplay-checkpoint-power-service-contract.prd.md#service-policy-scope) 主责。本政策不另设裁决机构，不把通过形成校验解释为未来履约保证。

多个工作或义务可以在同一份一般服务条款中组合；其依赖必须无环且分别有完整来源、最大责任和退出。激活一次建立完整原领域义务，后续以原 obligation/unit identity 执行。组合不是任意脚本：不能嵌入未声明义务、隐藏 fee hook、先产生不可逆效果再补其他方接受，或用结果未知的既有工作作新资金。

<a id="formation-policy-exit"></a>
## 6. 撤回、有限退出、继受与控制变化

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

生效前任一必需方可撤回自己的接受；提议方或有权必需方可关闭未生效原提议。最终接受不自动提交；显式激活与撤回依权威序竞争，先关闭则无业务效果，先激活则保留原合同，后来撤回仅停止新的代理行为。不能把 Stop Agent、关闭页面、换目标或到期显示当作已取消义务。

生效后，原领域拥有修复/重新履行、终止/结清、争议/最小保全、明确继受及无安全路径时停止新工作的规则。每条义务在形成前已列出有限程序、真实 payer 和足额闭合路径；一方不能耗尽另一方的机会或最后结清预算。无争议、到期且可安全履行的范围继续；无权或无变化的请求不能制造新 hold、延长期限或挤占他人程序槽。

继受是对准确剩余范围的新明示接受事件，保留原 agreement/obligation/unit lineage、已发生结果与历史责任；所有被改变义务或资金权利的原方及新方均按当前 authority 接受。只换组织代表不转移组织责任；换 principal 不能通过改显示名完成。新委托承担未来范围的额度与费用，旧方已花费、旧责任和旧程序机会不迁走。旧结果未知、不可分工作或合法保全范围没有可执行交接证明时先恢复原件，不让新旧执行权并行。

<a id="formation-policy-readable"></a>
## 7. 同源可读结果与实际后果

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

玩家看到授权来自谁、谁实际接受了哪版条款、哪些条件仍未满足、已承诺的最大成本/风险、哪些结果真实发生，以及当前可以修改、撤回、恢复、补证、等待或退出。通信 ACK、接受登记、世界激活、工作开始、实际交付、责任 final、付款与观察未知分别表达；不使用一个“已完成”掩盖未付债权。

形成和后续消费沿原 causal-decision receipt、领域结果及当前受众读取。组织 principal、签署代表及任期、执行 Agent 和实际资金权利人分别显示，旧签署者读取自己原责任所需事实不等于有新写权或可读新 owner 私有资料。只有受众确实可读的理由/证据可展示，不公开私有 prompt 或内部推理。

形成 receipt 不是声誉、资格或 R&D 贡献。只有 [原声誉来源](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-source) 及其当期适用消费者认可的实际领域事实才能派生相应记录；合同成立不发信用，组织代表变化不转移个人 credential。

一般即时交换可在完整受签条款中明确采用 source_projection_use，写出 schema、purpose、context、audience、消费者政策版本/hash及真实效果判据；其中 agent_acquisition 的有限用途由 [情境消费者政策](gameplay-contextual-reputation-contract.prd.md#contextual-reputation-consumer-policy)唯一维护。每方在签署前看见这项用途，改变即重新接受；未声明和旧 v1 原件不追溯采用。形成/支付 executor 仍只写原真实合同效果，后续消费者只读已实际双向非零履行及其真实控制来源，不从接受、预览或 ACK 生成 score、资格或资源奖励。

## 8. 验收与结束条件

本政策复用 CR001 原 Trace，不新增产品要求、global role 或通用裁决 authority。下列正向与反向范围缺任一项，不能以已有两个人交换样例代签。

| 适用义务 | 方法 | 独立判断 |
| --- | --- | --- |
| 普通组织授权、成员资金隔离、代表任期 | [CF09](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-09-organization) | 正确附件/门槛可形成；任意 passed/成员称号/OC grant 不可形成，第三方资金不被暗用。 |
| 多方同版、完整必需集合与可读性 | [CF10](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-10-multiparty) | 缺一/混版/非空但不完整授权均整份拒绝；修改重新接受。 |
| 长期实际花费/承诺/闭合暴露 | [CF11](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-11-mandate-exposure) | 并发、拆单和变更不超过原额度，退款不洗已花，原债务不被撤权清除。 |
| 全已采纳履约族、真实世界形成 | [CF12](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-12-family-handoff) | 各族正向形成及原领域接收均有原件；形成不等于交付/贡献/声誉。 |
| 撤回、责任保留与显式继受 | [CF13](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-13-revocation-succession) | 两种权威竞态、原责任、新旧范围和成本分别可核。 |
| 完整费用、容量与受保护退出 | [CF14](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-14-bounded-cost) | 完整调用图和最大次数独立复算；超限零部分效果，控制/原件读取不被执行额度饿死。 |
| 恢复、当前受众和实际消费者 | [CF15](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-15-recovery-audience)、[CF16](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-16-consumers)、[CF17](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-17-integration-compatibility) | 原结果一次、根未知正确 fence、受众无越权；旧 v1/CF01–08 保持。 |

实际源码、制度分类/激活、免费非绑定登记、当前授权与完整资金保护、各族 executor、费用/程序来源、持久恢复和正式消费者都须由同一实施候选交付并验证。设计结束条件是这些边界有具体合同和独立方法；文档存在不证明前述运行条件已经成立。
