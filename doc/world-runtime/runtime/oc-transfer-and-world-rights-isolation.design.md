# OC 转让与世界权利双向隔离系统设计

- 设计 ID：`DES-WR-OC-ISOLATION`
- 状态：`active`，目标技术合同；当前转账路径与各域已实现范围见 §3，本文不表示完整消费者已经接线或通过运行验收。
- Owner role：`runtime_engineer`；`blockchain_ops_engineer` 复核链资产、签名、费用和额度桥，`gameplay_designer` 复核原权利边界，`qa_engineer` 复核跨域方法。
- 上游：[GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011) 与 [GAME-018 AC03](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-03)。治理主体、事项快照与单向额度桥沿 [GAME-019](../../game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md)，本文只消费这些来源。
- 源码审读基线：`2731f781536018a857e5daf2cfa7ede504e9cd69`；相对源码链接用于说明当前路径，实际验证须固定自己的候选、world、链/网络及激活版本。

## 1. 问题、目标与非目标

链上提交、区块包含、转账实际生效、世界资格成立和真实业务履行是不同结果。玩家持有 OC、展示一张链上 receipt，或已经履行世界合同，都不能省略另一领域所要求的来源、授权、资金、资源和当前条件。反向也一样：世界库存、贡献、资格和合同不自带 OC 铸造、转账或外部赎回能力。

本设计规定各原消费者在输入、提交、恢复和读面上的隔离合同，完整目录见 §5。规则落在原链资产入口、原业务 writer、制度实例和原结果 reader；不新增统一的世界资格 action、隔离协调器、币种、信用余额或全世界扫描服务。显式合法付款、既有同根原子合同及受限单向 quota bridge 仍按各自原合同执行。

## 2. 需求承接与分配

原要求的比较动作 `transfer_oc / qualify_world_capability / earn_world_credit / enter_contract` 是产品语义，不因此成为四个已实现 API。技术入口、真实生产者、原始结果和可用查询均须按本文目录绑定；无对应入口时如实报告能力缺项。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | 链资产、原操作、世界事实及 quota 的身份、单位、版本和结果不可互换 | [来源与结果](#oc-isolation-source-types) | 原链资产、World writer、资格及服务 producer | 不用 balance、nonce 或链 included 单独证明 World 成功 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | 资源、信用、库存、合同、信誉、作者/准入及全部声明权利双向隔离 | [完整消费者](#oc-isolation-consumers) | 各域原权限、账本、生产/使用与实际结果 | 不新增通用兑换、资格捷径或未采纳机会 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | 治理输入仅在适用地方规则下与本地贡献合取，其他业务各守原 gate | [资格适用范围](#oc-isolation-governance) | GCB-004/005、区域 charter 与当前贡献来源 | 不把地方双资格扩为工业、合同、保供的通用前置 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | 独立操作保留各自结果，显式合法付款遵守其原原子业务 | [付款与组合](#oc-isolation-explicit-payment) | 原 native MainToken、共同合同及各付款 writer | 不以“隔离”禁止合法付款或拆分原子合同 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | 费用按原链/执行 authority；quota bridge 不产生治理、世界资产或反向兑付 | [费用与额度桥](#oc-isolation-fees-bridge) | 原 fee policy、计量、bridge order 与 quota 服务 | Phase 1 quote 不冒充扣费；不新增 Phase 2 模型 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | 失败、到期、重复、断连和 replay 各自至多一次，未知结果不重做 | [事务与恢复](#oc-isolation-recovery) | 原 operation/nonce、DWE、G1、当前 serving | 不把本地旧快照或读失败当作 canonical 未提交 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | 全部声明消费者与来源/费用/读取保持有界，完整请求不能静默截断 | [范围与预算](#oc-isolation-bounds) | 原 reader、writer、制度调用与 host/world 限额 | 不引入全库扫描、免费额外调用或自建全局去重账 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | 玩家分辨条件、成本、成功/失败和下一步，当前读取权限独立成立 | [跨入口结果](#oc-isolation-read-model) | Viewer、pure API、Agent 的原入口和 reader | 公开链可见不赋予私有 World 原件的读取权 |

## 3. 当前事实与目标差距

| 现有来源 | 可直接接受的事实 | 仍需目标接线的部分 |
| --- | --- | --- |
| [transfer submit](../../../crates/oasis7_chain_runtime/src/host/transfer_submit_api.rs) 的 preflight 与 tracker | 已检查原 expiry、asset、余额与 nonce；tracker 的 `confirmed` 来自 committed batch | 单独 `confirmed` 不说明执行是否产生原 World 转账效果，更不说明其他资格 |
| [转账 action](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_core_main_token.rs) 与 [货币准备/安装](../../../crates/oasis7_world_state/src/world/main_token_monetary_publication.rs) | `TransferMainToken` 准备实际 liquid 余额和原 sender nonce；余额不足、溢出、nonce replay 拒绝 | 当前独立转账机制不代签 §5 所有消费者和跨入口恢复 |
| [理想交易 Phase 1](../../p2p/token/mainchain-token-ideal-transaction.design.md#itx-signed-metadata) | 签名绑定链/网络、资产、expiry 和 metadata；`client_request_id` 不是 consensus 去重键 | 原 Phase 2 payer/sponsor/真实收费仍需其独立合法模型与实现 |
| §5 的各专业合同、[GR-011 读面](../../world-simulator/viewer/viewer-governance-targets-uiux.design.md#t-gov-08-gr-011) | 资格、工业、合同、保供等已有各自规则和方法 | 各真实入口须接收本文的双向隔离、实际原结果与组合方法；静态设计不是接线或 full 验收 |

## 4. 来源、身份与结果边界

<a id="oc-isolation-source-types"></a>
### 4.1 原事实类型

以下是原 producer/reader 必须提供或保留的逻辑含义，不新增可写协议或另一份权威余额。原编码已含字段时直接使用；缺项不得由 UI、账户昵称、模型推断或同名资产补全。

| 类型 | 最小精确来源 | 证明内容与限制 |
| --- | --- | --- |
| 原 OC 操作 `O` | 实际 chain/network、asset、sender/recipient、amount、sender nonce、签名 payload digest、原 action/transaction identity；各来源原 commit/result 引用 | 签名授权、提交/包含和原执行结果分开。当前账户余额和 last nonce 是现态，不是某笔操作的成功证明；外链/不同 network/同名 asset 不可代入 |
| 原世界操作 `W` | world、领域/实例、原 operation/action、payload/terms digest、对象和完整 scope、主体/代表、source/authorization refs、policy/artifact/activation、原 read cut 与 commit/result | 只证明其原 writer 记录的资格、库存、合同、生产、费用或其他效果；不能由 receipt 的存在推出完整履行、结清或永久资格 |
| 世界事实/信用 `F` | 原 producer/domain、world/对象、主体、事实/用途、数量及原单位、完整事实区间、原 effect/source-use、当前有效性/更正版本、原件与授权读取来源 | 只有原 producer 能解释其含义。物理交付量、履行贡献、情境声誉、资源入账互不换单位，不被包装成通用可转让 credit |
| bridge 服务订单 `Q` | 原桥接 scope、原链操作、稳定 order identity、服务侧用户/项目/额度对象、topup 与 query/reconcile 原结果 | 证明特定服务额度的实际状态；不证明世界资产、治理主体、投票 weight、作者或准入；无反向 OC 兑换 |

`OC` 与 native `MainToken` 的名称不产生两种新货币。是否同一资产、同一账本和同一可支付来源，须由原 chain/network/world/asset 关系与原 writer 确认。链上 receipt 引用可作为明确业务的一个证据；不能被通用“已付款”标记替换全部授权及结果。

### 4.2 不合并的结果轴

一份对照项分别引用一个 `O` 和一个 `W`，每侧保留原状态、可公开理由、实际效果/费用、原结果引用及当前可行下一步；多个对照项沿原有界查询分别呈现。bridge 使用其独立 `Q` 结果，不折叠进 world credit。对照项是只读投影，不额外写确认记录、不参与业务原子性，也不要求先执行另一侧。

必须区分：未提交的草稿、已送达/待决、已被链包含、执行已确认成功、执行已确认拒绝，以及当前无法确认。提交 ACK、链 inclusion、`accepted`、`active`、`fulfilled`、已付款各保留原语义；没有原执行结果时不能因 tracker `confirmed` 显示“World 成功”。若原 reader 不暴露足够历史，则结果为未确认并指出原查询/能力缺项，不能用当前余额差、last nonce 前进或旧 snapshot 补签。

## 5. 全部世界消费者与允许效果

<a id="oc-isolation-consumers"></a>
### 5.1 消费者目录

`GRI-C01` 至 `GRI-C14` 是本合同的静态验收目录，不是新运行枚举或每次转账都要调用的服务。每个实际入口只执行自己原合同的完整读写集。所有 native/制度/WASM 适用路径、直接内部调用、continuation、恢复及 replay 均沿同一原 authority，不能靠换入口绕开。

| ID / 原义务 | 实际 source / 当前条件 | 允许的原效果 | OC 与世界事实的双向边界 |
| --- | --- | --- | --- |
| GRI-C01 资源与库存 | 原资源/材料/物理 writer、owner、位置、容量及真实转移/生产；例如 [工业实体生命周期](governed-extension-lifecycle.design.md#extension-ordinary-authority) | 原合法数量、库存、位置、WIP 与对应费用；保留原守恒和 held/gross 规则 | OC 余额/receipt 不铸造材料、Electricity、Data 或库存；物品、资源或其估价不自行产生 OC transfer/mint/redeem |
| GRI-C02 服务信用与贡献事实 | [真实服务结果](continuing-service-fulfillment-and-remedies.design.md#service-read) 及实际已采纳贡献 producer，逐来源保留本来单位/用途 | 原履行/贡献记录及其明确被授权消费者；不存在同名 producer 时不记一笔猜测 credit | `earn_world_credit` 不定义新币或 API；ServiceOutcome、reputation、quota、Electricity credit 不互相改名；OC 付款不冒充服务已完成，服务事实不自行成为可赎回资产 |
| GRI-C03 合同及既有责任 | [同版接受](agent-mediated-contract-formation.design.md#contract-formation-required-acceptances)、[原履约族](agent-mediated-contract-formation.design.md#contract-formation-family-handoff) 与各原资金/资源/必需方 | 原接受、有限承诺、真实履行、款项和责任；接受本身不等于世界履行 | 支付人不自动成为资产 owner、签署代表或履行人；合同成立/fulfilled 不自动 OC 收入；明确合同支付仍按 §6 |
| GRI-C04 市场信誉与情境资格 | [原实际来源与用途](contextual-reputation-evidence-and-review.design.md#reputation-all-consumers)、当前窗口/更正/控制人及独立性 | 原 purpose 的有界资格或事实，旧历史保留 | OC、支付额、组织标签或接受事件不生成全局 score；资格/recognition 不自动返币、费用折扣或未知机会 |
| GRI-C05 工业作者与研发权益 | 原创作来源；[研发激活](rd-charter-provenance-and-share-transfer.design.md#rd-charter-activation)、[成员 epoch](rd-charter-provenance-and-share-transfer.design.md#rd-charter-membership-epochs)、原贡献/权利槽 | 原作者/贡献、已接受 slot/bps 与合法有限转让；旧费用、已付与责任保留 | 付款、股权持有或改代表不改旧作者；作者/slot/估值不直接成为 liquid OC；明确收益权支付沿原分配/许可结算 |
| GRI-C06 提案、模拟、试点、准入和已采纳 product/factory | [工业准入](industrial-creation-simulation-and-admission.design.md#creation-admission) 与 [普通扩展授权](governed-extension-lifecycle.design.md#extension-ordinary-authority)，当前作者/授权/完整 scope/time、技术与资源条件 | 原批准范围内一次实际启用/生产和成本；退出仅保原允许安全/价值保全/义务动作 | OC 不补作者、试点或治理准入，不令模拟产物成为库存；准入/批准不产生 OC，也不使未来收入可用作本次资金 |
| GRI-C07 许可、版税和收益权 | [原使用/交付与结算](industrial-license-and-royalty-settlement.design.md#license-settlement)、当前有限许可、原 pool/分配键/真实 payer | 原有限用途、逐真实使用结算和原授权支付 | 买入 OC 不获得许可；license receipt 不凭空生 royalty；转让、开放或退出不追溯重发已付、清掉旧责任 |
| GRI-C08 Agent claim 与制造 | [claim purpose](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#claim-purpose-source)、[制造合同](agent-industrial-manufacturing.design.md)，原 slot/并集容量、来源、授权、预算 | 原合法 claim/受限 starter 使用或实际制造/交付；reserve/ready 不当 delivered | OC 资金不替代当前 purpose/capacity/制造前置；restricted starter 只沿原 slot-1 消费范围，不转 liquid 或塞进通用交换；Agent/资格不自动兑币 |
| GRI-C09 charter 与地方资格 | [区域当前授权及双来源](regional-charter-authority-and-recovery.design.md#charter-authority-interfaces)；适用条件见 §5.2 | 原范围内 charter 能力及地方事项资格；资产/tenure/合同仍分属其 owner | OC 与贡献各不能单独覆盖另一缺项；charter 不代表 levy、tenure 或世界资产所有权，不创造 OC 权利 |
| GRI-C10 普通治理与事项快照 | [GAME-019](../../game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md) 的主体聚合、锁定/委托/控制人/事项快照；[当前分类执行](governance-classification-and-constitutional-execution.design.md#governance-commit) | 本事项原资格、快照、投票及合法后续执行；新动作当前重验 | 普通余额、外部转让、quota 或市场信誉不替代治理资格；生效窗口后的转让/撤回不追溯改票或双投；治理结果不铸币，明确财库拨款另需原 authority |
| GRI-C11 tenure | [取得/续期/转让](location-tenure-and-disposition.design.md#tenure-acquire-renew-transfer)，原空间、holder、用途、期限、义务和付款 | 原登记及依法处置/补偿，权利停止与物理腾空分开 | OC 不购买未经接受的 tenure；tenure 转让不自动出售物品或接替旧债，补偿只来自原真实 payer，不自行产生可赎回 OC |
| GRI-C12 levy | [具体征收授权](public-levy-authorization-and-settlement.design.md#levy-authority)、[原账本](public-levy-authorization-and-settlement.design.md#levy-ledger) 和当前范围/期限/上限 | 合法 levy 的一次实际扣款、专用分账、原路释放/合法支出 | 余额不是征收权，budget 不是资金；成员、到访或历史贡献不自带 levy；拒付不自动形成通行/治理禁令；无授权不自动兑换世界资源付税 |
| GRI-C13 竞争与冲突 | [原参入/暴露](chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-authority)、当前保护/资金/授权及原 war profile | 仅当前已合法激活的范围、损失、占用和有资金奖励；当前 War=Defer 保留 | OC 不产生参战同意、攻击权、结果或胜利；排名/控制不自动写治理权、tenure、reputation 或 OC 奖励 |
| GRI-C14 紧急保供 | [保供授权与结算](emergency-supply-authorization-and-settlement.design.md#des-wr-es-004)，原需求/资格、授权、保护、实际到货/里程碑、真实预算 | 原采购、分配、配给、补偿与有限使用 | OC 不替代保供资格/授权；无自动继承地方双资格；受限保供资源不转免费库存/现金赎回；allocated/partial 不冒称 fulfilled |

上述“世界事实不自动产生 OC”不取消原明确的合法付款、奖励、财库分配或有资金补偿。它们必须另有原业务中的 payer、source、条件、授权、金额/单位及唯一效果，不能仅凭通用 receipt 生成一笔转账。

<a id="oc-isolation-governance"></a>
### 5.2 治理输入与地方合取

锁定 OC、可撤回委托及实际控制人快照只进入其原事项、范围与窗口。治理 producer 提供主体、matter/scope、锁定/委托/控制人来源、政策/快照版本、有效期及 eligible/ineligible/unknown；区域资格消费者按 [charter 双资格](regional-charter-authority-and-recovery.design.md#charter-dual-qualification) 同时核当前地方贡献、charter 能力和受保护边界。两个来源分开保留，不压成全局分数，不允许资本或贡献抵消另一侧缺项。

只有当前声明的区域 charter/地方治理规则要求时才用该合取。工业提案/准入、普通市场合同、紧急保供，以及原公共通行/独立恢复各走本来 gate；不会因为主体不满足地方投票资格就额外拒绝合法业务，也不会因为持币就跳过业务所需的身份、资源、服务或保护条件。普通治理的适用规则由 GCB/GAME-019 决定，本文不替它设置新锁期、权重或控制人公式。

## 6. 合法显式业务与独立操作

<a id="oc-isolation-explicit-payment"></a>
### 6.1 独立结果与原原子族

是否独立由原业务的已接受条款、source 依赖及实际写集决定，不能由调用者选一个标签。若世界动作明确依赖原付款、锁定或履行结果，就先核该原依赖；本设计不允许省略它来制造“独立成功”。一个世界操作可以是含完整子结果的原共同根事务，对照项不会裁掉其必需方或原子腿。

| 原组合 | 必须检查与实际提交 | 失败、结果和保留 |
| --- | --- | --- |
| 独立 OC 转账 `O` 与独立世界操作 `W` | 各自在自己的原 authority 上校验、计费并产生原结果；一侧不替另一侧签名/授权 | `O` 成功而 `W` 拒绝，保留 `O` 与 `W` 的 blocker；`W` 成功而 `O` 拒绝，保留 `W`，不自动撤资格/抹贡献。没有预先签订补偿条款就不自动反转已确认交易 |
| 明确采用 native MainToken liquid 的同世界 `immediate_exchange_v2` | 按 [原完整履约族](agent-mediated-contract-formation.design.md#contract-formation-family-handoff) 和 [原共同激活](agent-mediated-contract-formation.design.md#contract-formation-common-activation) 收齐必需方、原 token 授权/nonce、真实 owner/资源/位置/费用；所有腿、费用、held/usage、原结果同根 | 任一前置失败无部分业务效果；提交未知恢复原整个事务。不能先执行一笔外部转账再用它代签另一腿；不能把应原子提交的合同分成两次发布 |
| 原业务明确规定的先履行后支付、里程碑、补偿或收益分配 | 只按原 source-use、实际履行/里程碑、原 pool/付款权及当前合法结算 authority 消费一次 | 真实已发生工作、未结责任、待付与已付分开；结算失败不重做物理服务、不改旧作者/slot/bps/原已付；迟到结果恢复原付款，不换收款人/nonce 再付 |

保留已确认世界记录不保证资格永久有效。新动作仍按当前原规则重验锁定、资金、委托、贡献、许可与窗口；失效可以阻止新动作，不能由此抹掉旧贡献、原付款、历史投票或已接受责任。

同世界 native MainToken liquid 是合法付款正例：它仍来自原货币账本，资金必须是原合法可用 liquid，并具真实 source owner 的同版接受、必要签名和当前授权；合法第三方来源及受托/代表边界沿原合同。restricted、held 或尚未按原规则解锁为 liquid 的 vesting 余额不得充作自由可用资金。原交换不接外链资产、跨分片兑换、bridge quota、治理/控制凭据、未声明 RD 权利或未来收入；合法 RD 整槽转让等另走原专属合同，而非扩大即时交换腿。

独立转账的当前 Phase 1 测试可采用原 liquid 账户 A=40、B=0，合法转 10 后 A=30、B=10，原 nonce 前进一次；前提是固定候选确实使用原未执行额外 fee 的该路径且无其他交易。此算例说明真实余额变化及世界资格不变，不制定费率，也不适用于已有其他实际费用的业务或未来 Phase 2。

<a id="oc-isolation-fees-bridge"></a>
### 6.2 费用、机会成本与单向额度桥

原费用事实至少分为报价/上限、已保留、实际扣费/去向和原路释放；原业务没有某阶段时明确不适用，不制造假 hold。链上成本由 [P2P 验证与费用边界](../../p2p/token/mainchain-token-ideal-transaction.design.md#itx-validation) 决定。Phase 1 `max_fee/fee_asset_id` 是签名 metadata，不扣款；不得因显示数字或“转账隔离”宣称所有操作免费。payer/sponsor、priority、实际 fee affordability/settlement 只有满足 [原 Phase 2 gate](../../p2p/token/mainchain-token-ideal-transaction.design.md#itx-phase2-gate) 的合法版本才生效。

世界资源消耗、WASM Data/Electricity、合同执行成本、许可、levy、物流、补偿等仍分别来自原合法政策和实际 writer；不得把链 quote 当缴清这些费用。每项保留 payer、原单位、source、policy/version、hold/actual/refund 与 receipt，不净成一个无来源的 OC 总价。无适用费权、无准确预算或可用来源时按原业务拒绝，不以本设计补一条零费规则。机会成本包括原锁定/占用、时间与已确认消耗；未来产出和未完成返还不是现有资金。

[operator-managed quota bridge](../../p2p/token/mainchain-token-newapi-quota-bridge.design.md#bridge-recovery-state) 是已有单向服务业务。只有原已支持 scope、真实链输入与稳定 order 才能触发服务 topup；`credited` 和 `reconciled` 分开。topup 成功后 query、项目或上下文恢复失败，保留原成功；unknown 查原订单，不能删除重建 order/身份或重发一次充值。额度不进入 World 库存、治理权、投票 weight、claim starter 或市场信誉；额度/服务消耗也没有 OC 赎回出口。该边界不改变原桥的价格、服务认证、限额和运维 authority。

## 7. 原请求、版本与持久化

<a id="oc-isolation-recovery"></a>
### 7.1 提交与去重

每个 consumer 在原提交点按原 canonical 输入重验身份、完整请求 scope/time、当前授权/撤销、对象及来源版本、资产可用/held、成本与保护；有一项不满足就按原事务无部分业务效果拒绝。不得缩成可用子集、换政策或换实例后自动继续。需要新范围时由原授权方显式形成新请求，既有结果和义务仍保留。

OC 沿原 sender nonce、签名 payload 和 action/transaction identity，World 沿原 domain operation/业务 effect/source-use 与版本，bridge 沿原稳定 order；相同内容的重复可能读回原结果，也可能由原 API 返回 nonce replay，均须保持至多一次实际效果。本文不将 `client_request_id` 升格为 consensus 去重，不承诺当前 API 必然缓存成功回包。相同 key 不同 payload 拒绝；改 request ID 不解除原业务去重和责任。

| 已知状态 | 正确恢复 | 禁止的推断或动作 |
| --- | --- | --- |
| 原 authority 确认未提交或原业务最终拒绝 | 保留拒绝和已经存在的另一侧结果；按当前原条件修复后，只有原协议允许才新发操作 | 不抹另一侧 receipt，不凭对照项自动补偿或补资格 |
| 原操作已提交且实际结果可证实 | 安装/读回原结果，保持原数量、费用、nonce、source-use 与历史版本 | ACK 丢失、投影落后、后来资格变化不重新执行原效果或补扣一次费 |
| 客户端丢 ACK、原结果暂不可读或 tracker 只有包含信息 | 用原签名 payload/action/operation 身份定位原结果，查询本身遵从当前 reader 的认证/公开规则；保持 outcome_unknown，恢复身份/查询能力后再决定 | 不用过期写授权代替当前读权，不用当前余额/last nonce/旧 snapshot 判定本笔成功或失败，不换 nonce/收款人/实例重发来探测 |
| durable commit 决定本身未知 | 沿下述 G1/authority 恢复原完整候选和原决定；只开放原合同许可的读取/恢复通路 | 不让旧 World 继续 mutate/save/依赖派发覆盖可能已提交事实，不把本地写失败视作 canonical 无决定 |

### 7.2 World 原子性与全 World 隔离

需要同根的业务沿 [ExecutionTransaction](../design.md#621-explicit-transaction-and-typed-delta-model) 准备完整写集：原业务状态、实际资源/物理结果、费用/hold、权利/消费索引、nonce/operation 及 receipt，统一发布。读面不另有一个可以先“确认”交易的写入口。

持久化遵从 [G1](runtime-storage-footprint-governance.design.md#storage-generation-g1-design)：`NotCommitted` 才可保留旧 World 可写；`Committed` 安装/恢复同代完整候选，之后维护失败不撤销；`StatusUnknown` 隔离整个不确定旧 World 的 mutator/save/依赖派发，查询原决定并对齐原 base generation、world/time、完整 staged 状态与索引后恢复。不是只冻结某个付款/资格对象。普通客户端丢 ACK 而 durable 结果已知时，仅核原操作，不凭此暂停整个已确定 World。

按 [DWE 原版本与历史](../design.md#runtime-version-design) 和 [当前 serving 恢复](../design.md#runtime-recovery-design) 消费实际 authority profile：待决 intent 按原 pending 规则重验；已固定决定/已提交结果恢复原版本和原完整结果，不按本机最新 manifest 重算。链共识与受控单 authority 的正式决定各走原机制，不互换证明。本地旧快照不说明 canonical 未提交；受限 serving 状态下只走其允许查询/恢复，不借本设计绕开权限或状态闸门。

重复、重连、快照+后缀恢复及 canonical replay 不再发送新的实时业务请求或外部副作用；确定性历史 apply 仍按原版本、原 delta 和顺序重建应有状态，不产生第二份业务效果。依赖方只消费原已确认结果，原 nonce/source-use/义务/费用前缀从同一完整代恢复；缺失原件或完整性证明保持不确定，不能清空索引后重做。

## 8. 跨入口的结果表达

<a id="oc-isolation-read-model"></a>
Viewer、pure API 和 Agent 使用原入口与当前可读结果，按 [GR-011 目标读面](../../world-simulator/viewer/viewer-governance-targets-uiux.design.md#t-gov-08-gr-011) 及 [pure API 能力映射](../../world-simulator/llm/external-agent-game-api.design.md#api-primary-entry-applicability) 接收，不新增钱包/兑换/资格万能按钮。每侧至少说明所做动作、原对象/请求、条件及缺项、范围/期限、实际已发生与未发生的效果、原费用/保留成本、来源新鲜度、当前结果和下一步。

独立两侧可以同时显示“OC 原转账成功；世界准入拒绝”或“世界贡献已记录；OC 原支付拒绝”。没有原结果时明确是哪一侧未确认。不得显示一个总 success/failed 覆盖两者；资格待审、已有旧结果与当前请求分开。可行下一步来自原 gate：补齐有权来源、取得当前许可、按原资金来源补款、选择原已存在替代、等待原决定、查询原请求或停止新操作。下一步不自动执行，也不保证一定有补偿/退款。

公开链字段继续遵从原公开 reader，不另发明鉴权；私有 World、治理主体、贡献原件、合同与 bridge 对象按其原当前读取权限。历史可追溯不等于所有受众可读；当前失去读取权时保留 receipt 原事实，只返回可公开状态/受限原因和原允许恢复路径，不泄露另一主体的私有凭据、余额细节或 token_key。结果 query-only 不重新产生 World 效果、付款、业务 fee、服务 topup 或义务；既有读查询的限额/成本及必要审计 metadata 按原 reader 处理。

原客户端缺少精确 lookup、查询失败或没有对应 action 时，应显示实际能力缺项与原可用查询方式；不把 snapshot 当原回执，不通过新 nonce/新请求尝试确认旧结果，也不将 SDK 或一个原生入口外推为全部 surface 已支持。

## 9. 有界范围、成本与全入口覆盖

<a id="oc-isolation-bounds"></a>
完整目录是静态设计与验收边界，不意味着一次 OC transfer 读取所有世界资格。独立链操作只读取原链资产必要集合；某个 World 操作只读取其原完整、事先声明且受权的业务集合。只读对照项至多关联一个原 OC 操作与一个原 World 操作；bridge 单独按一个原 order 查询。多项比较沿原分页/逐项读取，对照不增加业务调用、写事务或费用模型。

每次请求使用原领域的有限对象数、scope atoms、参与方、证据/历史索引、编码输入/输出、执行调用和时限预算；取实际 public API、host/executor、world、manifest、模块和原业务中更严格的限额。限额或来源完整性未声明就不开放对应写路径，不默认无限。原一次业务需要的完整 scope/授权集合不能靠分页分成几次部分发布；超限在原准备/写入前拒绝，必须按原规则显式重新提案。

按精确原 identity、原索引/水位与可验证完整性获取必要证明；历史原件在原 reader 允许的有界窗口内定位。缺页、旧水位、迟到或异代结果不是不存在；不得先扫描全库再截断。相同原操作的 nonce/source-use/消费索引和长期义务仍按各域的持久化保留，不能因读面分页、TTL 或容量紧张忘掉去重。

本设计没有新制度 invoke 或状态账；原业务已有的模块调用、原 world effects 和真实读开销按其原费用/配额执行。显式组合必须事前计入全部原调用与编码 envelope，不能用“隔离检查”添加未计费调用或提高 public 请求额度。资源/预算与费用事实可以进入[成熟经济审计](mature-world-economic-audit.design.md)，审计读面不反过来成为资格 authority。

实现接收时，C01–C14 每行列出实际 action/模块实例/内部 writer、结果 reader、原 gate、可写集、拒绝/unknown 行为和原方法；同业务的原生、模块、恢复等路径全部接收。当前不支持的真实组合保留明确缺项，不把 support label、链接存在或另一个 consumer 的通过当成该行已实现；对应设计义务仍由本合同及原领域条款保留。

## 10. 兼容与演进

现有 OC、World、quota 记录不因本文出现而复制到新账，也不自动转换成新身份、资格或外部资产。旧历史按原 schema/activation/terms/receipt 读取；新消费者必须识别自己实际支持的来源和结果层级。遇到未知 source/schema 或无法证实完整范围，拒绝依赖它的新业务并保留原历史，不猜测为零或成功。

付款、资格、政策、代表或授权变化只按原域的版本/生效/更正程序影响新动作或明确允许的未结部分；已经确认的余额变动、贡献、作者、slot/bps、费用及既有责任保持原身份。合法有资金纠正追加 linked 结果，不回写旧 receipt，不借回滚、开放或退出重复支付/赎回。正式功能和发行阶段仍以实际接线、原治理/部署 authority 与对应验证证据决定。

## 11. 验证设计与可追溯性

独立方法见[OC/世界隔离验收手册](../../testing/manual/oc-transfer-world-rights-isolation-acceptance.manual.md)。required 检查固定候选的正常/拒绝边界与当前真实入口；原 GR-011 的 full 验收还需同候选、实际 authority、原 chain/world/activation、全部适用消费者和恢复/重连/replay 证据。文档/链接检查及算例只证明设计内部关系，不证明实现、运行性能或玩家理解。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | [来源与结果](#oc-isolation-source-types) | 独立来源、原状态与单位，不把包含/余额/nonce 当原 World 效果 | [OI-01](../../testing/manual/oc-transfer-world-rights-isolation-acceptance.manual.md#oc-isolation-01-source)；required/full；固定真实链/world/版本及原结果 | 签名/payload、action、原 receipt、各原账和当前读权限 | 单独 tracker 或截图不证明业务效果 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | [完整消费者](#oc-isolation-consumers) | C01–C14 两向不自动权利、资产或外部价值 | [OI-02](../../testing/manual/oc-transfer-world-rights-isolation-acceptance.manual.md#oc-isolation-02-consumers)；required/full；逐实际 consumer 绑定原正常场景及混用负例 | 原/后权利、资源、信誉、库存、合同与 OC 差分 | 一个原业务通过不代表其他行 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | [资格适用范围](#oc-isolation-governance) | 地方声明的双资格与其他业务独立 gate | [OI-03](../../testing/manual/oc-transfer-world-rights-isolation-acceptance.manual.md#oc-isolation-03-gates)；required/full；原地方规则/资格/贡献和各业务合法 fixture | 适用/不适用、eligible/ineligible/unknown、原单次结果 | 不替 GCB producer 的全部独立验收 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | [付款与组合](#oc-isolation-explicit-payment) | 单侧成败保留与原同根原子族分别成立 | [OI-04](../../testing/manual/oc-transfer-world-rights-isolation-acceptance.manual.md#oc-isolation-04-payment)；required/full；独立 O/W 与原合法 liquid 付款/原子业务分组 | 完整原写集、资金/nonce、两侧结果或同根结果 | 不开放新交换、payer 或退款权 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | [费用与额度桥](#oc-isolation-fees-bridge) | quote/actual/hold/release、bridge credited/reconciled、无反向兑付 | [OI-05](../../testing/manual/oc-transfer-world-rights-isolation-acceptance.manual.md#oc-isolation-05-fees-bridge)；required/full；实际适用 fee/bridge profile | 原单位/金额/去向、order、原 topup/query 与历史 | Phase 1 不证明 Phase 2 已实现 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | [事务与恢复](#oc-isolation-recovery) | 各原身份一次效果，unknown/G1/旧版本恢复准确 | [OI-06](../../testing/manual/oc-transfer-world-rights-isolation-acceptance.manual.md#oc-isolation-06-recovery)；full；实际 authority 下故障/丢 ACK/断连/快照后缀与 replay | 原完整决定、同代状态、source-use/nonce、费用和两侧历史 | 本地快照及单一错误码不能代签 canonical 结论 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | [范围与预算](#oc-isolation-bounds) | 原完整 scope/来源/预算与全部实际入口，无静默截断/新增调用 | [OI-07](../../testing/manual/oc-transfer-world-rights-isolation-acceptance.manual.md#oc-isolation-07-coverage)；required/full；逐实际 writer/reader、原上下层限额和边界候选 | 完整目录、输入/输出及调用预算、超限/缺页/异代拒绝 | 目录存在不证明消费者已接线 |
| [GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)；Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2 | [跨入口结果](#oc-isolation-read-model) | 各侧条件/成本/结果/下一步可辨，当前读权与原件恢复保留 | [OI-08](../../testing/manual/oc-transfer-world-rights-isolation-acceptance.manual.md#oc-isolation-08-surfaces)；required/full；Viewer/API/Agent 各真实支持入口 | 同原操作的可公开结果、受限/unknown、当前请求与旧结果 | 不从一种客户端外推全部协议或真人理解 |

## 12. 设计决策与实施剩余

采用各域原 writer、精确事实类型和独立结果对照，保持原显式付款/原子业务。这样无需新增全局兑换或资格协调状态，也能逐消费者核对“谁可以改变什么、依据是什么、失败后保留什么”。来源缺失、当前权限不足和原决定未知具有不同恢复路径，均不由另一侧的成功填补。

本设计的必要决定包括完整 C01–C14 接收、来源/费用边界、适用治理合取、合法组合与原子性、原请求/G1/版本恢复、有界消费和对应方法。实施仍需各实际 writer/reader 接线、真实 authority/来源与同候选 full 证据；没有同名 credit producer 时如实保留该实际缺项，不以新币或空壳回执代替。原产品要求、GCB/各业务政策、Phase 2 gate 和发行限制不因此改变。
