# 常态市场与有界紧急保供

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[prd.md](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-09
- 专业域权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)

- 配套专业系统设计：[紧急保供授权、冻结分配与结算](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-001)（DES-WR-ES-001..004；目标技术合同，不代表已实现）。

本文定义常态市场与系统性必需品危机之间的产品状态边界，以及紧急保供的授权、介入和退出语义。它不定义危机阈值、价格、补偿、配给公式、物流算法、runtime 状态机或当前实现结论。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 只定义常态/紧急分界、冻结批次、结果类别和退出边界；分配算法与界面由专业 authority 决定。配套专业系统设计承接授权有效性、冻结批次、跨授权去重、物理结算与恢复；本项仅豁免独立产品交互 design，不豁免专业技术设计。
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

## 3. 有界介入规则

- 介入只能作用于授权包声明的品类、地点、主体范围和期限，并采用维持基本连续性所需的最小方式。范围外品类、地点、玩家和常态交易继续遵循普通市场规则。
- 允许的介入类别仅包括有授权的采购、补偿、分配或 rationing；具体数值、顺序和公式由专业域定义，产品层不冻结实现。
- 紧急资格不可转让、交易、叠加或转换为通用货币、永久优先权、治理权或其他世界资格；未使用资格随授权到期失效。
- 干预不得跳过物理路线、交付、目的地存储、合约结算、世界内资格或权威 receipt。发现、授权或分配决定不等同于货物已经到达或补偿已经最终结算。
- 不允许无偿没收、永久价格控制、任意扩大受益/负担范围，或以紧急状态覆盖正常申诉、恢复和审计程序。

### 3.1 稀缺分配与公平边界

当同一授权批次中有多个符合资格的采购、补偿、分配或 rationing 请求，而可用供给不足以同时满足时，权威处理必须先冻结该批次适用的**资格、需求、作用范围、资源与政策快照**，并依据已声明的优先依据生成可复验的结果。产品层不冻结优先权重、配额或分配公式，但专业域必须明确该依据适用于哪个批次、何时生效以及如何处理变更。

- 同一冻结批次的 tie resolution 必须由可审计的权威规则决定，不能由未声明的到达顺序、提交顺序、管理员处理顺序或其他隐藏顺序决定。若采用 lottery，随机性必须来自权威 seed，并由 receipt 保留足以回放和解释该结果的证明；不得以客户端随机数、界面先后或人工挑选替代。
- 在评估或提交时发现资格、需求、作用范围、资源或政策快照发生漂移，系统只能原子拒绝并给出重新报价/重新评估路径，或在当前快照下重新形成一个明确的新批次；不得部分扣减、静默回退到旧依据或让旧请求获得未声明的优先权。
- 已冻结批次不得因后续政策、危机标签、授权续期或管理决定而追溯重排。任何改变优先依据、范围或资源池的政策都必须形成新的授权/批次，并由玩家或 Agent 以新的显式请求进入，不能继承旧批次的排队位置、资格或补偿承诺。
- 玩家可读结果必须至少区分 `allocated`、`partial`、`denied` 与 `expired` 的语义类别（具体枚举由专业 authority 定义），并说明实际结果、适用的优先依据或拒绝/到期原因、作用范围、仍待决的部分和下一步。可读说明不要求暴露内部敏感字段，但不能用“处理中”“系统繁忙”或他人的资格替代自己的结果。
- 同一请求 lineage 的 retry、reconnect、重复投递或授权 renewal 不得产生第二次 world effect。新授权或新政策只能通过新的显式请求取得新的评估；已确认 receipt 保持原批次结果，不能被重放为第二次分配、补偿或资格。

## 4. 到期、续期与可审计退出

- 授权到期即自动回归常态市场；不得依赖额外管理员动作才停止紧急价格、采购、补偿、分配或 rationing 效果。
- 续期视为新的有界授权，必须提交新的事实证据快照、授权来源、范围和到期时间；原授权或危机标签不能自动滚动续期。
- 拒绝续期、提前撤销或申诉裁决生效后，未使用的临时资格立即失效，范围内后续交易恢复常态规则。
- 退出不删除已经发生的分配、补偿、异议和裁决。其授权、执行结果与 receipt 保持连续、可审计、可回放和可复核，但不能被重用为新的紧急资格。
- 紧急保供不能改写已确认的所有权、交付、结算或历史因果；纠错与补偿必须通过对应专业合同产生新的可审计结果。

### 4.1 待决请求不跨越授权边界

- 每个采购、补偿、分配或 rationing 请求都必须绑定它提交时的授权身份、适用范围和到期边界。请求已被登记、排队、在途或部分完成，不等于已取得交付、补偿或其他最终世界结果。
- 原授权到期、被撤销、被拒绝续期，或续期后品类、地点、主体范围或介入方式发生变化时，尚未由权威 receipt 确认的旧请求不得自动转入新授权继续执行、结算或获得优先级。它们只能按原授权仍有效时的专业合同完成、明确拒绝或保持待决；原授权已不支持继续时，必须停止在未完成状态。
- 玩家可以查看旧请求所绑定的授权及其当前结果，并在新授权实际覆盖其需要时提交新的显式请求；新的请求必须保留与旧请求的可读关联，但不能隐式继承旧排队位置、临时资格、补偿承诺或世界效果。
- 已确认的 receipt 保持原授权下的历史结果；不因后续续期、撤销或范围变化被追溯改写。重复提交、重新连接或自动重试不得使同一需求在相邻授权中获得第二次世界效果。

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

- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 系统设计依据：[emergency-supply-authorization-and-settlement.design / des-wr-es-001](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-001)。已承接：授权、冻结分配、退出或物理结算中本条对应的 runtime 合同与验证场景。
- 实现与验证依据：常态材料短缺仍按库存/税/运输损耗报价，库存不足拒绝排程，读面提供reduce_or_source_materials，而非授予紧急资格；这是本条常态市场直接子范围。最小授权包→专用紧急效果尚未交付；普通CrisisSpawn/Timeout不是该授权包。 [market](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy_support.rs#L318-L372)；[recipe](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L675-L792)；[quote](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/market_quote_decision_preview.rs#L29-L104)；[quote-read](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/viewer/runtime_live/market_quote_decision.rs#L25-L161)；[lifecycle](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/gameplay_loop.rs#L25-L237)；[crisis-state](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/gameplay_state.rs#L188-L216)。当前仓库默认/native及随仓builtin路径的只读静态审读；未执行Rust、正式玩家入口、跨节点或业务验收，测试源码不记为PASS。
- 核对日期与基线：2026-10-17；需求、专业设计和实现固定main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，引用文件逐Git blob校验；未引入未合PR #4865；当前仓库默认/native及随仓builtin路径的只读静态审读；未执行Rust、正式玩家入口、跨节点或业务验收，测试源码不记为PASS。
- 剩余范围：仍需交付紧急授权身份、完整包校验、范围/期限逐提交复核、缺项原子拒绝和专用结果读面；危机阈值/必需品/授权政策由owner明确。验证普通波动与局部故障对照和完整/缺项危机组合，不能以无紧急producer证明例外已安全生效。 实施待安排： [Refs #4866](https://github.com/nil-studio/oasis7/issues/4866)；本状态审查不关闭该任务。

- 要求：普通价格波动、局部故障和暂时短缺维持常态市场；只有预声明的系统性必需品危机具备证据、范围、授权来源、起止时间、介入方式和复核入口的最小授权包时，才可产生紧急世界效果。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)
- 验收：[AC-WR-ES-001](#ac-wr-es-001)

<a id="ac-wr-es-001"></a>
### AC-WR-ES-001：缺失授权包不改变常态交易

Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1；声明：[REQ-WR-ES-001](#req-wr-es-001)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-ES-001
- 给定：一个普通价格波动、局部设施故障或证据完整的系统性必需品危机。
- 当：系统评估是否启动紧急采购、补偿、分配或 rationing。
- 则：前两类继续按常态规则处理；危机只有在最小授权包完整且经权威确认后才可在声明范围内介入，缺任一项则原子拒绝且不产生部分效果。

<a id="req-wr-es-002"></a>
### REQ-WR-ES-002：稀缺分配必须依据冻结快照并可确定回放

Trace ID: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7

- 系统设计状态：部分覆盖
- 实现状态：未实现
- 系统设计依据：[emergency-supply-authorization-and-settlement.design / des-wr-es-002](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-002)。已承接：授权、冻结分配、退出或物理结算中本条对应的 runtime 合同与验证场景。
- 实现与验证依据：已沿冻结批次的适用前提核对typed入口、native/builtin生成器、持久状态和publication：当前交付没有授权批次、冻结资格/需求/资源/政策集合、分配向量及其receipt producer。普通市场报价与通用snapshot/原子publication不能充当冻结保供批次。 [action](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_agent_api/src/world_action/action.rs#L9-L558)；[event](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/events/domain_event.rs#L13-L936)；[bootstrap](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/bootstrap_gameplay.rs#L15-L100)；[lifecycle](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/gameplay_loop.rs#L25-L237)；[native](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/gameplay_loop.rs#L283-L505)；[builtin-crisis](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_builtin_wasm_modules/m5_gameplay_crisis_cycle/src/lib.rs#L20-L92)；[builtin-economic](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_builtin_wasm_modules/m5_gameplay_economic_overlay/src/lib.rs#L20-L100)；[state](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/state.rs#L630-L674)；[publication](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/event_processing/publication.rs#L328-L440)；[es-design](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/doc/world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#L50-L143)。当前仓库默认/native及随仓builtin路径的只读静态审读；未执行Rust、正式玩家入口、跨节点或业务验收，测试源码不记为PASS。
- 核对日期与基线：2026-10-17；需求、专业设计和实现固定main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，引用文件逐Git blob校验；未引入未合PR #4865；当前仓库默认/native及随仓builtin路径的只读静态审读；未执行Rust、正式玩家入口、跨节点或业务验收，测试源码不记为PASS。
- 剩余范围：实现已明确的冻结集合、批次身份、条件提交、分配结果和重放/去重合同；owner另行冻结优先规则、tie resolution、seed证明及窗口。需同候选乱序/重复/漂移/竞争的资源与receipt对账。 实施待安排： [Refs #4866](https://github.com/nil-studio/oasis7/issues/4866)；本状态审查不关闭该任务。

- 要求：同一授权批次必须冻结资格、需求、范围、资源和政策快照，使用已声明的优先依据与 tie resolution；快照漂移只能原子拒绝或形成新批次，不能以隐藏到达顺序追溯重排。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- 验收：[AC-WR-ES-002](#ac-wr-es-002)

<a id="ac-wr-es-002"></a>
### AC-WR-ES-002：重复投递不改变冻结批次结果

Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7；声明：[REQ-WR-ES-002](#req-wr-es-002)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-ES-002
- 给定：一个供给不足且已冻结快照的授权批次，包含多个等资格请求。
- 当：请求以不同到达顺序重复投递、重连或在快照漂移后重试。
- 则：等价重排序和回放得到同一 `allocated`/`partial`/`denied`/`expired` 语义结果；漂移时旧请求原子拒绝或重新评估，retry/reconnect 不产生第二次分配、补偿或资格。

<a id="req-wr-es-003"></a>
### REQ-WR-ES-003：紧急授权到期后恢复常态且不迁移待决请求

Trace ID: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1

- 系统设计状态：部分覆盖
- 实现状态：未实现
- 系统设计依据：[emergency-supply-authorization-and-settlement.design / des-wr-es-003](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-003)。已承接：授权、冻结分配、退出或物理结算中本条对应的 runtime 合同与验证场景。
- 实现与验证依据：当前普通crisis仅Active/Resolved/TimedOut，并未承载紧急授权、待决保供请求、需求lineage、预留/履行量；native/builtin超时惩罚和通用grant到期不执行本条授权边界切换。不存在可回读/恢复的本合同业务状态，不能把普通日志历史当作保供历史已交付。 [crisis-state](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/gameplay_state.rs#L188-L216)；[native](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/gameplay_loop.rs#L283-L505)；[builtin-crisis](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_builtin_wasm_modules/m5_gameplay_crisis_cycle/src/lib.rs#L20-L92)；[state](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/state.rs#L630-L674)；[action](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_agent_api/src/world_action/action.rs#L9-L558)；[event](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/events/domain_event.rs#L13-L936)；[snapshot](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/persistence.rs#L306-L380)；[restore](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/persistence.rs#L735-L785)；[es-design](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/doc/world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#L50-L143)。当前仓库默认/native及随仓builtin路径的只读静态审读；未执行Rust、正式玩家入口、跨节点或业务验收，测试源码不记为PASS。
- 核对日期与基线：2026-10-17；需求、专业设计和实现固定main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，引用文件逐Git blob校验；未引入未合PR #4865；当前仓库默认/native及随仓builtin路径的只读静态审读；未执行Rust、正式玩家入口、跨节点或业务验收，测试源码不记为PASS。
- 剩余范围：交付紧急授权/请求状态、失效谓词、旧请求不迁移、跨授权需求累计与预留释放/最终结算竞态合同，以及历史/下一步读面；在途处置政策另行确认。验收边界tick、撤销/续期、部分完成、重启/重连/回放。 实施待安排： [Refs #4866](https://github.com/nil-studio/oasis7/issues/4866)；本状态审查不关闭该任务。

- 要求：紧急授权到期、撤销或范围改变时，未确认的请求不能自动跨入新授权；未使用资格失效，已确认 receipt 与历史结果保留，玩家可见常态替代、补证、申诉或重新规划路径。
- 专业权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)
- 验收：[AC-WR-ES-003](#ac-wr-es-003)

<a id="ac-wr-es-003"></a>
### AC-WR-ES-003：授权边界切换不制造第二次保供结果

Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1；声明：[REQ-WR-ES-003](#req-wr-es-003)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-ES-003
- 给定：一个旧授权下登记、排队或在途但未取得最终 receipt 的采购或配给请求。
- 当：授权到期、撤销或被不同范围的续期替代。
- 则：请求保持原授权结果、明确拒绝或由玩家显式新提；不能继承旧优先级、资格、补偿承诺或世界效果，市场回归常态，已确认历史仍可追溯。

<a id="req-wr-es-004"></a>
### REQ-WR-ES-004：紧急保供必须是有证据、有界且可退出的市场例外

Trace ID: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3

- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 系统设计依据：[emergency-supply-authorization-and-settlement.design / des-wr-es-004](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-004)。已承接：授权、冻结分配、退出或物理结算中本条对应的 runtime 合同与验证场景。
- 实现与验证依据：正常材料短缺/价格成本由库存、税及运输损耗报价，排程与运输仍检查材料、路线可用/容量，MaterialTransit到达复核ready_at并产生物流结算记录；这是原REQ正常市场及物流的直接窄子范围。未将普通物流receipt冒称紧急allocated/partial，也未证明紧急介入不可旁路。 [market](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy_support.rs#L318-L372)；[recipe](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L675-L792)；[logistics](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/logistics.rs#L197-L280)；[transit](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L20-L155)；[arrival](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L151-L350)；[dispatch](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/event_processing/publication.rs#L160-L220)；[publication](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/event_processing/publication.rs#L328-L440)；[quote-read](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/viewer/runtime_live/market_quote_decision.rs#L25-L161)；[es-design](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/doc/world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#L50-L143)。当前仓库默认/native及随仓builtin路径的只读静态审读；未执行Rust、正式玩家入口、跨节点或业务验收，测试源码不记为PASS。
- 核对日期与基线：2026-10-17；需求、专业设计和实现固定main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，引用文件逐Git blob校验；未引入未合PR #4865；当前仓库默认/native及随仓builtin路径的只读静态审读；未执行Rust、正式玩家入口、跨节点或业务验收，测试源码不记为PASS。
- 剩余范围：实现紧急授权、品类/地点/主体/期限限制、补偿守恒与资格约束、退出和待决隔离、专用结果投影；补齐与真实物流/存储/合同结算的联合条件，而非新增旁路。政策由owner冻结；需完整ES-004组合与正式surface验证。 实施待安排： [Refs #4866](https://github.com/nil-studio/oasis7/issues/4866)；本状态审查不关闭该任务。

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

`game` 拥有危机、市场和经济平衡，包括阈值、价格、补偿及配给数值；`world-runtime` 拥有权威状态、时序、物流、结算、资格和 receipt；`p2p` 拥有治理授权、签名和分布式状态边界；QA 拥有具体验证。产品层不宣称紧急保供机制当前已实现、可用、平衡或适合公开发布。

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

- 不规定危机阈值、必需品清单、价格、采购量、补偿、配给、税费、库存、损耗、路线或结算公式。
- 产品层只约束冻结快照、声明优先依据、可回放的 tie resolution、非隐式顺序、批次不可追溯重排、结果可读性与单次 world effect；不规定优先权重、配额、分配公式、lottery 算法、seed 生成实现或具体 receipt 字段。
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

危机阈值、优先规则、冻结窗口与 seed 来源仍由玩法/治理 authority 确定，容量、wire 兼容与保留窗口由 runtime/QA 核对；已有系统设计不证明保供已实现或上线。

核对责任由本文 Owner role 联合所列专业 owner 承担；解除触发是相应功能进入实现或对外 claim 评审前，按本条 REQ/AC 核定专业承接、固定候选与实际证据。未核清前保留上列缺口，不用结构检查推导完成。
