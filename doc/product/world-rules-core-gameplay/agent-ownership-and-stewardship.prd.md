# Agent 所有权与持续经营

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-12
- 专业域权威：[`Agent claim 经济合同`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)

本文承载玩家取得、维持和结束 Agent 控制权，以及 Agent 资产、团队扩张和经济连续性的长期产品承诺。它补充首局与持续游玩、间接控制和成熟世界成长分册，但不冻结经济数值、状态字段、身份实现、界面结构、运营操作、测试步骤或当前放行结论。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 直接承载控制权确认、失去控制后的连续性和重认领的产品承诺；独立 design 会重复 ownership authority。
- 历史设计判定记录：本次分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，本次闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
<a id="1-产品承诺"></a>
本节的产品体验设计豁免不等于专业系统设计不适用；各要求的技术覆盖与实现证据分别按下列状态摘要核对。

## 1. 产品目标与承诺

玩家通过经济认领取得 Agent 控制权时，必须作出明确、非零且可理解的承诺，并持续看见这份控制权的成本、义务、风险和下一次选择。首个支持路径可以降低进入门槛，但不会把经济认领变成免费、可套现或可无限复制的能力。

本分册的 upfront、upkeep 与认领资助承诺适用于形成经济认领的路径。首次注册或控制绑定须按其真实权威结果解释，不能仅因玩家已能操作 Agent 就宣称已承担经济认领的维护费用；也不能由当前没有经济认领推断历史义务已结清。首聊是否解锁与经济认领是否存在分别判断，具体边界由 [Agent claim 经济合同](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#first-chat-unlock-boundary) 维护。

玩家只能把已绑定或已拥有的 Agent 视为自己的可操作对象；共享世界中的其他 Agent 可以构成世界背景与机会，但不会被误导为当前玩家的控制权。

<a id="2-玩家边界与组合关系"></a>
## 2. 玩家体验边界与组合关系

- 在确认前，玩家能读懂取得控制权的主要投入、持续义务、候选差异或风险，以及继续、比较、等待或补足条件等下一步。
- 取得后，玩家能理解控制权是否有效、何时需要维持、可能因何种可读原因失去或主动结束，以及相应的恢复或重新选择路径。
- 首个支持路径只帮助受支持玩家完成首个明确承诺；它不产生自由可转移财富，不绕过后续容量、维护、反滥用或世界权威边界。
- Agent 所有权保持排他、可归因和可审计；玩家不能通过旁路、误认世界实体或静默自动提交获得控制权。
- 更高的 Agent 容量属于长期成长的受限选择，不是默认囤积权，也不替代区域贡献、专业化或世界治理的其他前置条件。

### 2.1 首个 Agent 认领承诺包

首个 Agent 认领是一次持续的世界参与承诺，而不是只回答“能否支付”的解锁。玩家确认前必须能用一个连贯的承诺包回答：

- 当前候选为何适合或不适合当前目标：其可理解的用途、与其他候选的主要差异，以及这项差异对第一个目标的帮助或风险。
- 此次确认的非零 upfront 成本，以及确认后能够支撑多久的 upkeep runway；持续义务不得被包装为一次性取得。
- 哪些可读条件会导致主动结束、进入风险状态或被回收，从而保留什么、失去什么；玩家必须能看见补足、恢复、重新选择或安全结束的下一步。
- 当前最好的替代决策：比较其他候选、等待一个可理解的条件，或先解决资金/世界 blocker；当没有安全替代时也必须明确说明，不能把立即确认伪装为唯一无代价选择。

受限的首个认领资助只帮助符合条件的玩家承担这次 `slot-1` 的非零 claim/upkeep 承诺，且不能转成自由财富、设施/材料库存、liquid starter OC 或持续补贴。它与完成 Agent 已存在后、为首次对话授予 liquid starter OC 的独立首聊解锁不同：后者不支付或延长 claim/upkeep，也不将认领变成免费或设施补贴。具体 bucket、资格、余额、回收计算和对话 gate 由专业域维护；跨模块来源与 sink 边界见[资源模型与跨模块 provenance](prd.md#resource-model-and-cross-module-provenance)。

### 2.2 结束、失去控制与不可套利续接

主动结束、到期、回收或其他失去控制权的结果必须是同一条持久世界因果链中的不同可读原因，而不是让玩家以更换候选、身份表面或短暂退出重置世界承诺的入口。玩家可以结束当前控制关系，也可以在专业合同允许的条件下走向恢复、重新选择或安全停止；但不能要求系统把结束解释为“从未认领”，不能把已消耗或受限的支持重新铸造成新的自由余额，也不能以结束来规避已确认的欠费、处罚、交付或争议处理。

结束或失去控制时，玩家必须能读到以下最小结果包：

- 发生的原因、其生效范围和当前控制权为何不再有效；正常主动结束不得被伪装为惩罚，惩罚、回收或争议中的限制也不得被伪装为玩家自愿放弃。
- 哪些世界结果、历史 receipt、已完成交付和未决义务仍持续，哪些仅随控制权停止；历史可审计不等同于仍可操作、仍有资格或可重复领取支持。
- 当前可用的恢复、重新选择、申诉或安全停止路径，以及尚未满足时的可读 blocker 和下一次可决策点。不存在安全恢复时，系统必须明确停止，不能显示会静默授予新 claim、重开首次支持或绕过结算的假动作。

为避免 churn 套利，任何重新取得控制权都必须按当时的权威资格、容量、资源、治理和反滥用规则重新评估；它不能仅因玩家曾经结束、曾拥有其他 Agent、曾使用首个支持或持有历史 receipt 而自动通过。世界可以保留玩家的历史、关系和已产生后果，从而让后续选择具有情境与涌现价值；但这些持久记录不能自动升级为控制权、免费 upkeep、额外容量、可转移财富或对其他玩家 Agent 的权限。

### 2.3 恢复选项的比较与降级边界

当同一受阻状态已由权威世界确定存在多个可用恢复路径时，玩家必须得到可比较的选项集，而非只有一个笼统的“可恢复”提示。每个已发布选项至少说明：路径种类、定性时间、定性资源投入、定性风险、仍能保留的主要收益，以及推荐该路径的原因。该信息帮助玩家在保留现有经营线、承受再投入和改变后续方向之间作出选择；它不是隐藏数值、成功保证或自动动作。

- 对当前已支持的 `repair`、`rebuild`、`pivot` 路径族，三条路径同时可用时必须分别发布，且使用稳定顺序；不得因推荐其中一条而静默删除其他可行路径。
- 每条选项的定性字段必须可读并能彼此区分。未知或新增枚举可用人类可读的降级标签表达，但不能把原始内部标识、空白字段或未解释的技术错误当作玩家决策信息。
- 若权威状态没有可用路径、只提供旧式摘要，或无法构成完整的可比较选项集，玩家必须看到“尚未发布/不可用”的真实状态或既有摘要，并继续看到当前 blocker 与下一次可决策点；客户端、Agent 建议或历史快照不得臆造缺失选项。旧式摘要可以保留权威来源原文（包括旧路径标签），但它只承担兼容展示，不属于结构化比较项、不能满足 AS-8；本节的可读降级标签要求只约束结构化比较项。
- 比较、推荐和显示本身不确认、排队、执行或保留任一路径。玩家仍须通过相应的权威动作与资格、资源、治理和反滥用校验；推荐失败、状态变化或重连后必须重新以当前权威事实评估。

这一边界保持 world-first：控制权变化以权威世界事实而非客户端叙述为准；保持 emergence-first：玩家可在同一规则下重建关系和策略，而不能复制补贴或权限；保持 persistent / auditable：结束、未决义务和新选择连续可追溯；保持 extensible：未来的恢复或争议机制可以增加受限路径，但必须满足同样的资格重评、非重置和可读结果包。

<a id="agent-asset-economy"></a>
### 2.4 Agent 资产、团队与经济连续性

Agent 的取得、维护、容量、转让条件与关联经济权利属于世界规则与玩法系统的产品主责。Agent 可以成为长期持有、扩张和转让的经营资产，但取得与维持团队必须承担显著的时间、资源、维护、授权和协调复杂度；它不是无成本的线性囤积权。自治、委托、授权重评与责任归因的消费边界见[Agent 自治、委托与责任连续性](../agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#agent-delegation-boundary)。

- 早期体验以一个可理解的主 Agent 为经营锚点；后续可以形成团队、设施角色和授权层级，但组织层级不能取消小规模玩家的独立路线、绕过区域/治理资格，或把成员数量自动转为额外世界权力。
- 额外 Agent 是可持有、可调度且可转让的长期经营资产；玩家或组织取得额外 Agent 的常态路径是向世界内工业能力下单制造，订单消耗显著资源、占用适用产能并经历可读交付时间。
- Agent 的供给主要受工业、物流、设施与交付约束，而不是由普通治理 quota 直接发放或扩张；适用资格与安全边界仍可限制订单，但不能把治理影响力伪装成无成本 Agent 供给。
- 受规则约束的控制、经济、经营角色和关联资产权益可以随转让条件发生变化；转让生效后的自治范围、旧授权失效和新目标取得由[智能体专题](../agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#agent-delegation-boundary)消费并继续定义，不能由经济转让本身推导出额外世界权力。

### 2.5 可验证的控制权与恢复要求

<a id="req-wr-aos-001"></a>
### REQ-WR-AOS-001：首个经济认领必须是可比较的非零承诺

Trace ID: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf

- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 系统设计依据：[gameplay-agent-claim-economy-contract.design / des-claim-economy-001](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-economy-001)。已承接：非零认领成本、资金来源与回收的玩法设计。
- 实现与验证依据：已交付非零 activation/bond/upkeep 的只读计算、slot 限制和资金来源拆分；不把此计算等同完整候选比较读面。 实现定位：[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/agent_claims.rs#L66-L118)；断言/历史样本：[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/tests/agent_claims.rs#L188-L240)。本轮只读源码和断言，未运行 Rust、正式入口或真人验收；历史样本仍以其原候选/环境为准。 其余实现交付 PR 与适用运行记录未逐条核定；[核对任务 #4698](https://github.com/eng-cc/oasis7/issues/4698)仅记录审查。
- 核对日期与基线：2026-10-15；需求、专业设计与静态源码基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；本轮无新运行环境或端到端验收结果。
- 剩余范围：尚未核清候选用途差异、upkeep runway 与替代路径的完整确认前读面及所有事实漂移的原子拒绝；已有成本计算不证明玩家理解或首个经济认领全链。

- 适用条件：当前取得 Agent 的路径将形成经济认领；单纯首次注册或控制绑定不自动满足这一条件。
- 要求：玩家在确认首个经济认领前必须能比较候选用途与差异、非零 upfront 成本、确认后的 upkeep runway、失去控制权的可读触发，以及等待、补足或选择其他候选的替代路径；预览或本地提交不能单独产生控制权、自由财富或持久义务。
- 专业权威：[`Agent claim 经济合同`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)
- 验收：[AC-WR-AOS-001](#ac-wr-aos-001)

<a id="ac-wr-aos-001"></a>
### AC-WR-AOS-001：确认前显示承诺包且不产生旁路效果

Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf；声明：[REQ-WR-AOS-001](#req-wr-aos-001)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-AOS-001
- 给定：玩家看到一个将形成经济认领、仍可比较的首个 Agent 候选，其授权、成本、upkeep、失去控制权触发和替代路径均已声明。
- 当：玩家尚未完成权威确认，或确认时任一前置条件已发生变化。
- 则：玩家能读到非零承诺、持续成本和下一步；系统保持待决或原子拒绝并返回当前 blocker，不产生控制权、自由余额、容量豁免或部分扣减。

<a id="req-wr-aos-002"></a>
### REQ-WR-AOS-002：失去控制权后的恢复不能重置世界承诺

Trace ID: O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已交付 upkeep/grace、主动释放、闲置回收的独立原因与 restricted/liquid 退款拆分。 实现定位：[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/world/agent_claims.rs#L89-L242)；断言/历史样本：[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/tests/agent_claims.rs#L328-L480)。本轮只读源码和断言，未运行 Rust、正式入口或真人验收；历史样本仍以其原候选/环境为准。 其余实现交付 PR 与适用运行记录未逐条核定；[核对任务 #4698](https://github.com/eng-cc/oasis7/issues/4698)仅记录审查。
- 核对日期与基线：2026-10-15；需求、专业设计与静态源码基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；本轮无新运行环境或端到端验收结果。
- 剩余范围：已核对 upkeep、grace、主动释放及闲置回收分支；争议限制、未决交付/处罚跨控制关系保全、重新认领反套利及申诉读面仍无完整审计。

- 要求：主动结束、到期、回收和争议限制必须区分原因与范围，保留已确认结果和未决义务，并只提供按当前资格重新评估的恢复、重新选择、申诉或安全停止路径。
- 专业权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)
- 验收：[AC-WR-AOS-002](#ac-wr-aos-002)

<a id="ac-wr-aos-002"></a>
### AC-WR-AOS-002：结束与重认领保持连续且不可套利

Trace refs: O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287；声明：[REQ-WR-AOS-002](#req-wr-aos-002)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-AOS-002
- 给定：一个已生效的 Agent 控制关系进入主动结束、到期、回收或争议限制之一。
- 当：玩家查看结果并尝试恢复或重新认领。
- 则：结果显示原因、范围、历史 receipt、未决义务和真实可用路径；新的控制权必须重新通过当前资格、资源、容量和反滥用校验，重复请求不能重放支持、权限、财富或世界效果。

<a id="req-wr-aos-003"></a>
### REQ-WR-AOS-003：团队扩张必须保留取得、维护与授权边界

Trace ID: O7-REQ-f247bdc7-f4ae-4893-a35e-6840fab7f465

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已交付按当前已持有数量计算 slot/cap、追加成本与持续 upkeep 的有限团队规模切片。 实现定位：[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/agent_claims.rs#L47-L118)、[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/world/agent_claims.rs#L12-L37)；断言/历史样本：[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/tests/agent_claims.rs#L277-L325)。本轮只读源码和断言，未运行 Rust、正式入口或真人验收；历史样本仍以其原候选/环境为准。 其余实现交付 PR 与适用运行记录未逐条核定；[核对任务 #4698](https://github.com/eng-cc/oasis7/issues/4698)仅记录审查。
- 核对日期与基线：2026-10-15；需求、专业设计与静态源码基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；本轮无新运行环境或端到端验收结果。
- 剩余范围：已有 slot 容量、非零追加成本和 upkeep 计算；工业制造取得、团队委托协调、小规模独立路线与不自动扩权的组合未核清。

- 要求：从一个主 Agent 扩张到团队时，每个额外 Agent 都必须经过当时有效的取得资格，并承担可读的取得、维护、授权和协调约束；团队成员数量不得自动放大自治范围、产能或世界权力，小规模独立路线必须继续可用。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)
- 验收：[AC-WR-AOS-003](#ac-wr-aos-003)

<a id="ac-wr-aos-003"></a>
### AC-WR-AOS-003：团队扩张不静默复制权力或自动产能

Trace refs: O7-REQ-f247bdc7-f4ae-4893-a35e-6840fab7f465；声明：[REQ-WR-AOS-003](#req-wr-aos-003)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-AOS-003
- 给定：玩家或组织从一个可操作的主 Agent 比较并申请一个或多个额外 Agent。
- 当：扩张前后分别检查取得、维护、授权、协调和独立经营路径。
- 则：每个额外 Agent 的取得与持续经营约束均可被追踪；团队扩张不静默扩大自治/责任范围、世界权力或自动产能，且玩家仍能选择小规模独立路线。Agent 专题的自治、委托与责任结果由其自身 authority 消费并验证，不由本条重复立法。

<a id="req-wr-aos-004"></a>
### REQ-WR-AOS-004：工业制造供给必须承担资源、产能与交付约束

Trace ID: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928

- 系统设计状态：部分覆盖
- 实现状态：未实现
- 系统设计依据：[indirect-control-agency-execution-and-continuation.design / des-wr-ia-010](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010)。已承接：经本条对应 AC 承接的工业供给授权边界或转让资格与历史连续性。
- 实现与验证依据：已补齐当前基线 Runtime 生产源码清单并沿[穷尽领域事件分派](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/state.rs#L1008-L1140)、[模块输出](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/module_runtime.rs#L156-L217)及[模块/回执消费边界](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/event_processing.rs#L801-L884)闭合允许的状态写入。[ScheduleRecipe 资格/槽位](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L435-L481)检查 requester、工厂 owner 和 recipe_slots，[工业资源与交付时点](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L750-L793)形成材料、电力与 ready_at；[工业完成输出物料](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L439-L503)只产出 material ledger 并保存工业 receipt，未产生新 Agent。[既有 Agent 的 ClaimAgent](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/event_processing/action_to_event_gameplay.rs#L144-L263)要求目标 Agent 预先存在，再经[claim publication 路由](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/event_processing/publication.rs#L155-L170)和[claim 原子状态安装](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/agent_claim_economic_publication.rs#L337-L350)落地费用、押金、upkeep 与 claim；[当前持有 claim 读模型](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/viewer/runtime_live/claim_snapshot.rs#L198-L255)消费当前持有与维护状态。上述链分别证明通用工业和既有 Agent 认领子范围，不证明额外 Agent 的工业制造交付；本轮仅静态核对，未运行验收。 [Agent身份构造](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/state/core_policy_transition.rs#L45-L56)仅承接 AgentRegistered，而[工业完成状态](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L439-L503)产出物料并更新既有执行者；当前权威 Runtime 未实现工业订单制造/交付额外 Agent 的合同。 前轮 `3c0a31a6` 已读取 claim quote、upkeep/grace/reclaim 与测试定义；对应业务 CI job 为 skipped 的既有限界保留，本轮无新增运行结果。
- 核对日期与基线：2026-10-17；需求/设计/静态实现固定 `6463975178f3d32ef6d8e19e14f0b24f1c7f9471`，引用源码逐文件 Git blob 校验。本轮只读核对，未运行测试、Runtime、模型或产品验收；已有测试正文不视为同候选运行结果。
- 剩余范围：待实现额外 Agent 的正式工业订单、资源/产能/交付身份与创建/控制权取得的同一权威链，并补齐 quota 非旁路、取得后维护/授权/协调持续义务、早期单主 Agent 与小规模路线组合验收。现有材料配方、RegisterAgent 和既有目标 ClaimAgent 不构成制造合同；snapshot/replay 恢复不是新制造 producer。 后续窄域开发安排见 [Ref #4850](https://github.com/nil-studio/oasis7/issues/4850)，仅追踪工业订单制造交付；本状态审查不关闭该任务。

- 要求：额外 Agent 的常态取得路径应通过世界内工业订单表达，并让玩家或组织理解资源投入、适用产能占用和可读交付时间；普通治理 quota 不得直接发放 Agent 或替代工业供给约束，取得后的维护、授权与协调义务仍持续。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)
- 验收：[AC-WR-AOS-004](#ac-wr-aos-004)

<a id="ac-wr-aos-004"></a>
### AC-WR-AOS-004：工业供给不以治理 quota 或无成本路径替代

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928；声明：[REQ-WR-AOS-004](#req-wr-aos-004)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-AOS-004
- 给定：玩家或组织为额外 Agent 提交一个世界内工业订单。
- 当：订单进入确认、生产、交付或失败/等待处置之一。
- 则：组合证据能读出资源、产能和交付约束，并证明交付后的维护、授权和协调义务仍在；普通治理 quota 不直接产生 Agent、不绕过工业供给，早期一个主 Agent 的经营锚点与小规模独立路线保持有效。Agent 专题的授权与责任结果只作为消费条件，不把工业订单变成新的委托权威。

<a id="req-wr-aos-005"></a>
### REQ-WR-AOS-005：经济转让必须重评资格并保留连续性

Trace ID: O7-REQ-42168149-c979-4211-9777-2d72f7050147

- 系统设计状态：部分覆盖
- 实现状态：未实现
- 系统设计依据：[indirect-control-agency-execution-and-continuation.design / des-wr-ia-011](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011)。已承接：经本条对应 AC 承接的工业供给授权边界或转让资格与历史连续性。
- 实现与验证依据：已补齐当前基线 Runtime 生产源码清单并沿[穷尽领域事件分派](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/state.rs#L1008-L1140)、[模块输出](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/module_runtime.rs#L156-L217)及[模块/回执消费边界](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/event_processing.rs#L801-L884)闭合允许的状态写入。[claim/release action 合同](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_agent_api/src/world_action/action.rs#L309-L322)及[claim API preflight 与提交](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/agent_claim_api.rs#L127-L184)只有既有目标认领/释放合同；[claim 准备与报价重验](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/agent_claim_economic_publication.rs#L91-L139)重验认领身份、容量与资金报价，经[claim publication 路由](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/event_processing/publication.rs#L155-L170)、[state/journal 安装与事件路由](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/event_processing/publication.rs#L414-L447)发布。[终态删除 claim、退款账本安装](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/agent_claim_terminal_publication.rs#L221-L234)删除旧 claim 并安装退款账本，没有目标新持有人；[普通经济合同资源/声誉结算](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/economic_contract_publication.rs#L180-L226)只结资源与声誉。[claim/upkeep 原子重试回放测试正文](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/tests/agent_claims/raw_claim_monetary_publication_transaction_regressions.rs#L65-L128)含 claim/upkeep 失败不变、重试和回放断言，但本轮未运行，也不证明 Agent 经济转让的五门槛与未决义务连续性。 [claim事件全部消费分支](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/state/apply_domain_event_gameplay_claims.rs#L9-L29)仅有创建/维护、轻生命周期、释放/回收；[新owner赋值](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/agent_claim_economic_publication.rs#L211-L247)只属于未认领目标首次claim，[释放终态](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/agent_claim_terminal_publication.rs#L221-L234)删除旧claim。当前权威 Runtime 未实现保留转让双方与未决义务的原子经济转让合同。 前轮 `3c0a31a6` 已读取 claim quote、upkeep/grace/reclaim 与测试定义；对应业务 CI job 为 skipped 的既有限界保留，本轮无新增运行结果。
- 核对日期与基线：2026-10-17；需求/设计/静态实现固定 `6463975178f3d32ef6d8e19e14f0b24f1c7f9471`，引用源码逐文件 Git blob 校验。本轮只读核对，未运行测试、Runtime、模型或产品验收；已有测试正文不视为同候选运行结果。
- 剩余范围：待实现正式经济转让请求/生效点与五门槛原子重评：资格、容量、资源、治理、反滥用；包含失败/过期/待决的 blocker/下一步、重试重放防旁路以及身份/来源/审计/既有结果/未决义务跨入口连续读模型。release 后另行 claim、资源合同、旧 grant 失效或重配置 ack 不替代该合同；snapshot/replay 只恢复既有事实。

- 要求：Agent 的经济转让只能在当时有效的资格、容量、资源、治理和反滥用条件下生效，并保留既有身份、来源、审计历史、已生效结果和未决义务；经济转让本身不得推导额外自治、委托或世界权力。
- 专业权威：[`Agent claim 经济合同`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)
- 验收：[AC-WR-AOS-005](#ac-wr-aos-005)

<a id="ac-wr-aos-005"></a>
### AC-WR-AOS-005：转让生效点区分经济权利与 Agent 委托权威

Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147；声明：[REQ-WR-AOS-005](#req-wr-aos-005)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-AOS-005
- 给定：一个 Agent 的经济转让请求带有转让人、目标持有人、适用范围和待决义务。
- 当：提交、重连重试、资格变化或转让生效点发生变化。
- 则：权威结果只有在以下五类当前门槛全部通过后才生效：资格、容量、资源、治理、反滥用。任一门槛失败、过期或待决时，结果必须标明对应门槛、原因类别、当前 blocker 和下一步；不得产生部分转让、部分经济效果或隐含权限，重连、重复提交和旧请求重放不能绕过重新校验。转让前后的身份、来源、审计历史、已生效结果和未决义务连续可追溯。自治范围、旧授权失效、新目标和责任归因由 Agent 专题消费/验证，不能由经济转让条款重复授予或删除。

## 3. 权威边界

| 层级 | 本产品分册拥有 | 下层专业域拥有 |
| --- | --- | --- |
| 玩家价值 | Agent 控制权是可理解、可承诺、可维持和可结束的世界参与能力 | `game` 拥有经济规则、玩法保证和专业验收 |
| 权威状态 | 所有权、成本和失去控制权的结果属于同一权威世界因果链 | `world-runtime` 拥有校验、记账、状态、审计和恢复合同 |
| 玩家表达 | 玩家能够识别自己的 Agent、报价、风险与下一步 | `world-simulator` 拥有身份、Agent、Viewer 与 API 表达实现 |
| 验证 | 组合证据证明玩家没有免费、误认或不可解释的控制权路径 | `testing` 与 QA 拥有测试矩阵、样本和当前 verdict |

专业合同以 [`gameplay-agent-claim-economy-contract.prd.md`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md) 为主入口。本分册不复制费用、倍率、余额来源、状态机、治理门禁、运营入口、字段或任务证据。

## 4. 组合验收

- AS-1：代表性首个控制权路径贯通可读报价、玩家明确确认、权威取得、玩家可见的有效控制权与下一步；首个路径不被包装成免费取得。
- AS-2：玩家能区分自己的可操作 Agent、未绑定 Agent 与其他玩家的 Agent；世界中存在其他实体不能替代自己的绑定或所有权证据。
- AS-3：代表性维持、主动结束或失去控制权路径能说明主要原因、保留或失去的结果，以及可用的恢复、重新选择或安全停止路径。
- AS-4：支持首个控制权的受限帮助不产生可转移财富、无限补贴、所有权旁路或对后续成长边界的豁免。
- AS-5：产品、game、runtime、Agent/Viewer 与 testing 指向同一候选和权威事实；局部 UI、运营记录或文档迁移不能单独证明产品承诺通过。
- AS-6：首个认领样例在确认前证明候选用途/差异、upfront 成本、确认后 upkeep runway、回收或失去触发、恢复/重新选择以及等待或替代动作；受限认领资助与 liquid starter OC 首聊解锁在同一因果链中仍可明确区分，且不产生免费认领或持续补贴含义。
- AS-7：主动结束、到期/回收和受争议限制的样例可区分原因与生效范围；每条路径保留历史 receipt 与未决义务的可审计连续性，并明确控制权停止后的恢复、重新选择、申诉或安全停止。重新取得控制权按当时资格重新评估，不能重置首个支持、逃避既有义务、取得额外容量或把历史记录转换为权限/财富。
- AS-8：代表性多路径恢复状态发布稳定、可区分的 `repair`、`rebuild`、`pivot` 比较项，每项含时间、资源、风险、保留收益和推荐原因；未知值可读降级，缺失/旧式资料不臆造选项，且查看或推荐不执行权威动作。
- AS-9：从一个主 Agent 扩张到团队的样例满足 [AC-WR-AOS-003](#ac-wr-aos-003)，额外 Agent 的取得、维护、授权和协调约束可追踪，自治/责任不会被静默放大，且小规模独立路线仍可继续；[智能体专题](../agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#agent-delegation-boundary)只消费其自治与责任结果。
- AS-10：额外 Agent 的世界内工业订单样例满足 [AC-WR-AOS-004](#ac-wr-aos-004)，资源、适用产能、交付时间、取得后的维护/授权/协调约束可追踪；普通治理 quota 不直接发放 Agent，且早期一个主 Agent 的经营锚点保持不变。
- AS-11：经济转让样例满足 [AC-WR-AOS-005](#ac-wr-aos-005)，资格、容量、资源、治理、反滥用五类门槛逐项可验证，且每类通过、失败、过期或待决结果均有 blocker 与下一步；转让生效点、身份/来源/历史/未决义务连续性可追踪，重试不能旁路或产生部分效果。[智能体专题](../agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#agent-delegation-boundary)只消费自治、旧授权失效、新目标和责任归因结果。

### 4.1 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| AS-1 / AS-3 | gameplay_designer / runtime_engineer / viewer_engineer | PRD-GAME-011 / PRD-WORLD_RUNTIME-001 | 报价、确认、持有、结束或回收的组合证据 | test_tier_required |
| AS-2 | agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-011 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | 当前玩家绑定/所有权与共享世界 Agent 可读边界 | test_tier_required |
| AS-4 | producer_system_designer / gameplay_designer / runtime_engineer / qa_engineer | PRD-GAME-011 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | 非免费、不可旁路和反滥用的权威与玩家可读证据 | test_tier_required |
| AS-5 | producer_system_designer / qa_engineer | PRD-GAME-011 / PRD-TESTING-003 | 同候选跨域组合审计 | test_tier_full |
| AS-6 | gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-011 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | 首个认领承诺包、持续成本、失去/恢复、替代决策与两类启动支持边界的组合证据 | test_tier_required |
| AS-7 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-011 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | 结束/回收/争议限制的原因和范围、历史与义务连续性、可读恢复/停止、以及重新认领资格重评和反 churn 套利的组合证据 | test_tier_full |
| AS-8 | producer_system_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | 权威恢复选项的完整发布、定性比较、未知/缺失降级与非执行边界 | test_tier_required |
| AS-9 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / qa_engineer | PRD-GAME-011 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-016 / PRD-TESTING-003 | 主 Agent 到团队的取得、维护、授权、协调约束；自治/责任不因团队扩张静默放大；小规模独立路线与 Agent 专题消费边界 | test_tier_required |
| AS-10 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / qa_engineer | PRD-GAME-011 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-016 / PRD-TESTING-003 | 工业订单的资源/产能/交付约束、治理 quota 非直接供给、取得后维护/授权/协调持续义务与早期单 Agent 锚点 | test_tier_full |
| AS-11 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-011 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-016 / PRD-TESTING-003 | 资格、容量、资源、治理、反滥用五门槛逐项重评；通过/失败/过期/待决 blocker、下一步、非部分生效与不可重放；转让生效点、经济连续性与身份/来源/历史/未决义务保留；自治、授权和责任由 Agent 专题消费 | test_tier_full |

## 5. Non-Goals

- 不冻结费用、倍率、宽限期、容量或回收参数。
- 不定义账本 bucket、资金 provenance、管理员权限、签名阈值、CLI 或运营 runbook。
- 不承诺免费 Agent、可转移启动财富、无限补贴、默认多 Agent 囤积或自动认领。
- 不把主动结束、短暂退出、候选切换或历史 receipt 视为重置首次支持、逃避既有义务、取得额外容量或转移控制权的机制。
- 不冻结恢复路径的数值成本、时长、成功率、排序算法、状态字段、API、界面布局或翻译字典，也不把比较卡或 Agent 推荐承诺为动作已提交、资源已保留或结果必然成功。
- 不以本文或历史任务状态声明当前 preview、可玩性或公开发行已经通过。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-WR-AOS-001](#req-wr-aos-001) / [AC-WR-AOS-001](#ac-wr-aos-001) | `producer_system_designer` | [`Agent claim 经济合同`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#public-claim-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-AOS-002](#req-wr-aos-002) / [AC-WR-AOS-002](#ac-wr-aos-002) | `producer_system_designer` | [`Agent claim 经济合同`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#public-claim-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-AOS-003](#req-wr-aos-003) / [AC-WR-AOS-003](#ac-wr-aos-003) | `producer_system_designer` / `gameplay_designer` / `agent_engineer` / `runtime_engineer` / `qa_engineer` | [`game PRD`](../../game/prd.md#3-player-facing-authority-boundary); [`world-runtime PRD`](../../world-runtime/prd.md); [`world-simulator PRD`](../../world-simulator/prd.md); [`testing PRD`](../../testing/prd.md) | 主 Agent 到团队的扩张样例；额外 Agent 的取得/维护/授权/协调约束、非静默权力与小规模独立路线；Agent 专题只消费自治/责任结果 | `test_tier_required` |
| [REQ-WR-AOS-004](#req-wr-aos-004) / [AC-WR-AOS-004](#ac-wr-aos-004) | `producer_system_designer` / `gameplay_designer` / `agent_engineer` / `runtime_engineer` / `qa_engineer` | [`game PRD`](../../game/prd.md#3-player-facing-authority-boundary); [`world-runtime PRD`](../../world-runtime/prd.md); [`world-simulator PRD`](../../world-simulator/prd.md); [`testing PRD`](../../testing/prd.md) | 世界内工业订单的资源/产能/交付、治理 quota 非直接供给、交付后的维护/授权/协调与早期单 Agent 锚点 | `test_tier_full` |
| [REQ-WR-AOS-005](#req-wr-aos-005) / [AC-WR-AOS-005](#ac-wr-aos-005) | `producer_system_designer` / `gameplay_designer` / `agent_engineer` / `runtime_engineer` / `viewer_engineer` / `qa_engineer` | [`Agent claim 经济合同`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#public-claim-boundary); [`world-runtime PRD`](../../world-runtime/prd.md); [`world-simulator PRD`](../../world-simulator/prd.md); [`testing PRD`](../../testing/prd.md) | 资格、容量、资源、治理、反滥用五门槛逐项重评；通过/失败/过期/待决 blocker、下一步、非部分生效与不可重放；转让生效点、身份、来源、审计历史、已生效结果和未决义务连续性；Agent 专题消费自治/授权/责任结果而不重复立法 | `test_tier_full` |

## 未决问题与证据边界

费用、维护/回收参数、团队容量、恢复成本与完整经济转让的专业设计和同候选证据须分别核对；本文的非零承诺与连续性要求不因这些缺口取消，也不因存在比较卡或历史任务而视为已实现。

核对责任由本文 Owner role 联合所列专业 owner 承担；解除触发是相应功能进入实现或对外 claim 评审前，按本条 REQ/AC 核定专业承接、固定候选与实际证据。未核清前保留上列缺口，不用结构检查推导完成。
