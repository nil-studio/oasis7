# 常态市场与有界紧急保供

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[prd.md](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-19
- 专业域权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)

- 配套专业系统设计：[紧急保供授权、冻结分配与结算](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-001)（DES-WR-ES-001..004；目标技术合同，不代表已实现）。

当多个地点的持续供电中断已使玩家和公共服务无法维持最低运行时，玩家需要知道自己是否属于保供范围、能分到多少、要等待多久，以及旧申请在恢复或续期后还有什么效力。普通涨价和单个工厂经营失败仍由市场与常态恢复解决。

本文确定采用 `emergency-supply-policy-v1` 作为首版目标，定义玩家选择、授权、介入、补偿和退出承诺。精确阈值、窗口、分配与资金参数的唯一声明为 [GAME-018 §6.4](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#emergency-supply-policy-v1)，技术字段与状态转换归配套系统设计。本次参数是提交评审的设计选择，不是既有 owner 批准、已启用配置或已验证平衡事实。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 的 §§2–5 内嵌玩家选择、等待/拒绝/恢复语义，不新增独立交互 surface 或布局；数值政策由 GAME-018 §6.4 单一声明。配套专业系统设计承接授权有效性、冻结批次、跨授权去重、物理结算与恢复；本项仅豁免独立产品交互 design，不豁免专业技术设计。
- 历史设计判定记录：本次分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，本次闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
本节的产品体验设计豁免不等于专业系统设计不适用；各要求的技术覆盖与实现证据分别按下列状态摘要核对。

## 1. 产品目标

市场常态由玩家、Agent、合约、供需和物理物流共同形成。价格波动、区域差异、局部故障和一般竞争是常态市场需要吸收的信号，不应被治理方随意改写为紧急状态。

只有预先定义的系统性必需品危机，才可进入有界紧急保供。干预的目的仅是维持受影响范围内的基本连续性；它不能创造永久价格控制、可套利资格、无偿没收或绕过世界物流与结算的治理旁路。

## 2. 常态市场与触发边界

- 常态价格、供给关系和竞争由市场、合约、物流与公开规则形成。日常限价、任意补贴或常态配给不能包装成普通治理便利。
- 单一玩家或设施故障、未经验证的私有情报、普通价格波动、暂时短缺或一般性供给竞争，均不足以触发紧急世界效果。
- 只有规则预先声明的系统性必需品危机，且存在可审计的事实证据时，才可提交紧急保供授权；“重要”“稀缺”或治理多数支持本身不能替代危机定义与证据。
- 授权生效前必须同时具备最小授权包：必需品类别、受影响范围、事实证据快照、授权来源、开始时间、到期时间、介入方式，以及复核与申诉入口。任一项缺失时维持常态市场，不能产生紧急采购、补偿、分配或 rationing 效果。

### 2.1 首版政策决定与玩家取舍

下表解释 [首版专业政策](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#emergency-supply-policy-v1) 的产品含义；实现不得另选阈值、窗口或分配规则。

| 决策 | 首版承诺与玩家影响 |
| --- | --- |
| 保什么 | 仅既有 `Electricity` 的已登记最低连续运行需求。公共关键供电/调运/存储服务与其他 Agent 的最低运行按声明优先级竞争；一般扩张、投机库存、Data 和 OC 不纳入。 |
| 何时进入 | 以危机前已有效的服务义务、真实库存和可达供给证明持续、跨地点、跨独立控制主体的缺口。价格、一次故障、客户端自报或临时增负载不能取得资格。阈值、观测长度和证据新鲜度采用专业政策表。 |
| 怎么分 | 固定世界 tick 窗口内先登记，关闭后共同冻结。最早消耗区间先满足，再按公共关键服务/其他最低运行优先级和剩余需求比例分配；余数按预先登记的稳定主体身份处理。首版不用抽签，不按谁先刷新/提交分配。 |
| 付什么与承担什么 | 受益人取得绑定最低消耗义务的受限电力，不能转售或换现金。供方自愿接受公开、有上限的采购价和交付责任；公共资金先锁定后结算。资金、路线或目的地不足时只承诺实际可履行量。 |
| 何时退出 | 到期直接结束新资格；持续恢复、证据过期或有权撤销也停止新分配。未出发承诺解除，在途按原合同和截止处置，已完成历史不改写。续期必须新证据、新授权及玩家显式新申请。 |
| 失败后怎么办 | 查看自己的未完成量及所占资源，选择补证、下一批申请、普通市场采购或申诉。网络超时先查原请求；到期后不自动重试、不延长额度，也不把申诉等同成功。 |

玩家可以选择立即走普通市场以承担市场价和运输风险，也可以等待保供批次、接受可能的部分份额和固定截止。受益需求已被任一渠道承诺或满足的部分不能再次领取；调整生产、暂停非必要负载或改用本地供给会减少真实缺口，但不会带来额外现金奖励。供方可以拒绝报价或在承诺前撤回供给；确认后承担其明确范围的交付和资金占用。

## 3. 有界介入规则

- 介入只能作用于授权包声明的品类、地点、主体范围和期限，并采用维持基本连续性所需的最小方式。范围外品类、地点、玩家和常态交易继续遵循普通市场规则。
- 允许的介入类别仅包括有授权的采购、补偿、分配或 rationing；首版全部采用 GAME-018 §6.4 的同一供给义务合同。采购锁物资和资金，分配/rationing 确认受限量，补偿仅结算该义务已完成的供货/运输价值，不能额外再发同值现金。
- 紧急资格不可转让、交易、叠加或转换为通用货币、永久优先权、治理权或其他世界资格；未使用资格随授权到期失效。
- 干预不得跳过物理路线、交付、目的地存储、合约结算、世界内资格或权威 receipt。发现、授权或分配决定不等同于货物已经到达或补偿已经最终结算。
- 不允许无偿没收、永久价格控制、任意扩大受益/负担范围，或以紧急状态覆盖正常申诉、恢复和审计程序。

### 3.1 稀缺分配与公平边界

当同一授权批次中有多个符合资格的采购、补偿、分配或 rationing 请求，而可用供给不足以同时满足时，权威处理必须先冻结该批次适用的**资格、需求、作用范围、资源与政策快照**，并按首版专业政策生成可复验的结果。实际控制关系下的账户聚合后再算份额；同一已登记服务义务拆成更多 request 或换授权不会增加需求。

- 同一冻结批次的 tie resolution 必须由可审计的权威规则决定，不能由未声明的到达顺序、提交顺序、管理员处理顺序或其他隐藏顺序决定。首版采用需求比例与稳定主体身份余数规则，不使用 lottery。未来政策若改为 lottery，必须提供权威 seed 和足以回放的 receipt 证明，并显式创建新政策/新授权；缺 seed 不能回退到客户端随机数或人工挑选。
- 在评估或提交时发现资格、需求、作用范围、资源或政策快照发生漂移，系统只能原子拒绝并给出重新报价/重新评估路径，或在当前快照下重新形成一个明确的新批次；不得部分扣减、静默回退到旧依据或让旧请求获得未声明的优先权。
- 已冻结批次不得因后续政策、危机标签、授权续期或管理决定而追溯重排。任何改变优先依据、范围或资源池的政策都必须形成新的授权/批次，并由玩家或 Agent 以新的显式请求进入，不能继承旧批次的排队位置、资格或补偿承诺。
- 玩家可读结果必须至少区分 `allocated`、`partial`、`denied` 与 `expired` 的语义类别（具体枚举由专业 authority 定义），并说明实际结果、适用的优先依据或拒绝/到期原因、作用范围、仍待决的部分和下一步。可读说明不要求暴露内部敏感字段，但不能用“处理中”“系统繁忙”或他人的资格替代自己的结果。
- 同一请求 lineage 的 retry、reconnect、重复投递或授权 renewal 不得产生第二次 world effect。新授权或新政策只能通过新的显式请求取得新的评估；已确认 receipt 保持原批次结果，不能被重放为第二次分配、补偿或资格。
- 需求累计必须跨授权、跨账户和跨批次保持：当前可申请量扣除已履行和仍在采购、预留、分配、在途、待结算的承诺；最终履行与释放互斥，旧承诺处置完成前不能释放同一份需求给新请求。制造订单已预留的电力不属于保供可用资源；组织接替、资产交接和执行者变更也不能复制需求或迁移旧临时资格。

## 4. 到期、续期与可审计退出

- 授权到期即自动回归常态市场；不得依赖额外管理员动作才停止紧急价格、采购、补偿、分配或 rationing 效果。
- 续期视为新的有界授权，必须提交新的事实证据快照、授权来源、范围和到期时间；原授权或危机标签不能自动滚动续期。
- 拒绝续期、提前撤销或申诉裁决生效后，未使用的临时资格立即失效，范围内后续交易恢复常态规则。
- 退出不删除已经发生的分配、补偿、异议和裁决。其授权、执行结果与 receipt 保持连续、可审计、可回放和可复核，但不能被重用为新的紧急资格。
- 紧急保供不能改写已确认的所有权、交付、结算或历史因果；纠错与补偿必须通过对应专业合同产生新的可审计结果。

### 4.1 待决请求不跨越授权边界

- 每个采购、补偿、分配或 rationing 请求都必须绑定它提交时的授权身份、适用范围和到期边界。请求已被登记、排队、在途或部分完成，不等于已取得交付、补偿或其他最终世界结果。
- 原授权到期、被撤销、被拒绝续期，或续期后品类、地点、主体范围或介入方式发生变化时，尚未由权威 receipt 确认的旧请求不得自动转入新授权继续执行、结算或获得优先级。未承诺请求终止，已承诺但未出发的部分解除占用；已在授权有效时合法出发的旧义务按原合同的地点、数量、价格、截止和 escrow 完成或退回，不取得新增资格。原授权失效不能阻断合法旧义务的有限结算，也不能给它增量或展期。
- 玩家可以查看旧请求所绑定的授权及其当前结果，并在新授权实际覆盖其需要时提交新的显式请求；新的请求必须保留与旧请求的可读关联，但不能隐式继承旧排队位置、临时资格、补偿承诺或世界效果。
- 已确认的 receipt 保持原授权下的历史结果；不因后续续期、撤销或范围变化被追溯改写。重复提交、重新连接或自动重试不得使同一需求在相邻授权中获得第二次世界效果。
- 延误、损失和目的地拒收按专业政策的保管等待、退回及损失承担规则处置；部分交付只确认实际接收部分。无法证明货物去向或资金责任时显示可审计阻塞，不能假装全部退款、补足或删除货物。保供供货/补偿义务结清前，相应占用不进入组织退出的剩余资产分配。

这项边界使紧急措施保持 world-first：是否继续由当前权威授权和 receipt 决定，而非客户端队列或历史标签；保持 emergence-first：玩家、Agent 与组织只能在当前共同规则下重新规划，不能从授权切换中取得套利路径；保持 persistent / auditable：旧请求、新请求、授权与结果之间的因果可追溯；保持 extensible：未来可增加介入类别或授权形态，但不得让待决状态跨越已失效或改变范围的授权。

<a id="5-玩家可读的保供状态与恢复路径"></a>
## 5. 玩家体验：可读的保供状态与恢复路径

紧急保供的制度状态不能只存在于治理记录或后台执行中。受影响玩家、组织和 Agent 必须能区分以下结果，并据此决定继续常态交易、等待、申诉或重新规划；具体文案、布局、状态字段和通知机制由 runtime 与正式玩家 surface 的专业 authority 拥有。

- **常态或待审**：危机线索、提案、收集中的证据或本地请求均尚未产生紧急世界效果。玩家仍按常态市场行动；若可行动，surface 只可说明可补充的证据、可用申诉入口或下一次复核点，不能把“正在审查”表示成已获配给、补偿或价格保护。
- **已授权、尚未完成**：玩家能读到适用必需品、地点/主体范围、授权来源、开始与到期边界，以及自己的请求或分配处于等待、在途、待结算或其他尚无最终世界结果的状态。授权、登记或排队不等于物资已交付、补偿已结算，亦不冻结范围外的常态交易。
- **分配结果类别**：对于存在供给竞争的请求，surface 必须把 `allocated`、`partial`、`denied` 与 `expired` 表达为彼此可区分的结果语义，并让玩家读到结果依据、实际作用范围、未完成部分（如有）、拒绝或到期原因和适用的等待、补证、申诉、常态替代或重新规划下一步；不能把旧批次的排队、他人的配给或未确认的预计数量当作自己的结果。
- **已确认结果**：只有权威 receipt 支持的交付、补偿、分配、拒绝或其他世界结果才可被表达为已完成。结果须关联其授权和适用范围，使玩家能核对已获得/承担的实际后果，而不是把预计数量、他人的资格或暂时 UI 状态当作自己的权利。
- **拒绝、撤销、到期或复核后变更**：玩家必须看到结果类别、仍适用的常态规则、未使用临时资格已失效的事实，以及在适用时可执行的申诉、补证、常态替代或重新规划路径。不得静默保留旧资格、自动重提已失效请求，或把历史授权记录当成新的紧急权利。
- **退出后的连续性**：回归常态不会抹去已发生的交付、补偿、异议或裁决。玩家可追溯其授权、receipt 和复核结果；若先前预期未获最终确认，surface 必须明确其未生效而非以回溯方式制造损失或债务。

### 5.1 可验证的市场状态与保供要求

<a id="req-wr-es-001"></a>
### REQ-WR-ES-001：常态市场与紧急保供必须按证据分界

Trace ID: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1

- 系统设计状态：已覆盖
- 实现状态：部分实现
- 系统设计依据：[DES-WR-ES-001](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-001)及其 §§6–10 的共同数据/接口、原子性、兼容和恢复合同；本条覆盖危机证据、normal/effective 覆盖分离、首版政策、完整授权包、权限/预算、新鲜度与期限逐提交校验。
- 实现与验证依据：普通材料市场报价、短缺拒绝和原生危机生命周期仍在；它们没有签发本合同紧急资格。固定代码路径与 producer/状态审读见[实现基线](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#es-implementation-baseline)；实施关联 [#4866](https://github.com/nil-studio/oasis7/issues/4866)，本次文档不关闭实现任务。
- 核对日期与基线：2026-10-19；源码 `nil-studio/oasis7@e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`，设计为本次 PR 工作树。范围为仓库默认/native 与随仓 builtin 的静态审读；未执行 Rust、正式玩家入口、跨节点或业务验收，不覆盖未知线上工件。
- 剩余范围：设计中的首版政策与接口已给出；实现仍需专用最低服务义务/证据 producer、授权命令与预算状态、原子校验和读面。政策平衡与实际效果尚未验证，不由“已覆盖”推导已上线。

- 要求：普通价格波动、局部故障和暂时短缺维持常态市场；只有预声明的系统性必需品危机具备证据、范围、授权来源、起止时间、介入方式和复核入口的最小授权包时，才可产生紧急世界效果。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)
- 验收：[AC-WR-ES-001](#ac-wr-es-001)

<a id="ac-wr-es-001"></a>
### AC-WR-ES-001：缺失授权包不改变常态交易

Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1；声明：[REQ-WR-ES-001](#req-wr-es-001)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-ES-001
- 给定：一个普通价格波动、局部设施故障或证据完整的系统性必需品危机。
- 当：系统评估是否启动紧急采购、补偿、分配或 rationing。
- 则：前两类继续按常态规则处理；危机只有满足首版连续观测、常态缺口、跨地点/主体、成熟期和新鲜度门槛，且最小授权包、预算来源及权限完整时才可介入，缺任一项则原子拒绝。使用保供承诺填平 effective_coverage 不得自行证明常态恢复。

<a id="req-wr-es-002"></a>
### REQ-WR-ES-002：稀缺分配必须依据冻结快照并可确定回放

Trace ID: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7

- 系统设计状态：已覆盖
- 实现状态：未实现
- 系统设计依据：[DES-WR-ES-002](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-002)及其 §§6–10 的共同数据/接口、原子性、兼容和恢复合同；本条覆盖固定窗口、完整不可变五类快照、按主体聚合的确定性分配、跨地点共用资源与需求/预算联合 CAS。
- 实现与验证依据：沿 typed action、事件、bootstrap、native/builtin producer、WorldState 与 publication 核对，尚无本合同批次/需求/分配及其 receipt；普通快照与原子底座不代签业务。固定代码路径与 producer/状态审读见[实现基线](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#es-implementation-baseline)；实施关联 [#4866](https://github.com/nil-studio/oasis7/issues/4866)，本次文档不关闭实现任务。
- 核对日期与基线：2026-10-19；源码 `nil-studio/oasis7@e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`，设计为本次 PR 工作树。范围为仓库默认/native 与随仓 builtin 的静态审读；未执行 Rust、正式玩家入口、跨节点或业务验收，不覆盖未知线上工件。
- 剩余范围：设计中的首版政策与接口已给出；实现仍需冻结批次与分配 producer、共享资源/资金/需求占用、同源 reducer 和专用重排/竞争验收。政策平衡与实际效果尚未验证，不由“已覆盖”推导已上线。

- 要求：同一授权批次必须冻结资格、需求、范围、资源和政策快照，使用已声明的优先依据与 tie resolution；快照漂移只能原子拒绝或形成新批次，不能以隐藏到达顺序追溯重排。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- 验收：[AC-WR-ES-002](#ac-wr-es-002)

<a id="ac-wr-es-002"></a>
### AC-WR-ES-002：重复投递不改变冻结批次结果

Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7；声明：[REQ-WR-ES-002](#req-wr-es-002)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-ES-002
- 给定：一个供给不足且已冻结快照的授权批次，包含多个等资格请求。
- 当：请求以不同到达顺序重复投递、重连或在快照漂移后重试。
- 则：按首版政策的聚合主体比例与确定性余数处理，等价重排序和回放得到同一 `allocated`/`partial`/`denied`/`expired` 结果；拆号、拆 request 不增份额，制造/市场/其他授权的资源与需求竞争经共同 CAS 裁决。漂移时整批原子拒绝或显式重评，retry/reconnect 不产生第二次分配、补偿或资格。

<a id="req-wr-es-003"></a>
### REQ-WR-ES-003：紧急授权到期后恢复常态且不迁移待决请求

Trace ID: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1

- 系统设计状态：已覆盖
- 实现状态：未实现
- 系统设计依据：[DES-WR-ES-003](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-003)及其 §§6–10 的共同数据/接口、原子性、兼容和恢复合同；本条覆盖到期/撤销/恢复关闭、未出发释放与已出发有限履约、跨授权累计需求、数量区间互斥、组织接替、申诉及恢复。
- 实现与验证依据：普通 Crisis 的 Active/Resolved/TimedOut 不记录本合同授权、待决请求、需求累计或退出处置；现有 snapshot/replay 未承载本业务状态。固定代码路径与 producer/状态审读见[实现基线](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#es-implementation-baseline)；实施关联 [#4866](https://github.com/nil-studio/oasis7/issues/4866)，本次文档不关闭实现任务。
- 核对日期与基线：2026-10-19；源码 `nil-studio/oasis7@e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`，设计为本次 PR 工作树。范围为仓库默认/native 与随仓 builtin 的静态审读；未执行 Rust、正式玩家入口、跨节点或业务验收，不覆盖未知线上工件。
- 剩余范围：设计中的首版政策与接口已给出；实现仍需授权/承诺生命周期、需求来源与累计账本、退回/损失/复核 adapter、持久恢复和历史/下一步投影。政策平衡与实际效果尚未验证，不由“已覆盖”推导已上线。

- 要求：紧急授权到期、撤销或范围改变时，未确认的请求不能自动跨入新授权；未使用资格失效，已确认 receipt 与历史结果保留，玩家可见常态替代、补证、申诉或重新规划路径。
- 专业权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)
- 验收：[AC-WR-ES-003](#ac-wr-es-003)

<a id="ac-wr-es-003"></a>
### AC-WR-ES-003：授权边界切换不制造第二次保供结果

Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1；声明：[REQ-WR-ES-003](#req-wr-es-003)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-ES-003
- 给定：一个旧授权下登记、排队或在途但未取得最终 receipt 的采购或配给请求。
- 当：授权到期、撤销或被不同范围的续期替代。
- 则：未承诺请求停止、未出发承诺解除，合法已出发义务仅按原数量/范围/价格/截止和锁款完成或退回；新请求不得继承旧优先级或资格，且扣除旧已履行及活跃承诺。释放与最终结算互斥，部分履行和组织接替不复制同一需求；市场回归常态，已确认历史仍可追溯。

<a id="req-wr-es-004"></a>
### REQ-WR-ES-004：紧急保供必须是有证据、有界且可退出的市场例外

Trace ID: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3

- 系统设计状态：已覆盖
- 实现状态：部分实现
- 系统设计依据：[DES-WR-ES-004](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-004)及其 §§6–10 的共同数据/接口、原子性、兼容和恢复合同；本条覆盖同一义务的采购/补偿/分配/rationing、真实资源/escrow 来源、物理接收/用途限制、部分结算及两维结果投影。
- 实现与验证依据：普通市场与材料运输仍校验库存、路线、到达及其原始结算；此窄子范围不证明保供专用 allocated/partial、补偿守恒或紧急用途约束。固定代码路径与 producer/状态审读见[实现基线](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#es-implementation-baseline)；实施关联 [#4866](https://github.com/nil-studio/oasis7/issues/4866)，本次文档不关闭实现任务。
- 核对日期与基线：2026-10-19；源码 `nil-studio/oasis7@e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`，设计为本次 PR 工作树。范围为仓库默认/native 与随仓 builtin 的静态审读；未执行 Rust、正式玩家入口、跨节点或业务验收，不覆盖未知线上工件。
- 剩余范围：设计中的首版政策与接口已给出；实现仍需与真实电力路径、存储/使用、OC escrow/预算的业务联合提交，以及 API/Agent/正式 Viewer、并发与恢复验证。政策平衡与实际效果尚未验证，不由“已覆盖”推导已上线。

- 要求：正常价格、供需、竞争和局部故障继续由市场、合约与物流形成；只有预声明的系统性必需品危机并具备事实证据、品类、受影响范围、授权来源、开始/到期时间、介入方式、补偿/公开分配、复核和申诉入口的最小授权包，才可产生紧急世界效果。介入不得变成常态限价、永久配给、任意没收、可转让/可叠加资格或绕过路线、到达、存储、合同结算、世界资格与 receipt 的旁路。
- 专业权威：[`PRD-GAME-018`](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-05)、[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- 验收：[AC-WR-ES-004](#ac-wr-es-004)

<a id="ac-wr-es-004"></a>
### AC-WR-ES-004：正常市场与紧急保供区分且退出后恢复常态

Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3；声明：[REQ-WR-ES-004](#req-wr-es-004)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-ES-004
- 给定：普通价格波动、局部故障、暂时短缺，以及一项具备完整授权包的系统性必需品危机和一项缺少证据、范围、期限、授权、复核或申诉任一项的对照样例。
- 当：市场评估是否启动、执行、续期、撤销或退出采购、补偿、分配或 rationing。
- 则：普通样例维持常态且不产生紧急效果；完整危机只在声明品类/地点/主体/期限内以最小介入生效，玩家能读到授权理由、补偿/公开分配、复核/申诉、自己的 `allocated`/`partial`/`denied`/`expired` 结果及下一步。缺项原子拒绝；到期、拒绝续期、撤销或复核后回归常态，未使用临时资格失效但已确认 receipt/历史保留，待决请求不跨授权，重复/重连/replay 不复制效果。

## 6. 范围与权威边界

产品层定义 `常态市场 -> 危机证据 -> 最小授权包 -> 有界介入 -> 到期/撤销/复核 -> 常态恢复` 的玩家与制度语义。

`game` 在 [GAME-018 §6.4](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#emergency-supply-policy-v1) 唯一定义首版危机、优先级、窗口、预算、补偿及退出规则；`world-runtime` 冻结权威状态、时序、物流、结算、资格、receipt 与恢复接口；`p2p` 拥有既有治理授权、签名和分布式状态边界；QA 拥有具体验证。产品采用该政策而不复制第二份数值权威。产品层不宣称紧急保供机制当前已实现、可用、平衡或适合公开发布。

## 7. 组合验收

- ES-1：普通价格波动、暂时短缺和局部设施故障样例维持常态市场，不产生紧急采购、补偿、分配或 rationing 效果。
- ES-2：有效干预样例可读出完整最小授权包，并证明范围外品类、地点、玩家和常态市场未受该授权影响；缺少任一必填项的样例原子拒绝且不产生紧急效果。
- ES-3：分配与补偿样例不产生可转让、可叠加或可兑换的套利资格，且不绕过物流、交付、目的地存储、结算、世界内资格或 receipt。
- ES-4：到期、拒绝续期、提前撤销和申诉复核样例均可确定性回放；退出后不残留常态限价、配给或未使用临时资格，同时保留已发生结果的审计连续性。
- ES-5：代表性受影响玩家样例能区分常态/待审、已授权但尚未完成、receipt 支持的已确认结果，以及拒绝、撤销、到期或复核后变更；每种非完成状态均不伪造交付、补偿或资格，并在适用时给出常态替代、补证、申诉或重新规划的下一步。
- ES-6：原授权到期、撤销或被不同范围的续期替代时，代表性待决采购、补偿、分配与 rationing 请求不会跨授权自动执行、结算、继承优先级或产生第二次效果；玩家只能查看原请求结果，并在新授权确实覆盖时以新的显式请求重新规划。
- ES-7：同一冻结资格、需求、作用范围、资源与政策快照下，代表性稀缺分配在等价重排序、重复投递和回放中保持同一结果；只有已声明的优先依据和 tie resolution authority 能影响结果，未声明的到达/提交/管理员顺序不能改变结果。采用 lottery 时，权威 seed 与 receipt 支持确定性回放。快照漂移原子拒绝或重新报价，政策变化不追溯重排已冻结批次；玩家能区分 `allocated`、`partial`、`denied`、`expired`，读到依据/原因与下一步，且 retry、reconnect、renewal 不产生第二次 world effect。
- ES-8：代表性正常市场/危机样例证明最小授权包、范围/期限、补偿/公开分配、复核/申诉和物理/结算边界同时成立；缺项维持常态或原子拒绝，退出自动恢复常态且不迁移待决请求、不残留临时资格或复制历史效果。

## 8. 验收追踪

| 产品承诺 | 专业 owner | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| ES-1 / ES-2 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 常态负例、完整/缺失授权包、作用范围与原子拒绝的组合证据 | test_tier_full |
| ES-3 | gameplay_designer / runtime_engineer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 紧急资格不可转让/兑换、物理交付、结算和 receipt 无旁路的组合证据 | test_tier_full |
| ES-4 | producer_system_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer | `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 到期/撤销/续期/申诉回放、常态恢复与历史审计连续性证据 | test_tier_full |
| ES-5 | producer_system_designer / runtime_engineer / viewer_engineer / gameplay_designer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 从常态/待审、授权未完成到 receipt 确认及拒绝/退出的状态区分；每个状态的无伪造结果、可用下一步和正式玩家 surface 可读性组合证据 | test_tier_full |
| ES-6 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / viewer_engineer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 授权到期、撤销及范围变化续期下的待决请求、旧/新请求关联、单次 world effect、receipt 不追溯改写与玩家可读恢复路径组合证据 | test_tier_full |
| ES-7 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / viewer_engineer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 冻结快照、声明优先依据、确定性 tie resolution、权威 lottery seed/receipt（如适用）、快照漂移拒绝/重新报价、批次不可追溯重排、四类分配结果可读性及 retry/reconnect/renewal 单次 world effect 的组合证据 | test_tier_full |
| ES-8 / [AC-WR-ES-004](#ac-wr-es-004) | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / viewer_engineer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 正常市场与系统性危机的证据包、范围/期限/补偿/公开分配/复核/申诉、物理交付/存储/结算不可旁路、退出恢复常态与待决/重复负例 | test_tier_full |

## 9. Non-Goals

- 不在产品层复制 GAME-018 §6.4 已冻结的精确政策值或为普通市场新增价格、税费、库存、损耗、路线和结算公式；本次专业政策不授予任意覆盖常态规则的权力。
- 不在产品层定义 wire 字段、receipt schema、资源 CAS 或 lottery seed 算法；首版明示确定分配，未来引入随机政策需完整新版本合同。
- 不实现市场撮合、物流、存储、治理授权、采购、补偿、分配、rationing、申诉或 runtime 状态机。
- 不允许紧急状态成为永久治理权、管理员裁量、无偿没收、选择性 bailout、通用货币或绕过世界恢复程序的入口。
- 不改变 OC、地方经济记录、区域 charter、土地 tenure 或玩家账户/身份的专业权威。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-WR-ES-001](#req-wr-es-001) / [AC-WR-ES-001](#ac-wr-es-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[DES-WR-ES-001](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-001) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-ES-002](#req-wr-es-002) / [AC-WR-ES-002](#ac-wr-es-002) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[DES-WR-ES-002](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-002) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-ES-003](#req-wr-es-003) / [AC-WR-ES-003](#ac-wr-es-003) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[DES-WR-ES-003](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-003) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-ES-004](#req-wr-es-004) / [AC-WR-ES-004](#ac-wr-es-004) | `producer_system_designer` | [`PRD-GAME-018`](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-05)、[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、[DES-WR-ES-004](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-004) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_full` |

## 未决问题与证据边界

首版政策和授权/批次/需求/结算/恢复合同已经在本次设计中给出，不再把关键阈值、优先级、窗口、补偿或在途退出留给实现者自行决定。需要验证的仍是参数平衡、输入 authority 的实际接通、实现兼容性、并发/恢复、正式玩家 surface 与真实网络；这些运行证据本次没有提供。

核对责任由本文 Owner role 联合所列专业 owner 承担；解除触发是相应功能进入实现或对外 claim 评审前，按本条 REQ/AC 核定专业承接、固定候选与实际证据。未核清前保留上列缺口，不用结构检查推导完成。
