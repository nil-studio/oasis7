# 世界规则与玩法系统 PRD

## 文档身份

- 产品模块：世界规则与玩法系统
- 产品模块 slug：`world-rules-core-gameplay`
- 产品层唯一 PRD：`doc/product/world-rules-core-gameplay/prd.md`
- 产品模块总入口：`doc/product/README.md`
- Product PRD-ID：`PRD-PRODUCT-001`
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：`2026-10-15`
- 后继文档：`无`
- 下层专业域：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)

本文只定义玩家目标、间接能动性、核心循环、成长与资源压力的产品承诺。玩法规则、数值平衡、专题 PRD-ID 与测试证据由 `game` 专业域拥有。

### 活跃产品专题

- [`首局与持续游玩`](first-session-and-continuation.prd.md)：首局微循环、后引导承接、首次持续能力、中循环展开与失败恢复。
- [`间接控制下的玩家能动性与续接`](indirect-control-agency-and-continuation.prd.md)：玩家通过 Agent 推动世界时的意图可读、因果可解释、干预重排、记忆纠正与回流续接。
- [`Agent 所有权与持续经营`](agent-ownership-and-stewardship.prd.md)：玩家以明确承诺取得、维持或结束自己的 Agent 控制权，并读懂成本、风险和恢复选择。
- [`成熟世界成长与区域参与`](mature-world-progression.prd.md)：首次持续能力之后的独立成长、区域专业化、有限影响、anti-grind / 恢复，以及有界认可与通用机会申领边界。
- [`受治理的区域能力与扩展`](governed-regional-capabilities-and-extensions.prd.md)：区域 charter 成立/退化、地点 tenure、公共融资和通行保护；区域设施从条件报价到有限服务、维护/耗尽/退役，以及创作者扩展从治理提案到权威生效、审计与恢复的产品边界。
- [`区域冲突、软赛季与可恢复损失`](chartered-conflict-soft-seasons-and-recovery.prd.md)：宣战、有限参战范围、实体战利品/占领、可恢复重建和不重置世界的软赛季边界。
- [`沟通、合同、声誉与 R&D 连续性`](communication-contracts-reputation-and-rd-continuity.prd.md)：人类沟通与 Agent 合同的边界、持续服务争端、情境声誉及研究归因/份额的长期产品语义。
- [`组织连续性、解散与长期不活跃保护`](organization-continuity-dissolution-and-dormancy-protection.prd.md)：组织 charter 的个人保护底线、可审计解散顺序，以及长期不活跃时的保护、恢复主张与有限处置边界。
- [`常态市场与有界紧急保供`](market-normal-state-and-emergency-supply.prd.md)：常态价格形成、系统性必需品危机的最小授权包、受限干预和可审计退出边界。
- [`工业需求目标与生产交付结算`](industrial-demand-goals-and-settlement.prd.md)：需求目标的提交前比较、生产与匹配交付结算、缺口/盈余处置、补产修订和重复执行边界。
- [`普通共同决策与宪制边界`](governed-common-decisions-and-constitutional-boundaries.prd.md)：普通政策、公共财库与 charter 运行事项的有限白名单，以及玩家保护、安全权威、宪制轨道和紧急防绕过边界。
- [`Frontier 扩展与世界信息边界`](frontier-expansion-and-world-information-boundaries.prd.md)：相邻探索、物流与 charter 共同形成的扩展边界，非主权 pioneer priority，以及公共事实、探索数据与实时运营信息的分层可见性。
- 战争/治理的玩家结果、可读预览与反支配边界由本 PRD 承诺；成本、收益、冷却和评分的专业数值权威见 [`gameplay-war-politics-mvp-baseline.design.md`](../../game/gameplay/gameplay-war-politics-mvp-baseline.design.md)。战争不表示已成为当前首局主线。
- [`可玩性证据与承诺边界`](playability-evidence-and-claim-boundaries.prd.md)：玩家杠杆、继续游玩价值与分层证据如何共同约束当前产品结论。

## 1. 产品承诺

<!-- source-first provenance: doc/game/design.md#3-设计结构:20-34 is module navigation and integration context only; this section remains the canonical product-promises authority. PE-001..003 are owned by playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001 through #req-wr-pe-003. -->

玩家通过可读、有代价、有反馈的行动持续影响同一个持久世界，并在权威规则内与其他玩家、Agent 和区域系统产生可审计的涌现结果。产品不设置全体玩家共享的胜利或通关终局；玩家持续完成有边界、可归因的阶段成果，并在同一世界中形成新的能力、区域价值或下一阶段方向。

**世界前提与玩家位置。** oasis7 是一款 AI Agent 文明模拟游戏，舞台位于破碎的小行星带；玩家从文明之外设定方向，Agent 在持续世界中作出选择并留下可追溯的历史后果。本定义只界定题材与玩家位置，不规定具体机制或实现状态，也不代表当前发行可用性。Agent/provider、世界模拟与进入世界后的交互产品组合见[智能体、世界模拟与交互 PRD](../agents-world-simulation/prd.md)；世界规则、玩家目标与核心玩法承诺仍由本 PRD 拥有。

长期推荐围绕三条相连但不强制线性的抱负轴展开：建立并守住可恢复的能力、用该能力服务区域需求、获得有限且可审计的区域影响。组织、协议或治理等文明尺度项目只作为玩家自愿进入的共同扩展，不能取代独立成长或成为唯一有效路线。

### 分层可进入性与持续世界节奏

当前交付目标是受控真实玩家在同一持久世界通过有界授权 Agent 获得首产物、理解可恢复阻塞、保留成果并回访续接。全局 P0 只包括阻断所选首局—持续能力—回访路径的缺陷，破坏世界身份、确认历史、合法资产、授权或幂等连续性的缺陷，以及使这些结果无法实际验证或被玩家理解的必要缺口。专题内部 P0 表达专题启动后的次序，不自动成为首次受控试玩前置。

产品面向愿意逐步承担系统深度的玩家，而不是只面向能够从首局同时掌握工业、组织、外交和治理全貌的玩家。正式体验先把当前世界状态收成一个可理解的目标、主要阻塞和下一步；玩家可以在准备好时自主展开区域专业化、市场/物流、组织、外交和治理。渐进披露只能重排与解释复杂性，不能隐藏会改变当前损失、锁定、权利、风险或恢复路径的真实取舍，也不能把深层系统削成没有后果的模板或纯自动化。

持续世界的常态节奏是混合的：日常短命令与结果复盘足以维持普通目标，已授权且有边界的 Agent 工作包可在玩家离开时推进；深度建设、外交、区域项目和其他自愿共同扩展可以支持较长会话。高风险竞争或其他需要回应的事项必须给出有界、可预期的窗口与授权/恢复路径，不得把持续在线、在线时长或无止境值守变成取得基本成长、独立恢复或资格的前提。

## 2. 范围与玩家边界

覆盖首局目标、micro-loop、后引导承接、间接控制、资源压力与长期参与。玩家可以观察、决策、行动并处理反馈；不能越过资源、时间、权限、治理或反滥用边界直接改写世界。

### 物理尺度、间接控制与未来候选

玩家影响的是一个有物理尺度且持续存在的世界，但当前默认体验是通过目标、Agent、地点、设施、配方和治理等间接动作推进，而不是第一人称逐块编辑。表现层可以为可读性抽象或夸张，但不得把它呈现为世界物理真值。

当玩家提出当前未开放的过细动作时，产品体验必须给出可执行的 canonical 替代动作；没有安全替代时，必须说明边界和下一次可决策点，而不是伪造动作或只留下无解释的失败。具身或 block-editing 仅是未来候选：只有在强化本模块的间接控制主路线、具备对应专业域合同与验证，并经显式跨域决策后才可进入原型。

canonical 替代不是对原请求的静默自动执行。存在一个或多个安全候选时，每个已发布候选至少说明目标与作用范围、主要成本、主要风险、预计后果、是否仍可撤回，以及为何推荐；这些信息是可比较的预览，不产生资源扣减、权限继承、排队、锁定或世界效果。玩家可以比较、确认、放弃或改道；当替代将引入原请求未包含的新资源承诺、新权限、控制权变化或不可逆后果时，必须由玩家或具备对应范围的有效 Agent 授权明确确认，并在提交时重新校验，不能仅凭原请求、Agent 计划、客户端缓存或推荐排序推定同意。

替代动作只能继承原请求中仍有效且覆盖该替代目标、作用范围与后果的授权；超出部分必须取得新的明确授权。若候选已过期、授权或世界前置发生变化、玩家拒绝，或不存在安全候选，原请求与候选均保持无世界效果，并返回变化原因和下一次可决策点。并发确认、重连或重试至多产生一次 receipt 支持的结果；原请求、所选替代与最终结果之间保留可审计关联，但该关联不是重复执行或扩大权限的凭据。

本产品层只定义上述玩家承诺和端到端边界；玩法动作粒度由 [`doc/game/prd.md`](../../game/prd.md) 与其核心玩法骨架拥有，物理/执行真值由 `world-runtime`，表现真值由 `world-simulator` 的对应专业域文档拥有。

<a id="measurable-physical-truth-contract"></a>

世界只有一条持续、权威的时间线和一套可测量的物理真值。厘米级距离、顺序、成本与持久化结果可以由工业、物流、治理等粗粒度子系统消费，也可以由 Viewer 做可读性抽象，但任何映射都必须确定、可追溯，不能因子系统分辨率或视觉夸张而改写权威结果。

间接控制不等于旁观：对当前受支持的玩家意图，系统必须呈现意图是否被接受、Agent 如何解释并执行、主要世界后果，以及玩家可用的打断、重排、纠正、fallback 或恢复动作；不能以 Agent 自主性为由隐藏因果或让玩家失去下一次决策权。

### Data 所有权与授权边界

Data 是有归属、有获取成本且受授权边界约束的世界资源。未经授权的使用必须原子失败，不产生未授权收益；产品体验需要说明成本、归属、用途、授权状态和可恢复的授权或替代路径，且可读性层或 Agent 自动化不能静默绕过权限。

Data 授权必须贯穿请求的完整生命周期，而不能只在预览或提交入口做一次布尔判断。预览只说明候选 Data 的 owner、recipient / 使用主体、purpose、scope 与当时的授权有效性，不产生访问、转移、消费或可复用权利；已接受但尚未结算的请求仍不得让 recipient 获得 Data 或由其产生收益。提交到结算之间若授权过期、被撤销、作用域变化或已无法证明当前有效，未结算请求必须原子拒绝或进入可读待决，不产生 Data sink、访问收益或隐性义务，并给出重新授权、缩小用途、改用其他合法来源或放弃的适用路径。

只有 receipt 支持的已结算结果才代表一次、且仅一次的授权使用或转移；receipt 必须能追溯 owner、recipient / 使用主体、purpose、scope、授权依据与实际结果，但产品层不冻结这些信息的字段结构。重试、重连、重复提交或历史 receipt 重放不得复制该结果，也不得让已经过期或撤销的授权复活。结算后的合法使用及其 provenance 不因后续撤销而被追溯抹除；纠错、删除、退款、争议和其他副作用仍由对应专业合同裁决。本模块只定义这些玩家承诺与跨阶段不变量，具体许可状态机、时钟、结算规则、幂等实现和副作用矩阵由专业域拥有。

<a id="resource-model-and-cross-module-provenance"></a>
### 资源模型与跨模块 provenance 边界

本表定义玩家世界规则中的来源、可用 sink 类别与不可跨越的语义边界；它不定义数值、汇率、runtime 字段或实现状态。每项的余额、资格与 receipt 必须由相应专业域证明，不能由产品文字产生可用性或公开 claim。分布式基础设施只保证已提交状态的最终性、复制和恢复，不拥有资源玩法语义。

| 资源 / 记录 | 允许的来源与 sink 类别 | 不可转移 / 转换边界 | 反补贴边界 | 专业 owner / 合同 |
| --- | --- | --- | --- | --- |
| `Electricity` / `Data` | 通用资源可按权威规则用于已授权的世界操作、工业/服务消耗与经批准的设施 commission、service 或 upkeep sink。 | 不因材料、产品、设施记录或任何 starter 支持而自动转换、铸造、转移所有权或扩大可用范围。 | 不构成持续赠与；设施不得把它们重写为无成本或无限供给。 | [`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md) |
| restricted slot-1 claim/upkeep support | 仅可在符合资格时支持首个非零 Agent claim 及其声明的 upkeep 承诺。 | 不可变为自由余额、玩家间转移、liquid starter OC、设施库存或材料。 | 不是免费认领，也不提供持续 claim/upkeep 或设施补贴。 | [`Agent ownership and stewardship`](agent-ownership-and-stewardship.prd.md)、[`agent claim economy contract`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md) |
| liquid starter OC | 仅在 Agent 已存在后承担首次对话解锁这一受限用途。 | 不支付或延长 claim/upkeep，不能转为 restricted support、设施库存、材料或通用资源。 | 不形成持续对话、认领或设施补贴。 | [`Agent ownership and stewardship`](agent-ownership-and-stewardship.prd.md)、[`agent claim economy contract`](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md) |
| `CapabilityGrantV2` execution budget | 只有通过当前 capability authorization 并实际提交世界效果的能力调用，才可按专业合同产生一次 budget debit；没有预算消耗的操作按零处理。 | 这是绑定 grant subject/owner 的、不可转移的 grant-scoped 执行预算/计量记录，不是 `Electricity`、`Data`、库存或可自由支出的玩家余额；provider、presenter、viewer 或 module 不能改写 payer、转移预算或把预算扩大到另一 grant/world。 | preview、catalog/provider/Agent response、待定或未最终确定的授权、stale response、deny、无效果失败与 replay 不扣预算；不能把失败变成部分扣费、隐藏欠费或重试信用。 | [`doc/world-runtime/wasm/wasm-interface.md`](../../world-runtime/wasm/wasm-interface.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md) |
| facility / material inventory and records | 仅在授权设施/材料生命周期内记录 commission、服务、维护、回收和可审计 receipt；允许的 sink 由对应专题声明。 | 不是通用资源类型，不能自动转换为 `Electricity`、`Data`、claim support 或 liquid starter OC，也不自动获得转移/结算权。 | 设施库存/记录不能成为持续或无成本设施补给；补充、重置或新设施来源必须由专业合同另行授权。 | [`micro_depot contract`](../../game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.prd.md)、[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md) |

来源、sink 或转换发生冲突时，以表中专业 owner 合同与新鲜证据为准，并由根 `README.md` 保持公开 claim envelope。

#### CapabilityGrantV2 预算的玩家经济语义

`CapabilityGrantV2` budget 是能力授权范围内的执行成本/计量边界，不是新的通用货币。对 `subject` 绑定到玩家或组织 `owner_binding` 的 Agent grant，该 owner 是 payer 并承担预算消耗；对 Module/System grant，payer 是 grant 所绑定的受治理模块/系统 owner 或 authority。无论哪种 subject，provider、presenter、viewer、客户端缓存或调用 Agent 都不是隐式 payer，也不能替换 owner、转移预算或借用另一 grant 的预算。预算与 `Electricity`、`Data` 分离核算；它不自动兑换、铸造、转移或补贴这两类资源，任何实际的电力、数据或其他世界资源 sink 仍须由对应专业合同另行说明。

玩家在能力预览、报价或提交结果中必须能看见该 grant 的 owner/payer、目标能力与作用范围、是否需要预算、预算影响与可负担性（`affordable`/`insufficient`/`unknown`/`degraded`），以及结果 receipt 所支持的实际变化；产品层不冻结预算单位、公式、wire 字段或 metering schedule。预览、catalog/provider/Agent response、待定或未最终确定的授权、stale provider response、权限/版本/作用域/预算不足导致的拒绝，以及没有世界效果的失败，都不得产生玩家可见的 budget debit 或隐藏债务。runtime 可以在 staged transaction 中做内部 reservation，但它不是已扣费；拒绝或失败时必须与暂存效果一并释放。

只有 fresh authorization 通过且世界效果实际提交的调用，才在同一原子结果中将预算扣减一次并生成关联 receipt；操作不收费时记录零变化。预算 debit、能力状态变化、世界 effect 与授权审计结果不能拆成可分别成功的步骤。相同 nonce/request 的重试、重连、snapshot restore 或 replay 只能返回原 receipt/待决结果，不得第二次扣费、凭 replay 产生 credit、复活已撤销 grant 或把预算转给其他 subject。预算未知、超限、owner 失配或授权证据不完整时，产品结果只能是有界 pending/deny，并明确下一次复查或修复路径，不能部分执行、隐式透支或自动 top-up。

<a name="跨资源承诺、失效与重复提交边界"></a>

### 跨资源承诺、失效与重复提交边界

所有会影响资源、资格、设施库存或持续义务的玩家路径，都必须让玩家区分**预览**、**请求已送达/入队或 pending**、**经权威资源/规则校验的已接受承诺**与 **receipt 支持的已结算结果**。该区分适用于直接操作、Agent 代办、恢复重试和重连后的继续；它不定义专业域的字段、锁定实现、时钟、价格或状态机。

| 产品阶段 | 世界允许发生什么 | 玩家能据此认为什么 | 明确不能被当作什么 | 失效、重试与审计边界 |
| --- | --- | --- | --- | --- |
| 预览 / 报价 / 推荐 | 读取当前权威事实并说明目标、预计来源/成本、主要 blocker、适用范围和下一步；不得单独产生扣减、预留、资格、排队顺位或持续义务。 | 当前可以比较或放弃候选路径；若继续，仍须提交权威动作。 | 已接受、已扣减、已锁定价格/资格，或未来必然成功。 | 输入、权限、世界状态、报价有效条件或信息新鲜度变化后，旧预览不能静默复用；必须重新评估或明确拒绝，并保留替代/恢复路径。 |
| 请求已送达 / 入队 / pending | 接口可完成本层身份与格式等准入并保存待处理请求；尚未通过所属专业域的当前资源/规则校验时，不产生资源 hold、sink、设施、生产顺位或持续义务。 | 请求已被接收或等待有序处理；其世界结果仍待决定，可查询原因与下一复查点。 | 全部资源/规则校验通过、已获产能、已接受工业承诺或已结算。 | 后续从当前权威条件裁决；拒绝不产生隐性扣款。transport/submit 的 accepted 命名不能改变该阶段语义，重复查询不能自动重提或插队。 |
| 已接受的承诺 | 完成所属专业域当前权限、资源、治理与反滥用校验后，才能按专业合同形成有限的 hold、执行排程或持续义务；结果必须说明作用对象、范围、已取得与仍未取得的容量/资源、是否已有不可逆结果及下一次可决策点。 | 指定范围的承诺已获权威准入，以及哪些后果仍待完成或可被拒绝/解除。 | 已结算世界结果、所有未来容量已预留、自动续期、对无关资源/资格的授权，或对后续同类动作的无限保证。 | 过期、撤销、前置条件失效、重复提交或无法证明当前有效性时，不得生成新的 sink 或第二次效果；未结算部分按专业合同释放、拒绝或保持待决，不能静默改成欠费、补贴或其他资源类别。 |
| receipt 支持的已结算结果 | 权威世界结果、允许的 source/sink 变化及其责任/恢复边界已经生效，并可由对应专业域 evidence 追溯。 | 该范围内的资源/资格/义务后果已经成立，可据 receipt 进入下一步或适用的救济。 | 对历史 receipt 的重放以取得额外资源、再次结算、复活已到期资格，或抹除既有责任/历史。 | 纠错、退款、reclaim、争议或持续义务的违约只按各自专业合同处理，并保留资金来源、已发生后果与责任 provenance；产品层不得把它们承诺为统一自动退款或成功。 |

因此，玩家可以在承诺前比较和撤回候选，在已接受后读懂未决范围与下一步，并在结算后凭 receipt 继续、申诉或恢复；不能把建议、界面缓存、Agent 的计划、重连或历史 receipt 当成免费资源、既得资格或第二次执行授权。Agent 自动化与任何表现层同样受此边界约束，不能用“已计划”或“请求已送达”伪装为世界已经结算。

#### 条件报价与提交再校验合同

报价是读取时刻的条件性判断，不是对资源、设施容量、资格、价格、排队顺位或世界结果的预留。报价至少绑定其目标、输入、作用域与当时用于判断的权威条件；产品层不冻结版本字段、过期时钟或锁定实现，但不得把缺少这些实现细节误写成“报价仍然有效”。`electricity_after` 只说明该次报价中的可用资源扣减后余额；它不等同于 Agent 的电力运行 runway，也不能单独承诺生产会持续、不会进入临界/停机状态。两者都影响当前决定时，必须分别说明适用的余额、runway/风险和推荐的恢复或暂缓路径。

- **提交时重新裁决：** 提交必须针对当时的权限、资源、设施/地点、治理与反滥用条件重新校验，而不是消费旧报价。若任一条件已变化，系统只能以当前规则接受并产生一次可追溯 receipt，或原子拒绝且不扣减任何资源/不新建义务；不得静默沿用旧成本、旧风险、旧资格或把不足额变成隐藏欠费。拒绝或重新报价必须说明改变了哪类条件，并给出重新评估、等待、修复、改道或放弃当前候选中的适用下一步。
- **竞争、重连与 Agent 边界：** 两个玩家/Agent 对同一可争用事实取得的报价可以并存，但先被接受的动作改变条件后，后续提交没有优先权。重连、刷新或重复请求报价只会得到新的条件判断，不构成提交幂等键、自动续期或离线执行授权；Agent 取得报价后仍须拥有独立、有效且范围受限的动作授权才能提交。已经接受的动作的幂等、撤销和未决处理由其专业合同与 receipt 负责，不能由报价本身补足。

以下边界必须可用同一条代表性工业/服务路径验收：报价后插入一个已接受动作，使资源余额、设施可用性、权限或电力状态中的至少一项改变；旧报价的后续提交要么在当前条件下只结算一次并提供 receipt，要么原子拒绝且保留原有余额/义务。玩家能够区分“资源余额仍可支付”与“运行 runway/停机风险”，并能读到变化原因与下一步；把报价缓存直接当作预留、把失败转为欠费，或在重试时产生第二次 sink，均为失败。

#### BuildFactory 建设电力报价边界（冻结玩家承诺，不冻结实现公式）

`BuildFactory` 必须遵循条件报价合同。提交前，玩家或具备范围授权的 Agent 能比较 `build_now`、`prepare_inputs`、`move_to_site`、`restore_power_first` 与 `defer`；只展示专业合同当前真实支持且前置可证明的候选，每个可选项都说明目标/范围、追加成本与仍占用、预计结果/复查时延、失败或损失风险、可撤回性及推荐理由。报价至少说明 owner、site/location、factory kind/id、owner/location/chunk/existing-or-pending ID/recipe-fit 前置、当次 candidate/config/world revision，以及 owner-held electricity obligation 和每项 construction input obligation 的 kind、quantity、适用 ledger、before/after；若 surface 提供 aggregate，仍必须能展开到每项输入，不能隐藏 multi-input shortfall、重复收费或漏收。报价还必须说明建设/激活边界、首个工业目标关联、主要 blocker、下一次复查点和推荐动作，并提供可将本次条件判断与后续 submit/receipt 关联的稳定 quote identity 或等价 immutable context digest；产品层不冻结字段名或编码。报价只读，不产生 sink、hold、设施、排队顺位、激活或未来电力承诺；`electricity_after` 只表示按报价预计扣除后的余额，不表示 Agent runway、设施维护 runway 或不停机保证。

建设电力义务必须由适用专业 profile 明示为 `start-only sink`、`held-through-completion/arrival`、`boundary-revalidated conditional` 或 `non-reserved` 之一；未纳入 reservation chain 的部分必须标为 power-best-effort。建设成本、后续 `base_power_draw`/maintenance、配方电力与 battery runway 是不同信号，不能互相冒充。若 factory kind 会注册 PowerPlant，专业 profile 必须唯一选择建设完成或激活中的一个权威 output boundary；发电能力只能在该结果后生效，不能由报价或待建设状态提前入账。

在实现冻结前，成本/资源映射、power mode、建设/激活边界及权威 revision 必须回写到 [`world-runtime` industrial execution status and authority matrix](../../world-runtime/prd.md#industrial-execution-status-and-authority-matrix)：`world-runtime` 拥有 event/schema 与当前实现状态，M4 拥有 construction domain/profile，game/product 拥有玩家承诺，由 TPM 合流为单一 task truth；本条不选择其中任一实现，也不建立联合第二权威。该决定未记录前，任何 surface 不得把某一实现公式、字段或跨模块“已对齐”作为当前产品保证；无法证明的事实显示 `unknown/degraded` 并说明 authority 尚未冻结，不能伪造为 0、可支付或已预留。

提交必须绑定报价所示 candidate/config/world revision，并从新鲜权威状态重新校验 owner、site/location/chunk、kind/id、资源余额与授权。报价后竞争性状态发生变化时，只能按当前条件至多结算一次并产生 receipt，或原子拒绝且不产生任何新 sink、设施或义务；不得沿用旧成本、隐藏欠费、自动 top-up、overdraft、隐式 hold，或把失败重试当成新授权。重复提交、重连、Agent retry、snapshot restore 与 replay 只能重读同一结果，不能复制建设扣款、`FactoryBuilt`、激活、发电或奖励。

`test_tier_required` 至少覆盖：报价无世界效果且同一快照确定；正常建设、缺电、任一 construction input shortfall、owner/location/chunk/kind/existing-or-pending ID blocker 的正负样例；报价后资源/权限/地点漂移的 current-state revalidation 或 atomic reject；`start-only` 在接受后余额漂移时不改写旧 commitment，`held-through` 的 hold/consume/release 各至多一次，`boundary-revalidated` 在声明的 construction/activation boundary 重验，`non-reserved` 明示 best-effort，reservation mode 的竞争与 unknown 不 over-hold、不插队也不伪造 0；construction sink 与 recipe/runway/maintenance 的分离；PowerPlant 在 profile 声明的 output boundary 前不产出；Viewer 与 pure API 的逐项成本、mode、blocker、下一步、quote correlation 和 receipt 一致。本文不冻结绝对数值、runtime schema、队列、UI 布局或当前实现完成声明。

#### 在途工业任务的承诺、中断与结算

工业排程被接受后，不能继续沿用“报价可随时放弃”的语义，也不能把目标换向、停机或重连自动解释为取消。代表性配方任务必须让玩家在提交前读懂：接受时会消耗或占用的投入与能源、预期产出及副产物去向、形成结果所需的时间或阶段、是否存在取消/中止窗口，以及适用规则会保留、退回或损失什么。报价仍然只读；提交时只能原子拒绝且无新 sink，或创建一次可追溯的已接受工业承诺。

已接受任务必须保持可区分的 `已接受/待开始 -> 已开始 -> 进行中 -> 已完成 / 被阻塞 / 已中止` 产品结果；`被阻塞` 是等待恢复、继续或按专业合同转为终止的可决策状态，不是隐含完成。若专业合同允许开始前释放、撤回或过期，也必须将其表达为未开始的终止结果，不能伪装成已中止的在制任务。这里的分类不冻结 runtime 枚举或 UI 字段，但任何专业实现都不能把“请求送达”“目标已换向”“设施暂不可用”或“客户端已断开”伪装成开始、完成或中止。目标换向、Agent 打断、维护/权限/设施状态变化、重连、重复提交与事件重放均不得静默迁移旧任务、再次扣料、复制产出/副产物，或同时生成退款与完成结果。

取消或中止只在专业合同声明存在该能力时可用，并必须遵循当次任务适用的确定性处置规则：尚未开始的 hold 可以按规则释放；已经产生 sink、在制进度或世界效果的任务不得承诺全额自动退款。任何 salvage、部分返还、在制品保留、继续执行或安全停机都必须范围有界、只结算一次，并保留原投入、已发生结果和处置原因的 provenance；没有已实现且有证据的中止能力时，玩家只能看到真实可用的等待、修复、改道或重新规划路径，不能展示虚构的“取消”。

工业任务进入 `被阻塞` 后，必须收成一次可比较的恢复/退出决策，而不是只展示状态标签。玩家至少要看到：最早可归因的根因所在阶段/边、仍占用或已消费的投入与容量、当前保留/损失的在制价值、稳定窗口是暂停/清零/不适用、下一次权威复查边界，以及专业合同当前真实支持的 `继续/等待`、`修复`、`改道`、`降载或拆分`、`重新规划`、`中止/放弃` 路径。每个展示候选都必须逐项说明作用目标/范围、追加成本与仍占用、预计结果/复查时延、失败/损失风险、可撤回性及推荐理由；缺少任一字段的候选不得展示为可选路径。不存在安全路径时必须明确停止并给出下一次可决策点；不得用后台重试、自动改道、自动退款或“已恢复”标签代替玩家确认。恢复若仍属于同一因果计划，只能以同一 root 的 continuation 继续；若改变配方、设施、边或终端等产出因果，必须创建链接旧任务的新 revision/candidate，并从 `W=0` 重新计数。重复确认、重连、Agent 重试与 replay 只能返回同一阻塞/恢复处置，不得重复释放、扣料、产出、退款或里程碑。

中断后的玩家反馈至少回答：原任务及当前阶段、已经投入与仍被占用的价值、已形成或仍待形成的产出/副产物、保留与损失、对稳定产线候选进度的影响，以及当前真实可用的继续、等待、修复、改道、中止或重新规划动作。`game` 拥有这些选择的玩法取舍与平衡，`world-runtime` 拥有事件、状态、资源守恒、去重与 replay，Agent 拥有意图打断/换向的可解释 handoff，Viewer 与 pure API 拥有同一权威结果的表达，QA 拥有组合证据；产品层不规定取消 action、退款公式、队列算法、状态 schema 或 UI 布局。

<a id="source-deposit-product-contract"></a>
#### 原材料 source/deposit 的发现、争用、枯竭与补充

原材料流水线不能从一个未解释的“可采资源”数字直接开始。任何可进入采集/补充决策的 source/deposit 都必须保留稳定身份，并把**观察到来源**、**当前可提交的采集机会**与**receipt 支持的实际采出**分开。地图提示、材质倾向、Agent 推荐或旧观察只能帮助发现候选，不能单独证明当前可采量、访问资格、优先级或未来补充；专业 profile 必须声明该来源适用的发现/可见范围、数量与质量事实的新鲜度、访问或作用域边界，以及 depletion/replenishment 是否存在。未知、过期或互相冲突的事实显示 `unknown/stale` 或等价产品语义，不能按零竞争、无限库存或可直接采集处理。现有 chunk/fragment 生成、预算、首次采集与 `FragmentsReplenished` 的场景 authority 继续由 [`chunked fragment generation`](../../world-simulator/scenario/chunked-fragment-generation.prd.md) 专题拥有；本节只冻结跨 source profile 共用的产品决策与反滥用边界，不另建资源生成或补充真值。

提交前的只读 `source_assessment` 是现有 `resource_replenishment_quote` / `fragment_refill_preview` 与首次采集 preview 的跨 profile 产品覆盖，不新增第二套 action authority。它必须让玩家比较专业合同当前真实支持的 `mine_now`、`reduce_batch`、`move_to_other_frag`、`wait_for_replenish`、`restore_power_first`，以及其他 profile 已声明的准备访问/路线、改用合法材料或配方与延期动作；跨 profile 读面可以使用同义玩家文案，但必须映射回对应专业 action，不能发明不可提交的 `extract_now` 或 `wait_for_replenishment`。每个候选至少说明 source 身份与作用域、预计可满足量及其权威/估计边界、材料 profile/质量提示、访问/竞争条件、采集与到首个目标账本的时间/资源/物流成本、枯竭或补充状态、主要风险、对当前配方/首个工业目标的影响、`next_action` 与 `next_recheck`。预览、扫描、推荐和刷新都无世界效果，不创建 claim、hold、采集顺位、材料批次或补充资格；没有安全来源时必须明确停止或转向，不能无限推荐等待、静默自动换源或把同名材料当作等价输入。

采集提交必须从新鲜权威状态重新校验 source、访问权限、可分配量、材料适用性和必要路线/容量。多个玩家或 Agent 争用同一有限来源时，专业合同必须基于同一权威 allocation snapshot 与公开、确定、可回放的顺序/配给规则形成有界结果；网络到达顺序、后台处理顺序、反复扫描、拆小批、重连或 retry 不得取得隐藏优先级、超额占用或第二份材料。成功结果只能对实际获配量产生一次 source effect/receipt 与带 parent provenance 的 material batch；不足、耗尽、权限或状态漂移只能按既有 allocation 语义得到 `full/partial/deferred/denied` 中适用的可读结果、原子拒绝或重新报价，不能透支、负库存、自动补料或把失败变成隐式债务。`expired` 只在专业合同已声明 quote/intent 到期处置时使用，不得伪装成第五种 allocation outcome。

depletion 与 replenishment 必须延续同一 source 历史，而不是把客户端刷新或定时提示当作新库存。耗尽结果必须说明已采出、仍被有效承诺、未满足数量和真实的迁移/替代/等待路径；只有权威 replenishment effect 生效后，fresh assessment 才能看到新增可分配量。补充事件、到期/撤销释放和其触发的重评各最多产生一次效果；等待不会自动续期旧 quote、保留优先级或承诺 ETA。若 profile 不支持补充，耗尽来源不得持续显示 `wait_for_replenish`；若补充时刻或数量不可证明，必须显示范围/未知与复查条件，不能伪造精确倒计时或保证。

这项边界保持 **world-first**：来源可用性、争用和采出只由同一权威世界历史中的 assessment/commit/receipt 决定；保持 **emergence-first**：玩家可在更近但拥挤、较远但稳定、等待补充、减量或换材料/配方之间取舍；保持 **persistent / auditable**：source 身份、观察新鲜度、allocation、depletion、replenishment、实际采出与 material batch lineage 跨重连、恢复和 replay 延续；保持 **extensible**：矿藏、可补充 frag、回收源或未来其他 source profile 可声明各自规则，但不能绕过共同的只读预览、提交重验、有限分配与单次结算边界。

`test_tier_required` 至少覆盖：未知/已发现但过期/当前可采/耗尽/补充中或不支持补充的来源；同一 snapshot 下两个或以上意图争用同一有限 source 时总获配量不超过权威可分配量，等价重排、拆批与 retry 不改变已声明优先依据或复制占用；报价后数量、质量、权限或路线漂移时重新报价、形成新的明确 allocation snapshot，或在无 source/material sink 下原子拒绝；成功采集只产生一次 source receipt 与一次匹配 material batch；耗尽后提供真实换源/等待/换材料或配方路径，权威补充后才新增一次可分配量。重连、乱序、snapshot restore 与 replay 不复制采出、释放、补充、物流 credit、稳定进度或奖励；Viewer、pure API 与 Agent 对 source 状态、估计/权威边界、已承诺/未满足量、主要风险、下一动作和复查点保持同义。本文不冻结资源分布、发现概率、产率、补充周期、allocation/公平算法、所有权制度、runtime schema、地图/UI 或当前实现完成声明。

#### 从采集/补充到首个可消费账本输入的 handoff

首个可消费账本输入必须沿一条可追溯的来源链形成：`extraction/replenishment → refinement → source-to-ledger handoff → first canonical consumable ledger input`。这里的“形成”不是把采集结果、精炼结果或一条推荐直接当作下游库存；只有来源/精炼效果已按专业合同结算、handoff 按适用的账本与物流规则完成、实际数量和损耗可对账，并取得当前阶段的适用性结论后，批次才可作为下游 canonical input。产品层只冻结这条玩家可理解的因果边界；这些权威边界由 [`M4 industrial resource flow contract`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md)、`world-runtime` 与相关 Recipe/Product/Factory 专业合同联合界定：M4 负责材料账本/类型标签、handoff 与领域适用性语义，world-runtime 负责 receipt/event/schema、转移/结算顺序与当前实现状态，Recipe/Product/Factory 合同负责各自 profile 的输入/输出适用性及能力声明；本节不新增字段或实现完成声明。

玩家可以在提交前比较当前专业合同真实支持的采集/补充、减量、换来源/地点、等待补充、先恢复电力、精炼至目标或最小合法量、调运/等待容量和延期路径，并读到来源、预计实际产出/损耗、成本、目标账本/用途、第一配方关联、主要 blocker 与下一次复查点；确认后仍须按当前权威状态重新校验。玩家不能把 preview、推荐、已接受、精炼中、来源账本中的库存或在途承诺当作已到达、适用或已消耗的 canonical input，不能直接写入/改写目标账本、绕过 owner/权限/规格/容量、按材料名称猜测适用性，或以客户端/Agent 重试复制采集、精炼、转移、到达和首个 input sink。没有专业合同支持的自动补充、免费转换、静默降级、隐式改道或全额退款不得作为可选动作。

玩家与 Agent 共享的只读结果面必须把以下语义状态分开表达（这些是产品语义，不冻结 runtime 枚举或 UI）：

| 读面状态 | 允许玩家据此理解 | 仍然不能据此理解 |
| --- | --- | --- |
| `preview` | 当前来源、精炼与 handoff 候选、预计数量/损耗、目的账本、成本、风险和复查点可比较；无世界效果 | 已扣减、已预留、已到达、已适用或必然成功 |
| `source-settled` | 采集/补充或精炼的 source effect 已由一次 receipt 结算；这是源效果结算，不是 M4 的目的账本/终端 `settled` 生命周期结算，实际产物与 provenance 仍可追溯并待交接 | 下游账本已有可消费库存、规格已适用或已产生下游进度 |
| `in-transit` | 已接受一条明确的 source→edge→destination handoff，来源 effect 至多一次，预计到达量/损耗/等待和占用可读 | 目的账本已 credit、下游已解锁或交付已完成 |
| `arrived` | 实际到达数量已进入声明的目的账本/有界 buffer，保留 parent receipt、批次与损耗 lineage | 到达自动等于适用、可独占消费、生产完成或终端交付 |
| `applicable` | 当前 root/阶段/配方/规格/owner/账本条件下，matching parent receipt 与 arrival 允许该批次成为首个 canonical consumable ledger input | 可以跨 root/阶段复用、绕过 join/容量，或提前产生第二次 sink/奖励 |
| `blocked` | 最早可归因 blocker（来源、精炼、电力、权限、路线、容量、到达、规格/证据或 owner）及真实支持的等待、修复、补充、重报价、改道、隔离、返工、返还、补偿或放弃路径可读 | 已完成、已适用、已退款，或可由后台重试自动解除 |

失败恢复必须保持同一因果链：报价/预览过期或提交前置漂移时原子拒绝并保留原状态；采集/补充或精炼失败时不扣未结算资源，玩家可减量、换来源、等待补充、补电或重新报价；handoff 因权限、路线、容量或目的账本失效时，可在专业合同支持下等待容量、保留/隔离、显式 return、重报价、改道或补偿，否则保持有界 `blocked`；到达后规格/证据不足只能重验证、等待或拒绝，不能以同名材料、旧 receipt 或自动降级解锁下游。已经发生的 sink、运输损耗、在制或到达效果不得被“恢复”抹除；每个释放、返还、返工、salvage、conversion 或 compensating receipt 都必须一次且有 parent provenance，重复提交、重连、乱序、snapshot restore 与 replay 只重读同一 disposition。

该 handoff 的守恒与反滥用边界是：来源数量、精炼转化、运输损耗、目的账本 credit、适用性 decision 与首个 input sink 分层对账；一条 source→edge→destination operation 至多产生一个有效 source effect 与一个目的结算，未知/不适用/过期/错误 root 的 decision 在首个 sink 前 fail closed；同一数量不能同时属于多个目的账本或 join；未支持的 split/merge/mix 不得把残余变成免费库存；任何新用途、奖励或稳定进度只能由相应 production/delivery/settlement receipt 触发。玩家和 Agent 不能通过反复 preview、拆小批、改变到达顺序、重复 arrival 或跨入口重试刷取材料、解锁、优先级、稳定窗口或奖励。

这条产品边界的组合影响必须保持明确：**world-first** 要求账本输入反映真实来源、精炼、运输与容量约束；**emergence-first** 允许玩家/Agent 在这些约束内自选来源、时机、路线和恢复，而不是领取预写的免费转换；**persistent** 要求批次、receipt、lineage 与 blocked/待决状态跨重连、恢复和 replay 延续；**auditable** 要求每次实际数量、损耗、适用结论、owner/账本、原因与处置可追溯；**extensible** 允许未来新增 extraction、replenishment、refinement、ledger 或 handoff profile 复用同一边界而不改变既有历史。玩家承诺只到首个 canonical consumable ledger input；更细的配方、数值平衡、字段、队列、物流/账本算法、WASM/runtime 实现和 Viewer 布局仍由专业 authority 决定。

本 handoff 的 Non-Goals 是：不规定采集、补种、精炼或运输的产率/价格/损耗公式，不新增或重命名 runtime/ABI/ledger 字段，不决定队列、公平性、UI 布局或 Agent 自动化策略，不把产品读面当作当前实现完成证明，也不替代 `game` 的玩法平衡、M4 的材料/账本合同或 `world-runtime` 的事件、状态、持久化与 replay 权威。

#### 调运（`TransferMaterial`）的报价、在途与到达边界

当来源账本中的材料需要跨站点、跨区域或跨账本支撑当前配方/稳定产线时，产品层把 `TransferMaterial` 收束为一条可追溯的 source → edge → destination handoff；玩家承诺、可比较动作与反馈边界由产品层和 [`gameplay top-level design`](../../game/gameplay/gameplay-top-level-design.prd.md) 拥有，M4 拥有材料账本、handoff、适用性与物流等工业领域语义，`world-runtime` 拥有 event/receipt/job/replay、identity/schema 以及当前执行事实。提交前的 `logistics_transfer_quote` / `transfer_impact_preview` 必须是只读、确定且带条件的当前快照，至少让玩家看见 `submission_feasible`、可调运上限、来源前后可用量、目的地预计到达量、预计损耗/预计收到量、到达 tick/`ready_at`、在途量与容量、有效优先级及理由；当前配方缺口和推荐动作只能作为上下文派生读面，不新增第二套 runtime quote authority。

玩家必须能在确认前比较专业合同真实支持的 `submit_transfer`、`wait_for_transit_capacity`、`reduce_amount_or_source_materials`、改走本地/替代来源与延期/重新规划。报价不预留材料、不占用吞吐、不推进 tick 或账本；提交时按当前权威库存、距离/路线、容量、权限、优先级与目标账本重新校验。任一前置漂移、报价过期或无可执行容量都只能原子拒绝且不扣材料、不生成部分在途货物，并说明旧报价失效与下一次可决策点；不得静默降额、后台重试或自动改道。

`in-transit` 仅表示一次已接受、保留 source/edge/destination、预计到达量/损耗与占用的调运承诺；它不等于目的账本已 credit、材料已到达、已适用或已解除配方缺口。只有权威 arrival receipt 将实际收到量写入声明的目的账本/有界 buffer 后才是 `arrived`；`arrived` 仍须独立通过批次、规格、owner、账本、root/阶段与配方条件验证，才能成为 canonical consumable input。失败时仅可使用专业合同已声明的等待容量后重报价、减量/换来源、修复权限、显式 return/hold/quarantine、改道或重新规划；已发生的运输损耗、在制或到达效果不得被恢复抹除，也不得默认全额退款。

同一 accepted intent 的 source effect、在途创建、arrival credit、损耗与配方缺口推进各至多结算一次，并保留 parent receipt、batch/root lineage。重连、重复提交、乱序、snapshot restore、Agent retry 与 replay 只能重读同一 transfer/arrival disposition；若当前专业实现无法证明 lineage/idempotency，surface 必须先展示既有结果或要求新的显式决策，不能自动重提。`test_tier_required` 至少覆盖：跨站正向到达且缺口只解除一次；吞吐已满时零扣减、`wait_for_transit_capacity` 与重新报价；来源不足时减量/换源且无部分扣料；距离/权限失败无世界效果；报价后库存/容量漂移原子拒绝并保留原状态；重连、重复提交与 replay 不复制调运、损耗、到达或稳定窗口进度。本文不冻结物流算法、价格/损耗/吞吐数值、runtime 字段/schema、队列、UI 布局或当前实现完成声明。

#### 原材料批次时效、质量漂移与仓储保管

材料到达目的账本不等于其质量永远有效。每个材料/产品 profile 必须声明自身属于 `stable` 或 `time/environment-sensitive`：`stable` 批次在等待、运输和有界 buffer 保管期间不因时间自动失效，但仍受新的权威规格、owner、账本或污染/损坏事实约束；`time/environment-sensitive` 批次则必须声明适用的有效性边界、保管/运输条件与重新验证点。材料批次、profile、质量/规格适用性与 custody 语义的专业 authority 是 [`M4 industrial resource flow contract`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md)；`world-runtime` 继续拥有事件、schema、权威状态、时间与当前实现状态，产品层只冻结玩家承诺，不成为第二套 schema authority。未声明或 legacy profile 在完成显式 admission/backfill 前一律视为 `unknown`，并在首个 input sink、WIP、稳定进度或奖励前 fail closed；不能由本段文字、材料名称、客户端缓存或 Agent 推断为 `stable`、可用或已完成 backfill。该分类只定义产品结果语义，不规定所有材料都要衰减。

材料在 source/refinement settlement 时必须保留可追溯的质量 provenance：批次身份、材料 profile、实际数量、规格/品质证据、owner/账本、结算与当前 custody 的权威锚点，以及与 parent receipt 的 lineage。运输、转入/转出 buffer、保管条件改变或重新验证都不能就地改写 parent receipt；只能在声明的 custody segment 上产生链接的状态/decision 或处置 receipt，并保留已发生的数量、损耗和责任。仓储容量、占用和保管义务属于真实的 batch/custody 状态，不能因 surface 显示、重连或“等待”而免费释放或隐藏。

`time/environment-sensitive` 批次只在 profile 声明的边界重新验证，例如 handoff 接受、transit launch/arrival、buffer admission、recipe input join/start 或某个权威 world-time checkpoint；不得由 Viewer 刷新、客户端计时或每次重复预览自行制造验证。每个边界都必须以当前权威时间、路线/保管条件、批次 provenance、目的账本与下游规格重新判断。边界证据缺失、条件漂移或结果无法证明时，批次为 `unknown`/`blocked`，在首个 input sink、WIP、稳定进度、需求减少或奖励前 fail closed；不能把未知写成零损耗、稳定、可适用或自动延期。

质量是叠加在既有 handoff lifecycle 上的正交 facet，不替换 `preview`、`source-settled`、`in-transit`、`arrived`、`applicable` 与 `blocked` 的批次流转身份：`arrived` 只证明 custody/到达，不证明质量可用；`applicable` 必须同时满足到达、当前规格/用途条件与质量 facet 为 usable。这里的 `usable` 不是新的 enum 或持久化 lifecycle state，而是由 canonical M4/Recipe/Product target profile 针对当前用途派生的 predicate；它仅在当前 profile 证明质量有效，或显式允许 `degraded` 用于当前用途时成立。`unknown`、`quarantine`、`expired` 在消费侧一律映射为 `blocked`；`degraded` 只有目标 recipe/阶段 profile 明示允许该质量用途时才可映射为 `applicable`，否则仍为 `blocked`。质量重验保留旧 decision 的 immutable 结果，并只通过 linked revalidation decision 改变后续消费判断，不能回写旧 handoff receipt 或把质量结论提升为新的到达结算。

玩家与 Agent 的读面必须区分：`blocked`（必要有效性或 custody 证据尚未成立，保留当前占用与复查点）、`degraded`（profile 证明质量下降但存在明确的受限用途或 at-risk 继续规则）、`expired`（有效边界已过，不取得新的消费资格）和 `quarantine`（因污染、条件违约或待复验而隔离，不能被下游消费）。`degraded` 只有在目标 recipe/阶段 profile 明确允许时才可进入新的候选；`expired` 或 `quarantine` 不得靠同名材料、降级标签、混批或换账本恢复。读面至少说明批次/阶段、质量原因、仍占用或已损失的价值、下一次权威复查点，以及当前专业合同真实支持的恢复动作；这些是产品语义，不冻结 runtime enum 或 UI 布局。

恢复与处置必须显式且有界：专业合同支持时，玩家可以比较 `revalidate`、`rework`、`salvage`、`return`、`discard`、继续隔离或重新取得合法来源；每个候选说明追加成本、保管/容量占用、预计质量/数量结果、交付或配方影响、失败损失、可撤回性和推荐理由。`revalidate` 只能从 fresh authoritative state 产生链接旧 decision 的新 decision；`rework`、`salvage` 或转换必须创建新的 batch/lineage，并披露实际投入、损耗与新用途；`return`、`discard` 和补偿只能按专业合同各结算一次。没有对应能力时保持真实 `blocked`/`quarantine` 或拒绝，不能自动退款、免费补发、静默修复、无限延期或把隔离物料伪装成下游库存。

每个 `batch + custody segment + revalidation boundary` 的质量判断和处置至多生效一次，并绑定 parent receipt、当前 root/阶段/配方用途及权威时间/状态锚点。边界前后发生漂移时，旧 decision 保持 immutable，由 profile 创建 linked revalidation/处置；重复 submit、arrival、拆小批、重连、Agent retry、乱序、snapshot restore 与 replay 只能重读相同结果，不能刷新有效期、复制 sink、重复释放保管容量、把同一数量同时分给多个 join，或以混批掩盖失效批次。拆分、汇合和跨账本转移必须分别复验每个 parent batch；任何由 rework/salvage/return 形成的新用途都必须由新的可追溯 receipt 触发。

该边界保持 **world-first**：配方输入反映真实时间、保管条件、规格与损耗，而不是稳定库存假设；保持 **emergence-first**：玩家在保管、调运、复验、返工和改换来源之间做有代价的选择；保持 **persistent**：批次年龄/条件结论、隔离、处置和 lineage 跨重连、恢复与 replay 延续；保持 **auditable**：质量证据、custody、处置、数量/损耗和 parent receipt 可追溯；保持 **extensible**：未来可增加稳定、时效、环境或污染 profile，而不改写既有 settlement 与历史结果。`game` 拥有 profile 的节奏、机会成本和玩法取舍，M4/Recipe/Product 拥有材料质量与适用性语义，`world-runtime` 拥有权威时钟、状态、事件、持久化和 replay，QA 拥有组合验证；产品层不替代这些专业 authority。

本节的 Non-Goals 是：不规定衰减/污染/温度/湿度/保质期的绝对数值、公式或仿真模型；不默认所有材料可腐败或必须保温；不新增或重命名 runtime/ABI/ledger 字段，不决定运输寻路、队列、公平性、价格、产率、损耗、返工收益、配给或 UI 布局；不把 `degraded`、`expired`、`quarantine` 写成当前 runtime 已实现或公开发布能力，也不承诺自动补货、退款、销毁或补偿。

`test_tier_required` 至少覆盖：`stable` 批次在等待/运输/有界保管中保持质量但遇权威规格或污染事实变化时重新判定；`time/environment-sensitive` 批次在有效边界内可一次成为 `applicable`，跨过声明边界后只能得到 `degraded`、`expired`、`quarantine` 或 `blocked` 中与 profile 一致的结果，且在首个 input sink/WIP/progress/reward 前不消费；缺失 custody、时间、条件或质量证据时 fail closed 并显示 primary blocker/复查点；`revalidate`、`rework`、`salvage`、`return`、`discard` 各自只产生一次、保留 parent provenance 和实际数量/损耗；拆分/汇合/转移不把失效批次混成适用输入；重复 submit、arrival、重连、retry、乱序、snapshot restore 与 replay 不刷新有效期、不复制 sink/处置/容量释放；玩家与 Agent 以及 Viewer/pure API 对质量状态、占用/损失、lineage、下一步和不可用原因保持同义。`test_tier_full` 的跨窗口、长时间与多阶段组合验证由 M4、runtime、gameplay、Viewer 与 QA 专业合同另行定义。

#### 多阶段工业流水线与中间品背压

单个配方任务完成不等于多阶段流水线已经连通。代表性流水线必须以有向无环的阶段关系声明 `上游阶段 -> 中间品边 -> 下游阶段`；每条边至少绑定可追溯的阶段/配方或能力版本、材料类别、来源与目的账本，以及适用的电力、物流和容量前置。替换决定产出因果的阶段、配方、设施或边后，属于新的候选流水线，不能继承旧候选的稳定窗口、未决资格或里程碑进度。循环生产若未来成为正式能力，必须由独立专业合同定义库存上限、终止与反套利规则，不能把隐式环当作普通流水线接受。

中间品必须区分**可用**、**已预留**、**加工中**与**已承诺但尚未到达**。下游只有在上游产出已由权威 receipt 结算，材料按适用物流规则实际进入下游可消费账本，并且该批次满足下游阶段声明的材料规格/品质适用范围后，才能取得开工资格；上游的计划、报价、已接受或加工中状态都不能提前生成下游库存、进度或里程碑。规格/品质适用范围是消费资格，不是产品层新增的数值公式：提交前的适用预览必须说明候选批次、可消费数量、缺口与主要不适用原因，提交时则按当前批次、配方和阶段条件原子重验；来源未知、证据不足或不兼容时只能在下游产生 sink 或进度前 fail closed，不能由 Agent、Viewer、模块元数据或同类材料名称自行推断为可用。一次中间品不能同时被多个下游承诺消费；拆分、混批或汇合必须声明边集合、确定性的分配/齐套规则，以及“只消费适用数量并保留其余批次”或“整笔原子拒绝”的明确策略，并分别验证各输入批次的适用性，不能依赖隐藏提交顺序、Agent 猜测、表现层缓存或把不同批次静默混合后视为合格。

需要两个或以上独立输入边的阶段必须为每个 candidate + recipe + executable cycle/batch 声明单一 canonical join identity，以及稳定有序的 required parent-edge/receipt set、各自数量/规格/版本和适用窗口。默认 **all-input atomic**：所有 parent receipt 必须已经结算并实际到达目标账本、在同一权威快照上适用且可独占消费，才能产生第一个 input sink、WIP、阶段进度或稳定计数；先到的 parent 保持在权威 ledger/有界 hold 的 `join_pending`，不能按材料同名、最新到达或客户端顺序猜测缺失 parent，也不能形成隐藏 partial kit。

只有专业 profile 明确支持 staged intake 时，才能在齐套前消费部分 parent；此时必须把每个已消费 parent 与 join identity 绑定，公开 remaining parent obligations、仍占用的 stage/buffer/hold、完成条件，以及缺失、无效、过期或换线 parent 的 release/retain/reject/rework/salvage/return 中真实受支持的处置。Late arrival、资格恢复或容量释放只重评 unmet parent set；parent reservation 只能归属一个 join 并至多释放/消费一次。Cutover 保留旧 join 与 parent lineage，新候选不得复用旧 receipt；retry、reconnect、到达乱序和 replay 不得重复 sink、拼出第二份 kit、单边伪造完成或增加稳定进度/奖励。

Staged intake 还必须显式防止**部分保留互等**：阶段图是 DAG 并不能证明 reservation 依赖无环，因为不同 join 可能分别持有已消费 parent、stage slot、edge 或 buffer，随后互相等待对方尚未满足的 parent/容量。若同一权威 allocation snapshot 发现这种 wait cycle，或无法证明当前依赖关系可继续收敛，系统必须在新增不可逆 sink、WIP 或稳定进度前 fail closed；不得以 retry、重连、网络到达顺序或“已接受”状态继续加 hold。该 profile 必须声明 cycle 的可读 root、当前主体自己的 held/unmet、下一次确定的重评边界，以及真实支持的 release、retain、defer、终止或专业补偿路径；每次重评只能产生一次 complete、继续等待、释放/终止或原子拒绝结果，不能无界延长 hold 或静默跳过仍有效的意图。已消费 parent 的处置仍须沿原 join lineage 单次结算，不能为打破互等而隐式回滚、混批、改道或同时退款与完成；retry、reconnect、恢复和 replay 必须重读同一 cycle disposition。

每条中间品边必须声明有界缓冲与背压结果：下游暂不可用或缓冲已满时，只能按专业合同保持上游未消费的投入、将已结算产出放入仍有容量的缓冲，或原子拒绝新的上游承诺。已经发生的加工、损耗或运输不能被静默丢弃、瞬移、自动改道、无限堆积或伪装成下游完成；任何溢出、返工、报废或 salvage 只有在专业合同明确存在时才能按其有界规则结算一次，并向玩家说明代价和恢复选择。

当两个或以上已经接受且仍有效的下游意图共同消耗同一阶段执行位、边吞吐或目的缓冲容量时，它们进入同一权威分配域；`已接受` 不等于每个意图都已经取得容量。每个裁决边界必须基于同一权威快照评估完整争用集，并为各意图产生**完整分配**、专业合同允许的**部分分配**、**延期**或**拒绝**之一；所有 hold 与实际消费之和不得超过可用容量，未获容量的意图不得产生隐性 hold、sink、进度或排队保证。裁决必须使用已声明的稳定依据和稳定身份解 tie，不能由客户端、网络到达、Agent 重试或表现层顺序决定；产品层不冻结权重、配额或队列算法。

容量释放、hold 过期/撤销、消费者失效或前置恢复后，仍有效的延期意图只能基于新的权威快照重评；未消费 hold 至多释放一次，已消费结果不能伪装成可重新分配容量。玩家必须能读到自己意图争用的是哪个阶段/边/buffer、已持有与未满足数量、完整/部分/延期/拒绝结果、适用的顺序依据、下一次重评条件，以及等待、释放、减量/拆分、改道、替代来源、暂停或重新规划中真实可用的动作；不得泄露其他主体的私密细节，也不得展示没有权威依据的队列位置或 ETA。同一意图的重连、重复提交与 replay 不能刷新优先级或复制 hold/效果；专业 profile 必须给出可观察的重评、过期、终止或重新规划边界，不能让仍有效的意图在容量反复可用时被无说明地无限跳过。

跨多个 tick/cycle 的工业路径只有在专业 profile 明确声明时，才能对玩家形成 end-to-end service window；否则必须标为 best-effort，不能把 recipe duration、transit `ready_at` 或某一段 hold 拼成到期保证。声明窗口时必须创建独立于 candidate/batch/join/output bundle 的 canonical window identity，绑定起始事件、目标 settlement、权威 start/end boundary，以及 stage execution slot、edge throughput、destination buffer 与 terminal capacity 中每项 mandatory obligation。每项容量必须明确属于 held-through-completion/arrival、到边界重新校验的 conditional reservation，或 non-reserved；未实际持有的未来容量不能被展示为承诺。

Service-window profile 必须声明 none/soft/hard 中适用的产品语义与 on-time/at-risk/late/expired 处置。提交和每个 stage finish、transit launch/arrival、buffer admission、terminal settlement 边界都按当前权威状态重验剩余窗口与 mandatory reservations：hard window 无法成立时在下一不可逆 sink 前延期/原子拒绝；soft window 可以显式 late 继续，但不得伪装 on-time。开始前 expiry/cancel 只释放未消费 hold 一次；已经产生 WIP、transit、buffer 或 production effect 后，只能按 profile 继续为 late、pause、hold/quarantine、reroute/requote、reduce、defer/reject 或专业合同支持的补偿，不能抹除既有 sink、自动退款、静默延长窗口或遗留 orphan hold。

Power-dependent profile 必须为每个 stage/cycle/branch/terminal obligation 声明 power mode：`start-only sink`、`held-through-completion/arrival`、`boundary-revalidated conditional` 或 `non-reserved`。End-to-end hard/soft service guarantee 只有在所有 mandatory power obligations 已按声明模式纳入 window 时才成立；否则必须明确为 power-best-effort，不能用 `electricity_after`、battery runway、预计电耗或当前余额冒充未来 reservation。Submit 与每个声明的 stage/transfer/buffer/terminal 边界只按该 mode 重验 owner-held power；hard shortfall 在下一不可逆 sink 前延期/原子拒绝，soft shortfall 明示 power-at-risk/late/blocked，已经发生的 WIP/receipt 继续服从既有 pause/hold/quarantine/reduce/compensation，不允许 overdraft、隐式续租、自动 top-up 或虚构退款。

当产线存在权威维护状态或计划维护窗口时，排产前必须提供只读的维护取舍，而不是把维护压力留到事后 receipt。玩家至少能比较专业合同真实支持的 `maintain_before_run`、`run_at_risk`、`reduce_load` 与 `defer`，并读到维护目标/作用范围、追加成本与仍占用、预计产出与复查时延、故障/停机或产出损失风险、可撤回性及推荐理由；稳定窗口或在制结果受影响时，必须明确其影响。若当前没有可验证的维护真值，surface 必须显示 `maintenance_not_tracked`，不能用电力余额、battery runway 或 `unchanged` 伪造安全继续。预览不推进维护、排程、tick 或资源账本；提交按当前维护状态重验。若在 submit boundary 维护真值仍为 `unknown`，任何会产生 irreversible sink、停机/稳定影响或交付承诺的候选都不得保持 selectable，必须 atomic reject 或保持有界 pending（不得产生隐式 hold）并给出下一次权威复查；只有专业合同真实支持且明确标注 `at-risk/unknown` 的路径才可继续展示，且 preview 仍无 authority。前置漂移原子拒绝并保留原任务/未结算状态，成功只产生一次可追溯维护/排程结果，不自动续期、退款或宣称交付；重连、重复提交与 replay 不得复制维护 sink、停机结果或稳定里程碑。

每次权威 power admission、sink、hold、release、revalidation 与 unmet disposition 必须绑定 root operation、owning revision/segment、cycle/branch 与 baseline expected power，并至多结算一次；它不是材料或 terminal settlement。诊断与 review 要把 power available/held/consumed/unmet/unknown 分开，缺证据时不能写成 `0`，并把 insufficient power 归为 primary root、下游积压归为 secondary。共享 power 争用继续服从同一权威 allocation snapshot、稳定顺序与单次释放/重评规则，不能 over-hold 或靠 retry 插队；玩家只看到有权限的 power scope、mode、复查边界与 reserve/best-effort/restore/reduce/defer 中专业合同真实支持的动作。

Renewal/extension 不能由重连、重试、Agent 推荐或 replay 隐式发生；必须从 fresh authoritative state 重新校验 owner、candidate、route/spec、容量与当前 lifecycle，并以链接 parent 的 continuation/new commitment identity 产生一次结果。Runtime replay 使用已记录的 world-time/order 与 expiry/renewal disposition，不能按恢复时墙钟重算；每项 expiry、release、renewal 与 fresh-snapshot re-evaluation 至多生效一次，也不能刷新共享容量顺位。稳定 `W` 与 terminal/delivery claim 只按已声明 window policy 计数，生产准时但 transit/terminal 迟到时保留 production receipt，却不能获得 on-time delivery receipt、奖励或里程碑。

<a name="工业外部性与操作性缓释-profile（target-only）"></a>

#### 工业外部性与操作性缓释 profile（target-only）

工业运行目前已有输入、能源、物流、批次质量和终端结算的约束，但尚未定义某些工厂/配方持续运行是否会产生可归因的外部性，以及玩家何时应在吞吐与操作性缓释之间停下来作选择。本节只增加一个可选的产品承诺钩子：`industrial_externality` 由工厂/配方/operation 的专业 profile 声明，`operational_mitigation`（必要时也称 operational containment）是局部运行义务，不是新的 runtime 字段、污染模拟或系统性危机治理轨道。

- profile 只在专业 authority 明示作用范围、是否有外部性义务、mandatory mitigation/containment 条件、评估边界与当前证据状态时才成立；没有声明且未被要求采用 profile 的 legacy recipe/factory 保持原路径，不因本节追溯加门槛。已声明或被要求采用 profile 时，profile 本体、作用范围、容量或必要 authority 缺失、过期或无法证明，产品结果为 `unknown/blocked`，不可选择会产生不可逆 sink 的运行；不能从材料名称、batch contamination、Location radiation source、客户端缓存或 Agent 建议推断 profile。
- 在首个不可逆 sink 前，只读 `industrial_externality_preview` 可让玩家比较专业 profile 已声明的 `run_with_existing_mitigation`、`run_reduced_or_defer`、`hold_for_declared_treatment_or_legal_disposition` 或其他已支持动作。预览至少说明 externality scope/status、mandatory 与已占用/仍可用的 mitigation capacity、生产/终端影响、时间/空间/电力与机会成本、失败风险、`next_action` 与 `next_recheck`；预览不产生 sink、hold、清理、区域 credit 或生产进度。
- 提交必须从 fresh authoritative state 重验 factory、recipe/operation profile、owner/作用范围、mitigation capacity 与既有 output/terminal obligations。漂移、capacity 不足或 authority unknown 只能 requote、atomic reject 或 profile 明示的有界 pending，不得静默运行、降级、自动清理、改道、退款或发放区域 credit。只有 profile 明示且当前证据成立的路径，才能在同一原子结果中至多形成一个生产结果与一个链接的 externality/mitigation disposition。
- `industrial_externality` 是独立于材料 batch 的 quality/custody、byproduct identity/disposition、maintenance、production receipt 与 delivery/terminal settlement 的 operation facet；它不能静默改写材料数量/质量、终端价值、需求或稳定窗口。已有 WIP、in-transit、buffer、terminal-pending 必须沿原 root/revision lineage 取得一次 profile 支持的处置，不能因外部性预览或重连自动迁移；terminal-pending 的 quantity、recipient、destination、admission、capacity obligation 与 provenance 必须保留至 matching settlement 或一次受支持处置，不得静默改绑或提前 settlement。
- 玩家能在吞吐/目标进度与已消耗或持有的 mitigation capacity、时间、空间、电力、失败暴露及后续复查之间取舍；玩家不能直接生成/抹除 externality、豁免 mandatory mitigation、改变共享环境真值、领取区域权利或把 production receipt 当作清理、终端或治理结算。区域/系统性影响、共享影响权利与危机 containment 必须另行进入治理、M4/runtime 专业合同，不能由本 hook 隐式授予。

该边界保持 **world-first**：外部性及缓释事实来自同一权威世界历史，而非客户端估算；保持 **emergence-first**：运行、减量/延期与持有之间存在有代价的操作选择；保持 **persistent / auditable**：profile、作用范围、root/revision、处置与实际占用跨重连、恢复和 replay 延续；保持 **extensible**：未来 profile 可增加合法的局部义务而不改写既有批次、生产或终端 receipt。产品层只拥有玩家承诺与隔离边界；M4/domain 拥有 profile fact、capacity、externality ledger/impact receipt，`world-runtime` 拥有 event/schema、持久化、顺序与 replay，`game` 拥有选择节奏与机会成本，QA 拥有组合证据。

`test_tier_required` 至少覆盖一个 deterministic fixture：无 profile（legacy unchanged）、known-contained/mitigation available、mitigation capacity full、unknown 或 expired authority。须证明预览只读且确定；submit fresh revalidation 与 drift atomic reject；known path 至多一次 production + 一次 linked disposition；reduce/defer/hold 的吞吐、buffer、时间与复查成本可读；外部性不改变 quality/custody、byproduct、maintenance 或 production/delivery finality；重复 submit、重连、Agent retry、snapshot restore、事件乱序与 replay 不复制 production、externality、cleanup、progression 或 reward；Viewer、pure API 与 Agent 对状态、root blocker、成本、动作和下一步同义。`test_tier_full` 的多阶段、长期与区域影响证据必须由未来 M4/runtime/economy/governance/QA 专业合同另行定义。

本节的 Non-Goals 是：不规定污染、温度、环境、积累、衰减、扩散半径、阈值或生态公式；不规定清理经济、税费、区域 credit、治理/charter/claim 权利或 crisis containment；不新增 runtime/API/ABI/schema/UI/Agent 行为，不提供自动缓释、清理、降级、改道、退款或补偿，也不声明当前工厂/配方已有 externality evidence 或 release readiness。profile-less legacy 路径不追溯受此 hook 阻断。

工业性能与瓶颈诊断只能作为 read-only derived snapshot，不能由 Viewer 采样、客户端计时或运营面板成为第二套世界真值。每个 measurement/review window 必须绑定 canonical identity、candidate/recipe/factory/stage/edge/buffer/terminal config epoch、权威 world-time start/end boundary，以及 journal/state-root anchor；窗口只能在声明的 operation/window completion 或 checkpoint 关闭。Cutover 必须关闭旧候选窗口并为新候选开启新 identity，除非存在显式 handoff/conversion lineage，否则不得跨候选回填、合并或倒推指标；缺少 frozen plan 或 receipt 证据时显示 `plan_unavailable / unknown / not_tracked`，不能伪造为 `0`。

`plan baseline` 只能在权威 submit/allocation 边界从同一 canonical snapshot 创建一次，不能由 preview、quote、request、speculative hold 或表现层缓存生成。每个已经接受的 full/partial/deferred/denied 结果必须绑定 immutable baseline identity/revision、intent/candidate/config epoch、batch/join/output-bundle/service-window identity、snapshot/journal anchor，以及 requested、admissible、committed/allocated、executable、unmet/residual 数量和当时声明的 input/power/output/byproduct/stage/edge/buffer/terminal obligations；原子拒绝且未形成 accepted intent 时不创建 baseline。Deferred/denied baseline 记录当时计划结果但不代表取得容量、产生 sink 或进入 actual。

每个已经接受的流水线路径结果还必须在同一 submit/allocation 边界创建一次 immutable root operation identity；原子拒绝且没有 accepted intent 时不创建。Stage execution、input join、output bundle/branch、edge handoff/transit、buffer/terminal admission、service/measurement window、baseline revision、checkpoint、专业 product/module validation 与 production/delivery/compensating receipt 都必须同时绑定该 root、owning revision/segment 与直接 parent/child role。两个独立 accepted intents 即使 candidate、recipe、owner 与数量相同，也不能共享 root；fan-in/fan-out child 不能自行生成无 parent 的新 root，材料名称或 Viewer 推断不能替代权威链接。

Checkpoint continuation 与非因果 plan revision 保持同一 root，并创建链接的 child segment/revision；因果 cutover 创建 parent-linked child root/new candidate，不能继承旧 receipt、稳定 `W`、里程碑或 actual。每条 receipt 只能归属一个 root、owning revision/segment 和 child step/branch；缺失或冲突链接必须在 sink、credit、progress 或 reward 前 fail closed 为 `operation_identity_unavailable/unknown`。Root 在 terminal settlement 或显式 abort 后关闭，新的迟到效果只能服从既有 pending/compensating 合同，不能继续附着成普通完成；retry、reconnect、乱序、恢复与 replay 必须重读相同 root/child/finality，不能复制 sink、receipt、指标或奖励。

专业 Product/module validation 必须区分三层：quote/preview 只读且无 authority；validation decision 是 evidence-only child receipt；production/delivery settlement 才能产生材料 credit、用途结算或 finality。Validation decision 本身不得 sink/credit 材料、推进 `W`、减少需求、发奖或冒充 settlement。Profile 必须声明该 gate 对当前 root/branch 是 mandatory 还是 advisory、发生在 submit/start/completion 哪个边界，以及适用的 freshness/drift policy；advisory 结果不能被表现层包装成强制解锁或保证。

每个 decision 必须以 root+owning revision/segment+cycle/branch+validation epoch 唯一，绑定 exact parent batch/receipt、evaluated quantity、target stage/recipe/factory profile/config/spec epoch、validator/module release 与 proof/state/journal anchor，并得到 `applicable / not_applicable / pending / unknown / expired` 中一个权威结果。Mandatory gate 只有在 parent 已 settlement/arrival 且存在完全匹配的 `applicable` decision 时才能进入第一个 downstream sink；decision-only、缺失、负向、pending、unknown、expired 或 wrong-root/stage/batch decision 都 fail closed 为 hold/defer/atomic reject。Batch/spec/owner/ledger/route/profile/candidate/module drift 只使旧 decision immutable superseded，并按 profile 创建 linked child revalidation 或保持 pending/reject，不能改写旧 receipt、跨 root 复用或自动降级。Retry、恢复与 replay 重读同一 decision；output-bundle atomic/split policy 对每个 branch decision 至多消费一次。

已经 journaled 的 baseline 不能被后续库存/容量/profile 漂移、重连、retry、replay、reroute、renewal、reduce 或 replan 改写。专业合同支持调整时，必须在单一权威边界创建链接 immutable parent 的 child revision/new review，并把边界前后的 receipt/actual 分别归入其 owning revision；因果 recipe/factory/edge/terminal 变化继续触发现有 cutover/new candidate/`W=0`，非因果计划修订可以保留 candidate，但仍须分开 baseline 与 review。缺失或失效的 anchor 只能得到 `plan_unavailable/unknown`，不得用当前配置回填旧计划；重复提交、恢复或 replay 必须返回同一 baseline/revision/outcome，不能复制 hold、sink、指标或奖励。

Measurement checkpoint 只能由专业 profile 在声明的合法原子边界创建。每个 canonical checkpoint identity 必须绑定 operation/window、plan baseline/revision、权威 world-time/order、journal/state-root 与当时未决的 WIP、transit、buffer、terminal、hold 和 obligation；不能由 Viewer refresh、玩家反复打开 review 或任意 tiny window 生成，也不能切开尚未结算的 atomic join、executable cycle、output bundle/branch 或 source-to-destination handoff。边界暂不合法时，checkpoint 必须等待整个原子效果仍保持 pending 或完成后再关闭，不能生成半份 receipt、sink 或 actual。

Checkpoint 只关闭一次 immutable review segment，不追加、不重开，也不释放/续租 hold、改变稳定 `W`、发奖、结算交付或产生其他世界效果。Operation 尚未完成时，下一 segment 必须链接 parent checkpoint/review，并把边界上的 accepted backlog、join_pending、WIP、in-transit、buffer/terminal pending 与 holds 作为 opening state 延续，而不是重新计为 actual、重新接受或重新分配；默认保持同一 operation/baseline revision，只有显式 rebaseline/replan/cutover 才按既有 parent-linked revision 或新候选规则切换。缺失、冲突或不可验证的 checkpoint anchor 不得关闭窗口，只能显示 `checkpoint_unavailable / unknown / not_tracked`；retry、reconnect、恢复与 replay 必须返回同一 checkpoint、bucket 与 continuation disposition。

计划侧必须区分 requested、admissible、committed/allocated 与 executable units，实际侧必须区分 executed cycles、production-settled 与 delivered/terminal-settled；deferred、denied、unmet 与 residual 不能计入实际产出。同一 commitment/batch 在窗口边界只能落入一个 disjoint bucket：accepted-unstarted backlog、join_pending/held、active WIP、in-transit、buffer-held、terminal-pending 或 settled，并保留 lineage，不能在状态迁移时重复累计。Actual input sink、主产物/副产物、运输损耗、rework/salvage/return/compensation 与 unmet/residual 只从链接的 settlement receipts 对账；production 与 delivery 继续分开。

专业 profile 可以定义合法的 yield/loss/utilization 分类与 denominator 来源，但产品层不冻结公式或目标值。诊断至少能区分 stage slot 的 busy/idle/blocked、edge 已用与可用 throughput、buffer occupancy/peak/held、terminal held/admitted，并把 accepted-but-unstarted 与 WIP 分开。Bottleneck attribution 以 lineage 中最早的 canonical unsatisfied stage/edge/buffer/terminal blocker 为 primary root，派生缺料/积压标为 secondary；多个原因使用稳定 precedence 并保留 provenance，root 变化开启新的 attribution segment。重复 review、refresh、retry、reconnect、replay 或 compensating receipt 不得重复指标、改变 W/lease/receipt、发奖或产生世界效果；相同 journal、边界与 state-root 必须重建同一 snapshot。

改变展示或不影响产出因果的元数据不应误触发换线；改变阶段、配方、设施能力/版本、边目的地或其他消费资格与产出因果的变更，则必须创建新候选身份，并在一个权威 cutover 快照上逐项处置旧候选的**已预留**、**加工中**、**在途**与**已入缓冲**物料。边界前形成的 hold、在制品、运输承诺、批次与 receipt 继续绑定旧候选，边界后的新意图只属于新候选；材料同名、目的相同或表现层推荐都不能把旧状态静默重绑给新候选。新候选不得继承旧候选的稳定窗口、里程碑、排队资格、reservation 或 receipt。

每项旧状态只能取得一次可追溯处置：未消费 hold 可按专业 profile 保持、释放、延期或拒绝；加工中物料只有在旧前置仍有效且 profile 明确支持时才能排空/完成，否则进入暂停、终止、返工或 salvage 中真实受支持的结果；在途物料保留原边、目的账本、规格与已发生损耗，边失效时只能通过显式 return、hold、拒绝或新的 transfer/handoff 处理；已入缓冲批次继续保留旧 lineage，新候选仅能在显式兼容/迁移合同下重新校验后消费。没有专业处置能力时必须 fail closed，不能静默混入、重贴标签、瞬移、自动改道、同时退款与完成，或形成半旧半新的部分状态。若支持迁移/转换，必须产生一次链接旧 receipt、数量、损耗、变更原因与新候选身份的 conversion receipt。

换线前的玩家反馈必须比较当前真实可用的排空旧在制品、完成并交付、隔离、返工、salvage、放弃或延后换线；只有专业容量明确支持时才能提供并行新候选。反馈至少说明旧/新候选身份、当前阶段与 lineage、已消费/仍占用的投入和阶段/边/buffer 容量、保留与损失、交付延期、稳定窗口从 `0` 重启，以及下一复查点。重复换线、重连、重试与 replay 不得重复释放、交付、返工、salvage、退款或刷新优先级，也不能把旧产出或完成 receipt 计入新候选窗口/里程碑。

每条流水线的 terminal stage 必须声明有界的终端处置：进入 owner-bound 的有限 product ledger/buffer、进入具备明确 recipient/purpose 的交付承诺，或进入专业合同支持的 hold/quarantine。**生产完成不等于交付结算**：production receipt 只证明最终产物按实际数量与损耗进入合法终端账本或显式在途承诺；只有独立的 delivery/settlement receipt 才能减少需求、结算市场/服务/区域用途、发放交付奖励或完成终端里程碑。中间品边的 destination buffer、表现层的“已完成”标签或交易推荐不能隐式替代终端 owner、容量、资格与结算条件。

终端处置必须在排产前说明目的账本/recipient、可读的容量与已占用量、产品适用性/交付资格、生产后保留库存、交付是否仍待决，以及持有、预留容量、减量、改道、转本地用途、重报价或延期中真实可用的动作。终端容量不足、owner/recipient 失效、需求过期或路线不可用时，最终阶段只能按专业 profile 在产出 sink/稳定进度前阻塞或原子拒绝，或把已经合法生产的产物保持为有界、可追溯的未准入/待交付状态；不得无限堆积、免费销毁、自动转卖/降价/改道、静默没收，或把 production receipt 冒充 delivery receipt。稳定候选必须声明 terminal admission 是否属于 `W` 的有效周期条件：若属于，未准入周期不计进度；若只验证 production，则只能报告 production-stable，不能宣称已经交付或结算。

#### 生产结算—终端交付状态闭环与奖励门控

`production-settled` 是 production receipt 证明的事实，不是可以替代库存/交付状态的互斥 bucket。产物完成生产后，必须继续落入当前 root/owning revision 下的 `buffer-held`，或在明确终端准入/运输承诺下进入 `terminal-pending`；只有独立、作用域匹配且仍有效的 delivery/settlement receipt，才能把对应数量转为 `delivered/settled`，减少需求、发放交付奖励或完成终端里程碑。Production receipt、buffer admission、terminal preview/acceptance 或“生产完成”表现标签都不能提前触发这些效果，也不能把同一数量同时计入 buffer、pending 与 settled。

玩家必须能从这条闭环比较当前 profile 真实支持的 `deliver_to_terminal`、`hold_in_output_ledger`、`route_to_next_stage`、等待/预留容量、改道、减量或延期，并看到 primary blocker、仍被占用的数量/容量与机会成本、预计结果/复查边界、失败损失、可撤回性和成功后打开的下一工业用途；未声明的动作不得伪装成可选恢复。终端容量、owner/recipient、资格、需求或路线在生产后失效时，已合法生产的数量与 provenance 必须保持，只能进入有界 pending/hold、原子拒绝新的交付尝试，或执行 profile 支持的一次性 return/quarantine/改道处置；不得销毁、免费 credit、自动退款/降价/改道或发放交付效果。若 terminal admission 属于 `W` 条件，`terminal-pending` 不计稳定进度；production-only profile 可以报告 production-stable，但不能标记 delivered/finalized。重复 submit、arrival、乱序到达、重连、Agent retry、snapshot restore 与 replay 对同一 root+receipt 至多产生一次 production、delivery、需求减少、奖励、里程碑与 hold release，并且 Viewer 与 pure API 必须返回同义的 bucket、根因和下一步。

#### 需求目标驱动的生产批量与停产闭环

批量可执行不等于该批量值得生产。代表性订单、区域用途或玩家工业目标必须把一个可审计的 `demand goal` 与当前 factory/recipe candidate 的排程、生产和交付结果关联起来，使玩家在提交前能回答“还需要多少、这一批会缺多少或多多少、交付后应补产还是停止”。本节是 demand-goal identity、数量分层与目标处置的产品 authority；每种目标 profile 必须通过唯一 `goal_authority_ref` 指向一个拥有其 purpose/recipient、目标数量或合法单位、有效范围/窗口、取消/到期规则与满足边界的专业 authority，M4 成本/价值归因和 runtime receipts 均不能成为第二个目标来源。缺失、过期、冲突或未追踪的目标显示 `demand_unknown / not_tracked`，不能由客户端、Viewer、Agent 推荐、市场热度或旧订单推断成无限需求。

只读 `production_goal_disposition_preview` 必须绑定同一 demand-goal identity、`goal_authority_ref`、factory/recipe candidate、plan revision、canonical executable unit 与 authority snapshot，并区分 `target`、`already_committed`、`production_settled`、`delivery_settled`、`terminal_settled`、`remaining_demand` 和本次 `expected_executable_output`。`delivery_settled` 与 `terminal_settled` 是独立权威事实，不得合并计数；目标 profile 必须声明其中哪一个 matching settlement boundary 能减少该目标的 remaining demand，另一层只能保持可读但不产生第二次满足效果。预览至少说明合法批量/取整后预计产生的 `shortage / matched / surplus / unknown`、缺口或盈余数量、仍会占用的输入、电力、物流、buffer/terminal 容量、主要机会成本、目标/容量/价格等需要重新核验的边界，以及下一次决策点；production receipt 仍不能减少需求。产品层不冻结预测、价格、容忍阈值、批量公式或 runtime 字段。

玩家只能比较当前专业合同真实支持的 `produce_full`、`produce_reduced`、`replenish_or_transfer`、`hold_surplus`、`stop_or_defer` 与显式 `supplemental_plan`。当目标为 `demand_unknown / not_tracked`、expired 或 authority conflict 时，不得计算 `matched/surplus`，也不得把生产、补产或旧 schedule 作为 selectable action；surface 只能提供建立/选择有效权威目标、保留既有合法产物、停止或延期等当前真实路径，目标恢复后从 fresh snapshot 至多重报价一次。目标尚有缺口时，补料、调运、合法减量、延期或 parent-linked 补产计划必须说明保留进度、追加成本、预计可交付量和下一复查点；目标已满足时，新的不可逆生产 sink 必须等待显式继续用途或新的 demand goal，不能靠后台循环、Agent retry 或旧 schedule 自动追加。合法批量不可整除而产生盈余时，必须在提交前披露其库存/终端占用与处置风险，并只允许 profile 已支持的持有、本地用途、改道、换配方或停止；不得自动倾销、免费销毁、伪造成交、回填旧需求或把盈余包装成额外成长。

提交时必须从 fresh authority state 重验 demand goal、已承诺/已交付量、输入库存、executable unit 与输出/终端容量。报价后目标被取消、缩小、满足或过期，或者批量、产率、库存、路线和容量发生漂移时，只能重报价、在首个 sink 前原子拒绝，或在 profile 明示的有界 pending 下等待新的显式决定；不得静默缩放、继续生产或把旧 baseline 改写成新目标。Profile 唯一声明的 matching settlement（delivery 或 terminal）不足而形成补产时，必须创建链接原 demand goal 与结算 receipts 的 parent-linked supplemental revision，保留原计划、实际产出、损耗、已满足量与剩余缺口；非 matching settlement 不减少需求。重连、重复提交、乱序、restore 与 replay 只能重读同一 shortage/surplus/stop/supplemental disposition，不得复制生产、交付、需求减少、盈余处置、奖励或容量释放。

`test_tier_required` 至少覆盖：同一 demand goal 下 full 与合法 reduced batch 的只读比较；产量小于目标时显示剩余缺口，选择停止/延期只保留 shortage，选择补产则同一显式补产决策恰好创建一次 parent-linked child revision；产量等于目标时 profile 声明的 matching delivery 或 terminal settlement 将目标转为 matched，另一 settlement 不重复减少需求，后续批次必须显式 stop/hold 或绑定新用途；合法批量大于剩余目标时披露 surplus 与 buffer/terminal 占用，减产、持有或停止不产生自动销售/销毁/奖励；production-only receipt 不减少需求；`demand_unknown / not_tracked`、expired 与 conflicting authority 各自不产生 matched/surplus、生产/补产 selectable action、需求减少或奖励，恢复后只从 fresh snapshot 重报价一次；目标、库存、batch quantum、产率或终端容量在报价后漂移时无副作用拒绝或重报价；重复 submit/delivery、重连、乱序、Agent retry、snapshot restore 与 replay 不复制任何效果。Viewer、pure API 与 Agent 必须对目标、已承诺/生产/交付量、缺口/盈余、primary blocker、机会成本、下一动作和复查点保持同义。`test_tier_full` 再覆盖多个并发 demand goals 的数量归属、跨窗口目标到期/取消、三阶段多批次与运输损耗、partial matching settlement（delivery 或 terminal）、补产 revision 链、buffer/terminal 争用、持久化恢复与补偿 receipt。该闭环不新增自动需求预测、补货/停机策略、订单簿、价格/利润公式、runtime schema、队列、公平性或 UI 布局，也不宣称当前实现已完成。

跨 stage/ledger handoff 的 preview、accepted/hold/allocation、in-transit/pending、released/rejected 都只是决策或待决证据，不能被 Agent、Viewer 或客户端提升为 settlement。每个 source -> edge -> destination operation 必须以同一身份和有序 journal 收敛为“有效 source effect + 至多一次 destination settlement”、明确 pending/hold，或原子拒绝/无效果之一；故障、乱序/重复到达、重连、retry 与 replay 不得造成来源已扣而目的丢失、目的免费 credit、pending 与 settled 并存，或重复下游 sink/奖励。若专业合同支持纠错/补偿，必须新增可追溯的 compensating receipt，不得修改历史 settlement 或用陈旧 receipt 解锁下游。

每个 recipe + factory capability 必须声明 canonical executable production unit，以及是否支持按合法 quantum/cycle 部分执行；材料批次、玩家请求量或共享容量的 partial allocation 都不能自动等同于可执行的 partial recipe。提交前与结果必须区分 requested、当前 admissible、committed/allocated、executable、实际 executed 和 unmet remainder。Full-only recipe 在输入、能源、阶段/边容量或终端准入不足一个合法执行单位时，只能在首个 sink 前延期或原子拒绝；不得静默降额/取整、部分扣料、生成半批产物/副产物、留下隐藏欠费或提前计入稳定进度。

只有专业 profile 明确支持 partial 时，才能按其声明的最小/合法单位、确定性量化/取整和 residual 处置执行；余量只能保持、返还、按合同发生有界损耗或原子拒绝，不能由客户端/Viewer 浮点取整、隐式生成/销毁材料或改变配方。共享容量的 partial 结果必须再次通过 batch admissibility，不足合法单位时仍是 deferred/denied。Settlement 必须按实际完成的 canonical cycles 记录输入 sink、主产物/副产物总量和 unmet/residual；副产物目的账本/容量不适用时按既有 hold/block/reject 规则处理，不能免费丢弃或把原请求量重复结算。批量大小本身不创建新候选；一个 `W` 计数单位只在合法执行 cycle 完成且满足已声明 output/terminal 条件时成立，将等价工作拆成小批不得增加稳定窗口、里程碑、容量优先级或奖励。

一个 executable cycle 同时产生主产物与一个或多个副产物时，必须创建单一 canonical output-bundle identity，绑定 candidate/recipe/cycle、完整输出 branch set、稳定 branch order、每条 branch 的数量、目的 ledger/edge/terminal route 与 parent receipt。默认 fan-out policy 是 **all-output atomic**：在第一个 output credit、production progress 或稳定计数前，必须从同一权威快照预检所有 branch 的 owner、适用性、容量与路线；任一 branch 不成立时，整个 bundle 只能延期、原子拒绝或进入 profile 明确支持的有界 hold/quarantine，不得先结算主产物再丢失/阻塞副产物，也不得静默改道、合并 branch 或把 production receipt 冒充 delivery。

只有专业 profile 显式支持 split fan-out 时，才允许各 branch 独立 settlement；profile 必须声明 parent bundle 完成条件、已结算/待决 branch、failed-leg residual 处置和对 stable/terminal 进度的影响。每条 branch 至多结算一次，未决 branch 的容量/资格恢复只从 fresh state 重评该 branch，不得重跑已结算 branch、再次扣输入或复制主/副产物、需求减少与奖励。玩家必须能读到 bundle 中每条 branch 的用途、数量、owner/destination、容量/资格、held/unmet 与 production/delivery 状态，并比较等待、扩容、减量、专业合同支持的改道/本地用途或延期；没有显式 split policy 时不能把部分成功包装成完成。

阻塞必须保留因果方向：surface 至少指出根因所在阶段/边、受影响的下游阶段、当前中间品数量与状态、上游是否因背压继续或暂停，以及真实可用的等待、补料、扩容、修复、改道、降载或重新规划动作。规格/品质判断至少要让玩家区分**适用**、**待验证/证据不足**与**不适用**，并指出不满足哪个阶段/边的消费条件；待验证或不适用不能伪装成普通缺料、在后台降级为较低规格，或自动转换、混料、报废。隔离、复验、返工、替代配方/去向、继续持有或 salvage 只有在专业合同明确支持时才是可选恢复动作，并须披露实际损耗、时间/容量机会成本与后续资格；不存在该能力时必须显示真实阻塞或放弃路径。派生的下游缺料不能覆盖根因，也不能把多个阶段压成一个无法定位的“生产失败”。玩家的取舍应围绕吞吐、缓冲占用、交付时机、批次适用性与恢复弹性；只让同一批次反复验证、加工、重连或重放而没有新增能力、选择或世界用途，不构成成长或新的稳定流水线里程碑。

同一批次跨阶段的 lineage 必须在持久化、恢复和 replay 后仍能关联各阶段承诺、边、中间品数量/预留、父级 receipt、规格/品质适用结论及其来源、实际损耗与 blocker。重复提交、Agent 重试、重连或事件重放对每个阶段至多产生一次 sink、产出与里程碑效果，也不能把未知/不适用批次重试成适用；已完成阶段可以按专业合同从已结算结果继续，但不得重新发奖、复制中间品或跳过尚未满足的下游前置。`game` 拥有阶段节奏、容量取舍、规格/品质带来的用途选择、返工收益和 anti-grind 平衡；`world-runtime` 与工业模块拥有图、账本、批次属性、适用性校验、预留、状态、守恒、去重与 replay 合同；Viewer 与 pure API 必须表达同一根因、适用结论、背压和下一步。产品层不冻结品质数值/公式、buffer 数值、配方、并行度、队列/图算法、runtime 枚举或 UI 布局。

#### Starter 工业可行性闸门与 current/target 证据切线

首局或首次持续能力的工业目标在成为可执行推荐前，必须先通过一次 `Starter Industrial Feasibility Gate`。闸门只从同一份 fresh authority snapshot 判断当前是否存在一条到 profile 声明完成边界的安全 starter chain；它是玩家可读的产品决策结果，不是 runtime 状态、字段或新任务系统，不创建目标、不锁定资源/容量、不改动账本、不自动排程，也不保证产量、价格、ETA 或长期稳定。闸门顶层结果只有 `candidate_available` 与 `no_safe_starter_chain`，不能把 target contract 或未知事实包装成可行。

本阶段唯一的 canonical completion profile 是 `starter-industrial-smelter-to-assembler-v1`。该 profile identity（及其 profile revision）是产品语义的 join key，不是 runtime enum、配方解锁树或新的任务类型；runtime、Viewer、pure API 与 Agent 必须消费同一 profile revision 的闸门结果，不能各自维护一套 starter 前置判断。该 profile 的 current chain 是：在当前合法地点建成 `factory.smelter.mk1` → 以 `recipe.smelter.iron_ingot` 执行至少一个合法生产 cycle → 在该 starter Smelter 的 owner-bound output ledger 形成一次已结算、实际数量大于零的 `iron_ingot` production result → 允许把 `factory.assembler.mk1` 作为下一工业候选。profile 的 completion boundary 是 **starter Smelter 的首个 iron-ingot production settlement**；它不要求 delivery/terminal settlement，因此只能打开 assembler 候选/建设资格，不能宣称交付、需求满足、稳定产线或其他成长奖励。材料名称、同名配方、accepted/queued、preview、latest snapshot 或仅有计数器都不能替代该匹配的 settlement 语义。

闸门的 `fresh authority snapshot` 必须是在本次评估/提交前从同一 canonical world revision 重新读取的组合事实，至少覆盖 factory/site、recipe/version、输入批次适用性、power、物流/容量、output ledger 与 profile revision；各事实若不能证明属于同一 revision，或只来自客户端/Viewer/Agent 缓存、旧 quote、target-only 合同、已压缩的孤立历史记录，则结果为 `unknown/not_tracked`，按 `no_safe_starter_chain` 处理。fresh 不规定跨模块实现的字段名或时钟阈值，而规定 authority snapshot 的一致性、可追溯性和提交时重验边界；报价或上一次闸门结果不能延长其有效性。

该 profile 的 milestone 是 profile-scoped、单调且可恢复的产品事实：匹配 completion boundary 一旦由 canonical production settlement 证明，即使 bounded receipt history 被压缩、工厂的 latest-completion snapshot 被后续配方覆盖、发生重连/恢复/replay，也不得撤销 assembler 候选资格或要求重复首产。该事实必须与 settlement 在同一权威结果中至多写入一次，并独立于有界 receipt 列表、latest snapshot、Viewer 缓存和 `completed_recipe_jobs` 等聚合计数保持可验证；legacy snapshot 只有在能回溯到同一匹配 settlement 时才能 backfill，不能由材料余额、同名 recipe、当前计数或推荐推断。profile 版本、factory、recipe、output ledger、root/lineage 或 completion boundary 发生因果变化时，必须产生新的 profile candidate 并从 `W=0` 开始；不得把旧 milestone 移植给新候选。该持久语义不冻结具体存储字段，但所有 surface 必须对其返回同一结果。

玩家读面必须把 profile identity、profile revision、authority snapshot/version、completion boundary 和结果一起呈现。`candidate_available` 还必须给出当前证据支持的完整 chain、首个可验证成果（首产 iron ingot）、实际 `progression_effect`（仅打开 assembler 候选/建设资格）、`next_action` 与 `next_recheck`；`no_safe_starter_chain` 必须给出最早 primary blocker、`current/target/unknown` 证据分类、已保留/消费价值、可执行的补料/补证/等待/修复/改道/换候选路径、`next_action`、`next_recheck`，并明确 `progression_effect=none`。若没有真实安全恢复，必须停止并保持该结果，不得用后台重试、自动补料或推荐排序伪造可行链。上述是跨 surface 的单一产品结果契约，不冻结 wire schema、UI 布局、配方数值、队列或 runtime 实现。

这里的闸门结果是“整条 starter chain 是否可进入首局推荐”的产品层结果，与后文 recipe candidate discovery 的 `candidate_available`、`candidate_blocked`、`candidate_unknown` 三类候选结果不是同一层级；后者仍负责解释单个配方候选，不能单独证明整条 starter chain 可达。

闸门必须同时核对当前工厂/地点能力、当前可用且适用的 recipe candidate、原料来源或精炼结果、有效物流路径与容量、电力前置、产物接收/终端用途及 profile 声明的 completion boundary。只有这些事实均有当前证据，且每个所需阶段/边可执行时，才可返回 `candidate_available`；链路不存在、任一不可逆前置无安全恢复，或必要事实仍是 target-only/unknown 时，返回 `no_safe_starter_chain`。多输入 join、多阶段 root/window、output bundle、terminal settlement 或 maintenance 只在对应专业合同已有 current evidence 时才能成为 starter 前置，不能因为 walkthrough 或 target PRD 提到它们就默认可用。

证据切线只表达产品承诺范围，不改变专业 authority 的实现状态：

| 证据分类 | 产品含义 | Starter 闸门行为 |
| --- | --- | --- |
| `current-evidence-backed` | 当前专业合同与 fresh evidence 已共同证明该事实及其完成边界 | 可以进入候选链，但仍须在提交时重新校验 |
| `target-contract` | 已定义规则/目标，但尚无当前实现或组合证据 | 不能作为当前 starter 前置；只能说明未来能力或后续复查方向 |
| `unknown/not_tracked` | authority、状态或适用范围不足以判断 | 按 `no_safe_starter_chain` 处理，保留未知根因，不以零成本、无限容量或安全可达填充 |

`candidate_available` 必须让玩家看到稳定的 starter-chain/candidate identity（或等价的不可变上下文摘要）、所绑定的 authority snapshot/version、工厂/配方/输入/物流/电力/输出边界、主要风险、completion boundary、首个可验证成果、即时收益、`progression_effect`、`next_action`、`next_recheck` 与完成后打开的下一 beat；identity 必须同时绑定该链的产出因果前置，多个候选不得被静默选定，推荐理由必须来自同一快照。首个可验证成果只表示该链真实完成了声明的第一个世界结果，不能提前发放稳定/交付成长；即时收益必须是玩家因这次选择保住的能力、用途或下一选择，不能是后台完成或免费资源。`no_safe_starter_chain` 必须指出最早且可行动的 blocker、它属于 current/target/unknown 哪一类、已保留或已消费的价值、真实可用的补料/补证/等待/修复/改道/换候选/重新定目标路径，以及下一次复查边界；该结果不产生工业成长奖励、progression_effect 或下一 beat，只有恢复后以 fresh snapshot 重新通过闸门，才能再次进入目标。没有安全恢复时必须安全停止，而不是无限等待、自动补料、后台改道或发放免费输入。

闸门只负责接受前的可行性判断；玩家确认后仍须按既有工业 lifecycle、quote、receipt 与 fresh revalidation 合同执行。提交前任何工厂、配方、输入、路线、容量、电力或终端事实漂移，或该链的 authority snapshot/version 失效，都必须重新运行闸门或原子拒绝，不能保留旧 starter-chain/candidate identity、静默切换到另一条链或沿旧 identity 发放 progression_effect。`production-only` profile 可由匹配的 production receipt 完成一次首产物目标并标为 `produced/undelivered`；稳定条件只决定何时升级为 `production-stable`，不得延迟或重复首产物完成；声明 terminal-admission/delivery 的 profile 必须等待匹配的 delivery/terminal settlement receipt。重复请求、重连、Agent retry、snapshot restore 与 replay 只能重读同一闸门/receipt 结果，不产生第二次资源效果或目标完成。

该闸门保持 **world-first** 的真实工厂、材料、物流、电力与终端约束，保持 **emergence-first** 的来源/配方/路线取舍，保持 **persistent/auditable** 的 candidate、目标与 receipt lineage，并为未来专业 profile 保留 **extensible** 的能力切换边界。`test_tier_required` 与 `test_tier_full` 是验收目标，不是当前 pass 或当前可用性声明；在 fresh composite runtime + QA evidence 证明 Gate 与 starter chain 之前，任何 surface、产品结论或 release 说明都不得宣称 Gate/current starter chain 已实现或默认可用，缺少该证据必须按 `no_safe_starter_chain` 处理。验收目标至少覆盖一条 current-evidence-backed 正向链、首个可验证成果/即时收益/`progression_effect`/下一 beat、稳定 identity 与 authority snapshot/version 绑定、工厂/配方/原料/物流/电力/输出各类 blocker、target-only 与 unknown fail-closed、报价后事实漂移、生产与交付边界，以及重复/重连/replay 无副作用；`test_tier_full` 再覆盖多候选容量争用、跨窗口/多阶段链、production-only 与 terminal-admission profile、持久化恢复和 Viewer/pure API 同义结果。该补充不新增配方、数值、runtime schema、队列、自动补给/改道、UI 布局或当前实现声明。

#### 工业流水线生命周期到玩家决策的收口

现有工业合同分别定义了阶段、批次、物流边、配方版本、工厂能力和终端结算，但这些事实必须在产品层收束为一个可执行的玩家读法；玩家不能从 job、ledger 或 receipt 名称自行推断“现在能做什么”。对每个当前工业目标，正式玩家 surface 与 pure API 必须从同一权威快照提供以下四类产品结果（这是读面分类，不是新的 runtime 状态或字段）：

| 产品读面 | 必须让玩家看懂 | 不能误导成 | 当前决策边界 |
| --- | --- | --- | --- |
| `准备/待开工` | pinned recipe/capability、输入批次适用性、来源账本、物流边/容量、电力与终端前置，以及尚未消费的占用/缺口；工厂已建成但没有 active/作用域内 recipe 时必须明确显示 `factory-ready/recipe-missing` | 已取得产能、已开始或已计入稳定进度；`BuildFactory` receipt 不能冒充 recipe 已启用 | 选择补料/补证、等待容量或配方准入、修复前置、改用当前合法候选、延期或放弃；只有专业合同支持的动作可展示 |
| `运行中` | 当前 stage/WIP、已消费与仍占用的批次/容量、运输或缓冲位置、primary blocker 与下一复查点 | 已交付、已满足需求或必然按时完成 | 继续/等待、修复、改道、降载/拆分、重新规划或受支持的中止/处置；不得把重试当作新授权 |
| `产出待交付` | production receipt 的实际主/副产物、损耗、目的账本/终端、owner/recipient/资格与剩余交付前置 | delivery/settlement、需求减少、交付奖励或终端里程碑 | 等待/取得终端容量、交付、持有、改道/重报价或延期；没有 delivery receipt 时，只有 `production-only` profile 可由匹配 production receipt 完成一次首产物目标并保持 `produced/undelivered`，稳定条件另行决定 `production-stable`，声明 terminal admission/delivery 的目标仍不能完成 |
| `已结算` | 与当前目标匹配的 production/delivery/terminal receipt、实际结果、保留库存、稳定窗口与可打开的新用途 | 可以重复消费同一结果或重复获得成长 | 只在 profile 支持的范围内比较继续稳定、扩容/服务、合法换线或下一个目标；重复查看、重连和 replay 不产生第二次进度 |

每个读面至少同时说明 `factory/capability + recipe version`、关键输入批次/适用性、有效物流边、power/maintenance 风险、output bundle/副产物处置、`primary blocker`（无则 `none`）、已占用或已消费价值、`next_action`、`next_recheck` 与 `progression_effect`。任一权威事实缺失或漂移必须显示 `unknown/degraded`，不能以零成本、空缺口或“已完成”填充；`accepted`、`produced`、`buffer-held` 和 `terminal-pending` 都不自动打开下一个玩法目标。

`BuildFactory` 完成到第一次可排产之间必须存在一条明确的产品桥：建设 receipt 只证明工厂能力/设施结果，不自动授予 active recipe、原材料、物流容量或生产奖励。工厂处于 `factory-ready/recipe-missing` 时，玩家可比较当前专业合同支持的配方准入/启用、补齐输入与物流/电力前置、等待治理/证据、改用当前合法 recipe 或延期；配方准入/启用的 preview 只读，不产生 recipe 生效、输入 sink、排队、产出或稳定进度。无副作用的 ScheduleRecipe 排程预览始终可以解释已识别候选的 available/blocked/unknown 前置、缺失事实与恢复路径；未知投入、产出或耗时只能标为 unknown，不伪造可执行报价。只有取得权威的 active/作用域内 restricted recipe 结果，并在 fresh state 证明 factory fit、原料适用、物流、power 与 terminal 前置后，才可提交并形成排程承诺或不可逆结果；首次排产接受仍不等于首次生产，任何生产奖励/能力里程碑都必须至少等待一次 production receipt。目标随后只按 profile 声明的完成边界推进：`production-only` 的首产物目标由匹配 production receipt 完成一次，稳定窗口只推进独立的 `production-stable` 里程碑，terminal-admission/delivery 还须取得匹配的 delivery/terminal settlement receipt。缺少可用 recipe 或其准入证据时必须保持 blocked/unknown，不能自动挑选同名配方、把建设成本当作 recipe 解锁，或以“工厂已建成”打开下一目标。

当前目标必须按 profile 声明的完成边界判定：`production-only` 的匹配 production receipt 完成一次首产物目标并标为 `produced/undelivered`；独立稳定条件成立后才升级为 `production-stable/undelivered`，不得重复目标完成或奖励，不得减少交付需求或发放 delivery/terminal 奖励；声明 terminal admission/delivery 为因果条件的目标，只有其 purpose/recipient、数量与适用性由匹配的 delivery/settlement receipt 结算后才可完成。结算后若存在多个真实可达方向，玩家可以比较继续稳定当前能力、扩展/服务需求、合法换线或延期中的候选；候选的节奏、价值和数值取舍由 [`gameplay top-level design`](../../game/gameplay/gameplay-top-level-design.prd.md) 拥有，本产品层只要求每个候选说明第一动作、关键输入/工厂/物流前置、机会成本、主要风险、预计新选择或区域用途与回退路径。没有安全候选时必须明确停止并给出下一次决策点，不能自动生成新目标或把重复生产包装成成长。

若原料适用性、工厂能力、配方授权、物流边/容量、电力、终端 owner/资格或交付条件阻塞当前目标，目标保持 blocked 并沿同一 root 显示最早根因、仍占用/已损失价值和真实可用的恢复动作；不得静默换配方、改道、降规格或转移终端。改变 factory、recipe、输入来源、边或 terminal purpose 等产出因果内容时，必须建立 parent-linked 新目标/候选并从 `W=0` 重新计数；同一计划的修复只保留 root continuation。每次处置、交付或目标完成至多生效一次，重连、Agent retry、重复提交和 replay 只能返回相同结果。

该收口组合了 **world-first** 的真实材料/工厂/物流约束、**emergence-first** 的有代价路线选择、**persistent/auditable** 的目标与 receipt lineage，以及 **extensible** 的 profile 演进；它不新增 runtime schema、队列、配方公式、价格或 UI 布局。`game` 负责目标节奏、成长/反刷与候选平衡，M4 与 `world-runtime` 负责 lifecycle、receipt、守恒、幂等和 replay，Viewer/pure API 负责同义表达，QA 负责组合证据。

该闭环的 `test_tier_required` 至少覆盖：同一三阶段 `join → stage → transit → buffer → terminal` 路径下四类读面的正向样例；缺料、批次不适用、工厂/配方不兼容、边容量不足、停电和终端失效各自保持正确 primary blocker 与下一动作；同一 production receipt 在 `production-only` profile 中可完成生产目标但保持 undelivered 且不减少交付需求，在 terminal-admission profile 中则必须等待匹配的 delivery/settlement receipt 才完成目标并打开交付型下一候选；因果换线从 `W=0` 开始，非因果恢复保留 root continuation；Viewer 与 pure API 的 `next_action`、`next_recheck`、`progression_effect` 同义，重复提交、重连、乱序、Agent retry、snapshot restore 与 replay 不复制 sink、交付、奖励或目标完成。

<a name="工厂选址与网络拓扑决策（补齐-buildfactory-的地点选择产品缺口）"></a>

#### 工厂选址与网络拓扑决策（补齐 BuildFactory 的地点选择产品缺口）

现有工业路径已经定义了 `BuildFactory` 的 site/location/chunk 合法性和物流 path/edge 的执行规则，但如果玩家只看到“地点可建/不可建”，建厂就会退化为一次付款与前置检查，无法理解站点如何改变原料输入、产物输出、能源、容量和后续扩张。本文只补齐选址与网络拓扑的玩家决策语义，不新增地点、物流或迁移 runtime 能力。

站点候选必须从当前工业目标与同一权威快照派生，而不是从最近地点、客户端缓存或 Agent 推荐直接选定。每个候选至少说明：`site/location/chunk` 与 owner/作用域前置、原材料来源到账本的有效输入路径、产物到 terminal/目的账本的有效输出路径、路径/边容量与预计损耗、power 前置、输入/输出 buffer 与终端适配、建设或转移成本、主要交付风险、未来扩张或换线的机会成本、primary blocker、`next_action` 与 `next_recheck`。未固定 recipe 时，与具体 recipe 绑定的输入/输出路径、边容量/损耗、buffer/terminal 适配和生产 power/maintenance 只作为 advisory estimate；缺失时显示 `unknown/degraded`，不构成普遍 `BuildFactory` 阻断。`site_available`、`site_blocked` 或 `site_unknown` 的建设结论只由 site-level facts 与当前专业 profile 明示的 construction prerequisite 决定；recipe 固定或进入 recipe candidate discovery/`ScheduleRecipe` 后，才按新鲜状态验证 recipe-specific topology。后两者可以说明缺失的站点级、profile-declared 或 authority 证据，但不能进入可提交的建厂选项。

在提交前，玩家至少可以比较当前专业合同真实支持的 `build_at_site`、`prepare_inputs_or_route`、`restore_power_first`、`use_another_legal_site` 与 `defer`。站点比较预览必须让玩家看懂“靠近原料”与“靠近终端/能源”的即时收益、仍占用或将消费的价值、运输损耗/容量风险、对首个制成品或稳定产线的影响，以及失败后的等待、补料、改路、改站点或放弃路径。预览是只读的：不创建工厂、不扣建设资源、不锁定 site、物流 edge、吞吐、buffer 或 recipe，不改变排队顺位，也不替玩家确认新的 owner、治理或长期承诺。

`BuildFactory` 提交必须绑定所选 site 与当时有效的 site-level context，并从新鲜权威状态重新校验 owner、location/chunk、factory kind 及当前专业 profile 明示的 construction prerequisite。未固定 recipe 时，不能把某个 recipe 的输入/输出路径、边容量/损耗、buffer/terminal 适配或 recipe power/maintenance 当作普遍建厂前置；这些事实仅作 advisory estimate，延至 recipe candidate discovery/`ScheduleRecipe` 时按新鲜状态验证。recipe 已固定且 profile 明示其 topology 为 construction prerequisite 时，才一并 fresh validate 对应输入路径、输出路径、容量、power、terminal 与治理/权限条件。报价后任一已被当前 `BuildFactory` 合同声明为建设前置的站点或拓扑事实发生漂移时，只能按当前条件重新报价或原子拒绝；未固定 recipe 的 advisory estimate 漂移不阻断建厂，但必须在后续 recipe candidate discovery/`ScheduleRecipe` 重新读取。不得静默切换站点、自动改道、降级路线、隐藏损耗或生成部分设施。站点/拓扑变化若改变输入来源、物流边、能源、输出去向或 terminal purpose，则属于 parent-linked 新 factory/capability candidate，稳定窗口从 `W=0` 开始；只改变展示元数据且不影响产出因果时才可保留原 identity，无法证明时按因果变化处理并 fail closed。

站点候选被阻塞后，玩家必须能读到最早根因、已占用/已消费价值、受影响的输入或输出边、稳定窗口影响和下一次权威复查点，并只看到当前 profile 支持的等待容量、补料/补电、改路、改用其他站点、降载或延期动作。不存在合法站点或安全恢复路径时，必须明确 `no_safe_site_fallback`（或等价产品语义）并停止；不能把推荐、自动迁厂、后台重试或已接受目标伪装成工厂已建成、物流已连通或生产已开始。已存在的 accepted-unstarted、WIP、in-transit、buffer、terminal-pending 工作不因新的站点候选自动迁移；terminal-pending 保留 recipient、destination、admission、capacity obligation 与 provenance，直到 matching settlement 或一次受支持处置。若未来专业 profile 支持 relocation，必须沿既有 cutover/处置合同逐项产生 parent-linked 结果，否则不得在产品层展示迁厂动作。

这项补充保持 **world-first**：站点选择受真实账本、路径、容量、能源与终端约束；保持 **emergence-first**：玩家在源头近、终端近、能源近与扩张弹性之间承担可理解的机会成本；保持 **persistent/auditable**：所选站点、有效拓扑、阻塞与新旧 candidate 沿 root/receipt lineage 延续；保持 **extensible**：未来的区域、能源或工厂 profile 可复用同一选址预览与 cutover 边界。产品层拥有玩家比较、确认、失败恢复和 `W` 隔离语义；`world-runtime`/M4 拥有合法地点、路径/边、容量、账本与状态权威；`game` 拥有站点取舍、节奏与数值平衡；Viewer/pure API 负责同义表达。

该闭环的 `test_tier_required` 至少覆盖：同一目标下两个合法站点（例如原料近但输出路径拥挤、终端近但输入损耗较高）可从预览比较并显示成本、容量、损耗、power、终端适配、机会成本与下一步；站点、输入/输出路径或容量在报价后漂移时原子拒绝或重新报价且无设施/资源/物流副作用；站点因果变化建立新 candidate 并从 `W=0` 开始，accepted-unstarted/WIP/in-transit/buffer/terminal-pending 旧工作不自动迁移，terminal-pending identity/容量义务保留并只接受一次合法处置；无合法站点时显示阻塞根因和真实恢复路径；重复提交、重连、Agent retry、snapshot restore 与 replay 不复制建厂、路径 reservation、输入 sink、稳定进度或奖励，Viewer、pure API 与 Agent 对候选、root blocker、旧工作 identity/处置、`next_action` 和 `next_recheck` 保持一致。`test_tier_full` 覆盖三站点以上的共享边/终端容量争用、拓扑变化期间的 WIP/in-transit/buffer/terminal-pending 处置、持久化恢复与支持的 relocation/cutover profile。

#### 配方候选发现与选型（补齐 `factory-ready/recipe-missing` 到首次排产的产品缺口）

现有产品路径已经要求工厂建成后不能把建设 receipt 当作配方已启用，但还缺少“玩家如何找到值得比较的配方”这一段。`factory-ready/recipe-missing` 不能把整个 recipe catalog、同名版本或 Agent 推荐直接变成可排程列表；否则玩家看不出当前工业目标为什么需要某个配方，也无法区分“没有候选”“候选被前置阻塞”和“事实未知”。本节只定义候选发现、比较和选择的玩家语义，不新增配方生命周期状态、解锁树、runtime schema 或数值平衡。

配方候选是从当前主目标和同一权威快照派生的只读产品结果，而不是新的世界实体。每个候选至少绑定：当前目标/用途、recipe version 与 authority/scope、factory capability fit、所需输入批次及适用/缺口、物流边或 path/capacity、power/maintenance obligation、主产物与副产物的目的地/容量、首个可验证成果、机会成本、主要风险、下一次复查点和推荐理由。候选读面只使用当前有效的 `active` 或作用域内 `restricted` recipe；`pending_validation`、`validated_pending_admission`、`retiring`、`retired` 或无法证明 authority 的版本可以作为解释性 blocker 展示，但不能进入可选择排程。相同 recipe name 不合并版本、来源或历史 receipt。

候选发现至少收敛为三类产品结果（不是 runtime enum）：`candidate_available`（当前前置可证明且可进入排程预览）、`candidate_blocked`（版本/准入、工厂适配、原料规格/品质、物流/容量、电力/维护或终端条件中至少一项明确不足）和 `candidate_unknown`（关键 authority、批次 provenance 或复查事实缺失）。玩家可以比较 `inspect_candidate`、`prepare_inputs_or_route`、`wait_for_admission_or_recheck`、`use_another_legal_candidate` 与 `defer` 中当前真实支持的动作；没有安全候选时必须给出 `no_candidate_reason`、所需前置和下一次决策点，不能自动挑选同名配方、静默降级、把缺料伪装成“暂无任务”或由 Agent 代替玩家确认新的资源/权限承诺。

选择候选只产生一个可回看的、无世界效果的 `recipe_selection_preview`：它说明该候选如何服务当前目标、第一步投入/占用、预计产出与副产物去向、未满足前置、失败/损失风险、稳定窗口与交付影响，以及可回退的等待/改用/延期路径。选择不创建 recipe activation、hold、input sink、队列、物流 reservation、产出或稳定进度；只有玩家或具备对应范围的 Agent 明确确认后，`ScheduleRecipe` 才能在新鲜快照上重新校验并固定 recipe version/candidate identity。报价后任何产出因果事实漂移都必须重新发现/重报或原子拒绝，不能沿旧选择自动换配方。

玩家反馈必须回答“为什么推荐这个配方、当前缺什么、投入后先获得什么、失败后怎么恢复、何时再看一次”。成功选择后打开排程预览，不等于排程已接受；配方选择、排程接受、首次 production receipt、稳定里程碑和 delivery/terminal settlement 继续保持分层。Viewer、pure API 与 Agent 必须从同一快照给出同义的候选、blocker、机会成本、`next_action`、`next_recheck` 与 `progression_effect`；重连、重复查看、Agent retry、snapshot restore 和 replay 不复制选择、预留、排程或奖励。`game` 负责候选的目标价值、节奏与平衡，Recipe/Factory/M4/runtime 负责目录、授权、适配证据与权威版本，Viewer 负责表达，QA 负责组合验证。

该缺口的 `test_tier_required` 至少覆盖：工厂建成但无候选、单一可用候选、多个候选可比较、仅有 blocked/unknown 候选、同名不同版本、候选在预览后发生准入/原料/物流/电力/终端漂移；available/blocked/unknown 候选均可读取无副作用的 `ScheduleRecipe` 排程预览，后两者只解释阻塞/未知与恢复；只有 `candidate_available` 且最终新鲜校验通过才能提交，其余结果不产生 sink/hold/queue/W/奖励；选择预览保持只读，提交后只固定一次当前 recipe version，失败提供补料、补证、改路、改厂、等待或延期中的真实动作；重复查看、重连、retry、restore 与 replay 不自动选择、不复制效果，Viewer/pure API/Agent 对候选与下一步保持一致。

#### 代表性配方执行档案（把分散规则收成一条可验收工业链）

候选字段齐全不等于一条工业链已经设计完整。每个作为产品验收基准的代表性配方，必须提供一份**配方执行档案**，把同一 candidate 下的工厂能力、原材料批次、物流边、合法执行单位、输出分支和终端用途收成一个完整声明；任何一项只能从别处“猜到”或由 surface 临时拼接时，该档案为 `incomplete`，不得用于宣称流水线可排程、可稳定或可交付。档案是跨专业合同的只读投影与验收夹具，不是新的 runtime schema、recipe catalog 或第二套材料/物流权威。

最小代表性档案必须同时回答以下问题：

| 决策面 | 最小完整声明 | 玩家由此能判断什么 |
| --- | --- | --- |
| 工厂与配方 | pinned recipe version/authority、factory capability fit、作用域与当前 lifecycle state | 这座工厂为何能执行该版本，以及准入/退役或能力变化是否会阻止开工 |
| 原材料与齐套 | 至少两条 required input edge；每条绑定来源 ledger、batch/quality/custody 适用结论、数量与 join policy | 哪些原料已可消费、哪些仍缺失/未知，以及 full-only 或 staged intake 会占用什么 |
| 物流与容量 | 每条跨地点输入/中间品边的 effective path、预计实际到达量/损耗、吞吐或 buffer obligation 与复查边界 | 是立即调运、减量、换来源/路径还是等待容量，且“已发出”不等于“已到达” |
| 执行与能源 | canonical executable cycle/quantum、full/partial policy、stage capacity、power mode 与预计时间/风险 | 本次究竟承诺多少、哪些资源会在何时变得不可逆，以及不足一个合法单位时为何不能产出半批 |
| 产出与去向 | 一个 output bundle 内的主产物和每个副产物 branch、owner/destination、atomic 或 split fan-out policy | 某个副产物去向失效时是整批阻塞还是只保留未决 branch，不会出现主产物成功而副产物消失 |
| 终端与成长 | production-only 或 terminal-admission 完成边界、terminal owner/capacity/purpose、首次成果与后续目标 | 一次生产 receipt 是否只证明产出，何时才完成交付、减少需求、获得里程碑并打开下一选择 |

代表性最小样例固定为“一座具备目标 capability 的工厂 + 一个 active/restricted 配方版本 + 两类独立原材料 + 至少一条有损或有容量约束的物流边 + 一个主产物与一个必须处置的副产物 + 一个终端用途”。该复杂样例是本节与 SC-28 的代表性覆盖目标，不是每条 recipe 或 starter 首产的强制前置；首局仍按上文 `starter-industrial-smelter-to-assembler-v1` 的 current chain 与首个 iron-ingot production settlement 边界核验，不因本样例要求额外增加双输入、副产物或终端交付。这里冻结的是覆盖形状，不冻结材料名称、配比、产率、价格、绝对容量、tick 数或字段编码。档案必须引用 M4、Recipe/Factory 与 `world-runtime` 的当前权威事实；缺少 authority、批次 provenance、路径、power mode、output policy 或 terminal boundary 时显示对应 `unknown/blocked`，不能用默认零值、同名材料、最近路径或“推荐配置”补齐。

玩家在同一档案上至少可以比较 `schedule_declared_cycle`、`prepare_or_source_missing_inputs`、`transfer_or_wait_for_capacity`、profile 支持时的 `reduce_to_legal_unit`、`repair_power_or_factory_fit`、`resolve_output_destination` 与 `defer`。每个候选必须说明即时收益、仍占用/将消费的价值、主要失败成本、对稳定窗口和交付的影响、下一次权威复查点与推荐理由；确认只固定所选 candidate/cycle 的一次承诺，未选路径不取得 hold、吞吐顺位、input sink 或稳定进度。任何改变配方版本、工厂能力、required edge、输出 branch 或 terminal purpose 的修复都是 parent-linked 新 candidate，并从 `W=0` 开始；仅补齐同一 candidate 的原料、容量或电力前置才保持 root continuation。

失败与恢复必须用这份完整档案做守恒判断：任一 required input 不适用、join 未齐套、effective path/容量不足、power/factory fit 漂移或 mandatory output/terminal 失效时，在首个不可逆 sink 前延期或原子拒绝；已经存在 WIP、在途或已结算 branch 时，只能沿 profile 支持的等待、释放、改道、隔离、返工、return、salvage 或终止路径各处置一次。不得静默换配方、把 full-only 降为 partial、让同一批原料加入两个 join、自动补货/退款，或在 output branch 失败时丢弃副产物并保留完整奖励。

该档案的 `test_tier_required` 至少以同一 deterministic fixture 覆盖：两类输入齐套后只启动一个合法 cycle 并产生一次 bundle；其中一类 input 为 `unknown/not_applicable` 时无 sink/WIP；物流满时保持可读 blocked，容量释放后仅重评未决 edge；power 在报价后不足时原子拒绝或按声明 mode 延期；mandatory 副产物 destination 失效时遵守 atomic/split policy；production-only 与 terminal-admission 分别只在各自声明边界完成目标。Viewer 与 pure API 必须从同一档案读出 factory/recipe、两类输入、path/loss/capacity、cycle、bundle/branch、terminal、primary blocker、机会成本、`next_action` 和 `next_recheck`；重复提交、重连、arrival reorder、Agent retry、snapshot restore 与 replay 不复制材料消费、hold、产出、交付、稳定进度或奖励。`test_tier_full` 再扩展到三阶段、三个以上输入/输出分支、cutover、长期持久化恢复和跨窗口 lineage。

<a name="原料替代、替代来源与换配方决策（补齐输入阻塞后的合法替代边界）"></a>

#### 原料替代、替代来源与换配方决策（补齐输入阻塞后的合法替代边界）

现有合同已经禁止按材料名称推断适用、静默降级、隐藏混料和免费转换，却没有正面定义缺料、来源耗尽或批次不适用时何种“替代”可以进入玩家决策。产品必须区分三种不能互相冒充的选择：`替代来源` 是同一材料 profile 改用不同 source/ledger/path；`原料替代` 是不同材料 profile 经当前 pinned recipe 明确许可后满足同一 required input edge；`换配方` 则改变 recipe version、输入/输出因果或终端用途，属于新的配方候选。推荐、同名材料、相似品质、市场可买、Agent 判断或旧版本曾经接受都不能自动建立原料替代关系。

原料替代默认禁止。只有当前 recipe/factory 专业 profile 对具体 input edge 声明有限 substitute set，且同时给出适用的规格/品质与 custody 边界、确定性的数量比例和 rounding、residual disposition、owner/ledger/path 前置，以及对 power、时长、主产物、副产物、质量、terminal 或价值分类的影响时，才可形成合法候选。缺少 ratio、provenance、适用性、owner、route、输出影响或 authority 的候选必须显示 `unknown/blocked` 或 `no_legal_substitute`，并在首个不可逆 sink 前 fail closed；`degraded` 只用于专业 profile 已明确允许的降级质量结果，不得用它包装未知证据，也不得按零损耗、等量替换、等价产出或安全可用补默认值。

输入阻塞发生在下一不可逆 sink 前时，player-facing surface 应提供只读 `industrial_substitution_preview`（产品语义，不冻结 runtime 名称），比较当前 profile 真实支持的 `keep_and_wait`、`use_alternate_source`、`prepare_substitute_material`、`switch_to_legal_recipe`、`reduce_or_defer` 与 `pivot`。每个候选至少说明原/目标材料或配方身份、authority/compatibility、admissible quantity、比例/rounding/residual、额外投入与转换损耗、power/物流/buffer/terminal 占用、已 hold/已消费价值、预计 output/value class、主要风险、稳定窗口影响、`next_action` 与 `next_recheck`。首局只在当前 primary blocker 确实由输入造成时展示至多 2–3 个有权威依据的主要候选；未知兼容性不能成为虚假安全选项，也不能阻断仍合法的原配方等待/补料路径。预览不扣料、不转换、不预留、不取得队列顺位，也不推进 tick、WIP、`W` 或奖励。

玩家或具备对应范围授权的 Agent 明确确认后，提交必须从新鲜权威状态重新校验 recipe/profile、批次、比例、质量/custody、owner、ledger、route/capacity、power、output/terminal 和权限，并把实际 settled substitute batch/receipt 绑定到 canonical input/join identity。旧候选中未消费的 hold 只能按原 identity 释放或保留一次；短缺、到期、延迟、retry、重连或 Agent 重规划都不能自动触发替代。合法替代先产生一次 parent-linked substitution decision receipt；该 decision 只冻结选择与适用依据，不扣料、不 credit、不转换、不推进 `W` 或奖励。只有 profile 明示需要材料 conversion 且转换实际结算时，才另产生一个链接该 decision、输入批次与 conversion effect 的 exactly-once settlement/provenance receipt；无需转换的替代不得伪造 conversion receipt。实际 production receipt 才能推进工业进度，delivery/terminal settlement 仍独立决定交付、需求减少和奖励。

若 pinned recipe profile 已声明该替代材料与原输入具有相同的产出因果、质量、power、时间、副产物、terminal 和 value class 语义，则 recipe version 可以保持不变，但必须建立可审计的 child decision segment，记录所选 batch、比例、rounding、residual、损耗、父 receipt 与 authority snapshot。任何上述因果语义变化都必须形成 parent-linked 新 recipe candidate/version，并从 `W=0` 开始；旧 receipt、reservation、排队资格、稳定进度或奖励不得迁移。换配方始终遵循新候选与既有 cutover 合同，不能包装成“原料替代”来保留旧进度。

部分替代只在 profile 已支持 partial execution 时可用，并继续服从 canonical executable unit；residual 必须按 profile 确定地保留/结转、return、产生一次有界损耗或原子拒绝，不能由客户端取整、隐藏丢弃或进入第二个 join。首个不可逆 input sink 之后，不得追溯改写为替代；只有专业合同明确支持的 rework、conversion、return、hold 或 salvage 才能各产生一次 linked disposition。切换失败必须保留已消费、仍占用、可追回与已损失价值，并给出等待、补原料、改来源、重新报价、换配方或终止中真实可用的恢复动作，不得自动退款、瞬移、降级或免费重算产出。

这项合同保持 **world-first**：替代资格和效果来自 settled batch、profile 与权威 receipt，而非客户端或 Agent 猜测；保持 **emergence-first**：玩家在稀缺原料、转换损耗、物流/能源占用、产出质量与交付时机之间形成真实取舍；保持 **persistent / auditable**：所选批次、比例、损耗、owner、parent linkage 和 decision 跨恢复/replay 延续；保持 **extensible**：未来材料 profile 可以增加有限替代关系，但不能改写既有 recipe 或 settlement 历史。产品层拥有三类选择、确认与进度边界；`game` 拥有比例/损耗带来的节奏、价值和平衡；Recipe/Factory/M4/runtime 拥有 profile、批次、join、ledger、conversion 与 receipt 真值；Viewer/pure API/Agent 负责同义表达，QA 负责确定性与反套利验证。

`test_tier_required` 至少以同一 blocked-input fixture 覆盖：默认禁止的异料替代无 sink；同料换来源、profile 许可的异料替代与换配方作为不同候选展示完整机会成本；preview 保持只读；fresh commit 只结算一次；ratio/rounding/residual 守恒；同名但不适用、错误 owner/quality/route、过期 profile 与 stale preview 原子拒绝；无因果变化的替代保持 pinned recipe 并只建立一个 child decision，有因果变化或换配方建立新 candidate 且 `W=0`。重复提交、切换、重连、Agent retry、restore 与 replay 不复制 sink、conversion、receipt、产出、稳定进度或奖励。`test_tier_full` 扩展到多阶段 join、并发 root 争用同一替代批次、品质/保管漂移、in-transit/buffer 状态、跨窗口 cutover 与持久化恢复；Viewer、pure API 与 Agent 对候选、blocker、成本/损耗、lineage、`next_action` 和 `next_recheck` 保持一致，并证明不能通过廉价替代、反复切换或拆分提交制造高价值产出、刷新顺位或重复成长。

<a name="配方生命周期、版本兼容与受控退役"></a>

#### 配方生命周期、版本兼容与受控退役

配方不是可被客户端或 Agent 就地修改的库存标签，而是具有来源、作用域和版本身份的受治理工业能力。配方生命周期使用六个且仅六个产品状态：`pending_validation`（待验证）、`validated_pending_admission`（已验证待准入）、`active`（已生效）、`restricted`（受限生效）、`retiring`（退役中）和 `retired`（已退役）。只有 `active` 或在声明作用域内的 `restricted` 版本，且当前授权、工厂能力、原材料规格/品质、物流路径、电力与终端前置均满足时，才能成为新的生产候选；提案、模拟结果、同名配方推荐或“已下载”状态都不产生预留、输入 sink、产出、稳定窗口或交付资格。

状态转换必须由权威条件驱动，不能由客户端缓存、Agent 推荐或重连隐式触发：

| 状态 | 进入条件 | 允许的下一状态与条件 |
| --- | --- | --- |
| `pending_validation` | 新配方或因果变更版本缺少完整确定性验证、来源/规格证据或 profile 证明 | 证据完整后进入 `validated_pending_admission`；提案撤回、验证失败或不可补足时进入 `retired`；不得直接进入 `active`。 |
| `validated_pending_admission` | 验证完成，但治理准入、作用域或激活授权尚未成立 | 当前授权明确激活后进入 `active`，仅有边界授权时进入 `restricted`，拒绝/撤回/授权失效时进入 `retired`。 |
| `active` | 当前治理授权、版本激活和必要专业证据均成立 | 作用域收缩或安全限制进入 `restricted`；已有 successor、撤销或计划退出的明确决定进入 `retiring`。 |
| `restricted` | 已生效版本被限制在明确的 owner、区域、用途、工厂或时间作用域内 | 新鲜授权扩大作用域可回到 `active`；继续收缩或退出进入 `retiring`；截止边界完成后进入 `retired`。 |
| `retiring` | `active`/`restricted` 版本收到明确的撤销、到期、替代或受控退出决定 | 完成声明的 cutoff 与存量处置后进入 `retired`；只有 cutoff 前取得新的权威授权，才可回到 `active`/`restricted`，不能靠旧 receipt 复活。 |
| `retired` | 拒绝、撤回、退出完成或 cutoff 到达 | 该版本是终态；只能创建新版本/新候选，不得重新激活或恢复旧排程资格。 |

状态与工业任务的动作边界如下；`accepted 未产生首个不可逆 sink` 不等于已经取得配方资格，WIP/在途/buffer 也不能因状态转换自动换绑新版本：

| 状态 | 新排程 | `accepted` 未产生首个不可逆 sink | 已有 WIP / 在途 / buffer |
| --- | --- | --- | --- |
| `pending_validation` | 不可选择，只能查看预览 | 保持无效果待决或原子拒绝，不产生 sink | 在下一不可逆边界前暂停；保留 lineage，仅可走 profile 明确支持的 hold、return 或 quarantine。 |
| `validated_pending_admission` | 不可选择，等待准入 | 保持无效果待决、重新报价或原子拒绝 | 不新增生产/交付效果；按 profile hold、quarantine 或 return，不能伪装为已准入。 |
| `active` | fresh state 满足前置时可选 | 重新校验后只启动一次 | 按 pinned version 继续，沿既有 lineage 结算。 |
| `restricted` | 仅在声明作用域内可选，越界拒绝 | 作用域内可重验并启动；越界只能 hold、重报价或拒绝 | 仅在作用域内继续；越界按 profile hold、quarantine、return、rework 或 terminate。 |
| `retiring` | 不可选择，不自动推荐或切换 successor | hold、重报价或拒绝；切换 successor 必须显式确认并新建 child candidate | 仅 profile 允许时 drain/finish；否则一次性进入 hold、quarantine、return、rework、salvage 或 terminate。 |
| `retired` | 不可选择 | 原子拒绝且无效果；successor 必须是新候选 | 不产生新效果，保留历史并仅按既有 profile 处置，不得复活旧版本。 |

每个已接受的工业任务必须固定一个不可变的配方版本/能力身份，并把它与 factory capability、输入批次、物流边/路径、输出 bundle 和 terminal purpose 关联。改变输入/输出/副产物、规格或品质适用范围、阶段关系、能源/物流前置、工厂能力或终端资格等产出因果内容，必须创建新的配方版本和 parent-linked 候选；新候选从稳定窗口 `W=0` 开始，不能继承旧候选的 receipt、reservation、里程碑、排队优先级或实际进度。仅改变不影响产出因果的说明/展示元数据时，才可保留原生产身份；无法证明为非因果变更时，按新版本处理并 fail closed，不能靠名称相同或当前缓存猜测兼容。

配方兼容不是“材料名称相同”或“有一座工厂”即可。提交前的只读预览至少让玩家看见配方版本的来源/授权作用域、输入与产出/副产物用途、匹配的 factory capability、每条必需物流边或路径、规格/品质适用结论、power/容量/terminal 前置、六状态之一和下一次复查点。预览状态必须与上表及 SC-24 使用同一组 `pending_validation`、`validated_pending_admission`、`active`、`restricted`、`retiring`、`retired`；`not_licensed`、`incompatible`、`expired`、`unknown` 只能作为 blocker/authority reason，不能伪装成新的生命周期状态或可选择的“降级替代”。

这些事实必须收成一条玩家可执行的产品循环：`查看配方状态 → 预览工厂/原料批次与品质/物流路径与容量/终端适配 → 在新鲜排程、等待/补证或复验、改用当前合法候选、处置旧任务与延期中做选择 → 提交时按当前权威状态重验 → 读取运行/阻塞/退役处置结果 → 进入下一个工业目标`。每一步都必须说明 primary blocker、已占用或已消费的价值、当前选择的机会成本、对稳定窗口 `W` 的影响与下一次权威复查点。任一环节只有一个合法动作时仍须说明原因；没有安全动作时必须 fail closed，不得用 Agent 推荐、同名 successor 或“自动优化”代替新的授权和玩家选择。

六态还必须给出与动作一致的进度/奖励语义：`pending_validation` 与 `validated_pending_admission` 只有 evidence，不产生 accepted 任务、sink、`W`、产出或奖励；`active` 只有实际 production/delivery receipt 才推进首个制成品、稳定窗口与后续用途；`restricted` 只有作用域内的实际 receipt 才产生一次进度/收益，不能暗示作用域外或永久容量；`retiring` 只记录适用 profile 支持的受控排空、处置、转换或历史结果；`retired` 只保留历史与已记录的最终处置，不因退出本身发放里程碑。每个非 active 状态必须只展示当前 profile 真实支持的一项或多项动作，并给出下一次可决策/复查点；若补证、等待、继续、切换 successor 或处置均不安全，必须明确返回 `no_safe_fallback`，不能用自动换配方、免费转换、静默降级或伪造退款制造进展。

配方状态或前置在报价后漂移时，系统只能按当前权威版本重新评估并产生一次 receipt，或在首个不可逆 sink 前原子拒绝/保持无效果待决。至少以下情形必须 fail closed：版本身份或授权范围缺失；输入规格、owner、账本、路线、容量、power 或 terminal 资格不匹配；尝试以 `retired` 版本取得生产资格，或对 `retiring` 版本的既有生产请求继续/排空却没有当前适用的显式 profile；候选引用了错误 root、批次、stage 或 module/provenance。`retiring` 缺少 drain/finish profile 不一并禁用其独立合法的 hold、隔离、return；每项动作仍按自身适用 profile 校验，不为 `retired` 版本恢复生产资格或授予新的处置效果。失败必须保留原任务/已结算历史的 lineage，说明最早 blocker、已占用/已消费与下一步；不得自动换配方、免费转换材料、静默降低规格、伪造退款或以“已接受”代替生效。

`restricted` 与 `retiring`/`retired` 不是同一状态：`restricted` 仍是受限的已生效能力，只能在声明作用域内取得新排程或保全既有价值；它不得把局部授权扩张为永久容量或作用域外效果。`retiring` 不再接受新排程，只允许 profile 支持的既有任务排空或取得一次 hold、隔离、返工、salvage、return、改道、终止或 successor handoff；`retired` 则只保留历史读取/回放和已记录的最终处置。三者都不能追溯删除已结算历史。若存在兼容 successor，仍须在同一权威快照完成 fresh revalidation，并由玩家或具备范围授权的 Agent 明确确认；转换必须创建链接旧 receipt、实际数量/损耗与新候选身份的单次 handoff/conversion receipt，且新候选从 `W=0` 重新计数。旧版本的重连、重试、回放或陈旧批准不能重新激活、复制产出或刷新容量顺位。

为使每个状态都有可执行的下一步，以下是产品动作语义（不是 runtime 字段、API 或 UI schema）；除 `active` 外，若当前 profile 没有列出的安全处置，必须明确返回 `no_safe_fallback` 和下一次可决策/复查点：

| 状态 | 玩家可做的动作 | 进度/奖励语义 | 失败、下一步与无安全替代 |
| --- | --- | --- | --- |
| `pending_validation` | 查看缺失/不确定证据，补齐并重新提交、撤回或放弃提案 | 只有 validation evidence；不产生 accepted 任务、输入 sink、WIP、`W`、产出或奖励 | 失败/未知保持待验证；下一步是补证据后重验或放弃；不能修复时明确 `no_safe_fallback`，不伪造降级替代 |
| `validated_pending_admission` | 查看验证结论，等待/请求准入、比较当前有效版本或撤回 | 验证决定仍是 evidence-only；不产生生产资格、预留、输入 sink、`W`、产出或奖励 | 准入撤回、过期或前置漂移时必须重新验证/重新申请或改用当前有效版本；无合法准入路径时 `no_safe_fallback` |
| `active` | 在当前 pin 和前置下执行 `ScheduleRecipe`，推进首个制成品，并在实际结果后选择新的工业用途 | 只有实际 production/delivery receipt 才推进执行、稳定 `W`、首个制成品或后续用途；预览/接受不发奖，实际里程碑应打开可读的下一方向 | 缺料、缺电、物流、容量、治理或 terminal blocker 只能等待、修复、重新报价、降载/改道或重规划；都不支持时 `no_safe_fallback` |
| `restricted` | 在声明作用域内排程或继续既有承诺；也可持有、减量、请求扩大授权或受控退出 | 只有作用域内实际 receipt 产生一次进度/收益；不得把局部授权显示成作用域外资格或永久容量 | 越界或授权漂移时 hold、重报价、请求授权或一次性处置；不能安全保全时 `no_safe_fallback`，不能自动扩容、换配方或退款 |
| `retiring` | 对既有任务选择 profile 支持的 drain/finish、hold、返工、salvage、return、改道、终止或经确认的 successor handoff | 只记录 cutoff 前既有承诺的一次实际结果或处置/转换 receipt；退出本身不发放新稳定进度或里程碑 | 新排程不可选；下一步是完成声明的 cutoff/处置或 successor fresh revalidation；没有安全处置时 `no_safe_fallback` |
| `retired` | 读取历史/回放；查看已记录的最终处置与 successor lineage | 不再产生新的生产资格、稳定进度或里程碑奖励，不能复活旧版本 | 新排程直接拒绝；只能创建新版本/新候选；没有合法 successor 时 `no_safe_fallback` |

该边界同时服务五项产品原则：**world-first** 让可用配方由真实授权、材料和设施能力决定；**emergence-first** 允许玩家在约束内选择来源、版本、路线与退役处置，而不是领取免费转换；**persistent** 让版本 pin、lineage、pending 与退役处置跨重连/恢复/replay 延续；**auditable** 让提案、准入、版本变更、兼容结论和单次 receipt 可追溯；**extensible** 允许未来增加配方/工厂 profile 而不改写既有批次和历史。配方治理不直接发放原材料、产能、库存或市场权利，也不规定配方比率、授权角色、版税、兼容算法、runtime schema 或 UI 实现。

配方生命周期验收至少包括：

- 一条提案→验证→准入→生效→受限→退役中→已退役样例；生效前无世界效果，玩家可读每个状态、阻塞、可执行动作与下一步。
- 同名但改变产出因果的两个版本；旧任务固定旧版本，新候选 `W=0`，两者不共享 receipt、reservation、进度或奖励。
- 授权、输入规格、factory fit、物流/电力/容量或 terminal 证据缺失/漂移的负例；首个 sink 前原子拒绝或无效果待决，且显示 primary blocker 与复查边界。
- 受限、退役中与已退役同时存在新排程、未开始任务、WIP/在途/缓冲批次的样例；受限只在声明作用域内产生一次实际价值，退役中新排程不可选且只按 profile 排空/处置，已退役只能读取历史或创建新候选；兼容 successor 需显式确认、parent-linked conversion 并从 `W=0` 开始，没有安全处置时显示 `no_safe_fallback`。
- 重复 submit、arrival、重连、Agent retry、snapshot restore 与 replay；对同一权威生产 cycle/effect 及其已确认 output branch，重复操作至多产生一次 sink/产出/receipt；同一配方版本下不同的合法请求或 cycles 可以分别结算，但必须保留请求身份与内容摘要、candidate/recipe/cycle/output-bundle lineage，不能通过换请求身份或拆小批重复同一效果、增加奖励或恢复退役版本资格，旧版本不能复活，Viewer 与 pure API 对状态、兼容结论、机会成本和下一步保持同义。

### 世界宪法级产品不变量

- 玩家通过目标、Agent、地点、设施、配方、关系与治理等受支持动作获得间接战略能动性；资源变化必须来自被授权的 source/sink 因果链，不能凭空生成或绕过成本。
- 每个权威行动都必须经过规则与权限校验并产生可审计后果；玩家能够读懂 target、action、cost、blocker、result、next decision 与 recovery，不靠隐藏状态猜测世界为何变化。
- 社会关系、组织、市场与制度可以在权限、治理和 anti-abuse 边界内产生有限涌现，但不会因此获得绕过权威规则的能力。世界保持持久、开放式演化，不要求强制终局。
- 系统性危机只能在受治理、可审计的 containment 边界内限制扩散并保护基本连续性；恢复必须留在同一权威世界时间线中，由玩家、Agent、组织或区域项目按既有资源、权限、治理、反滥用与申诉边界推进。containment 或恢复不得 reset 世界、追溯改写已确认因果、跳过申诉、选择性 bailout，或把局部运维/管理员操作包装成世界恢复；受影响主体必须能读到触发事实、作用范围、当前限制、恢复/申诉路径与下一步。
- 玩家前台始终围绕一个当前主目标：系统基于权威世界状态给出可达推荐与“继续”路径，玩家只在阶段性方向、主动换向或实际影响共同资源/权限的事项上作出必要选择。目标作用域、canonical 转译、资源/权限校验、共同治理、反支配与审计是后台护栏，只在实质改变当前选择时以原因和替代路径出现；不得把它们扩张为逐动作的表单或确认负担。
- 当前不支持的细粒度请求必须转换为 canonical 可执行替代动作；没有安全替代时明确停止并说明下一次可决策点。具体规则、确定性执行、Agent 行为与网络/治理合同分别由 [`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md) 与 [`doc/p2p/prd.md`](../../p2p/prd.md) 维护。

### Kernel、治理参数、制度模块与 Agent strategy 的分层边界

本产品层采用四层边界来判断一个新能力属于世界的什么部分。分层是产品语义和决策顺序，不是对 runtime 类型、存储 schema、WASM ABI 或 Action 名称的冻结；这些实现合同仍由对应专业域拥有。一个能力只有在低层护栏满足后，才能向上层开放选择。

| 层级 | 产品职责 | 可被改变的范围 | 不可被改变的边界 |
| --- | --- | --- | --- |
| **L0 Kernel** | 时间推进、空间与身份、所有权、资源守恒、行动/计算成本、权限校验、权威状态转换与可审计结果 | 仅由世界规则和兼容迁移共同维护 | 不得瞬移、凭空创造或复制资源、绕过所有权/权限、重复结算或改写已确认因果；任何制度和 Agent 都不能绕过 L0 |
| **L1 Governed parameters** | 在治理和版本规则内调节基础物理/经济参数，例如成本、容量、速度、损耗、配额或费用边界 | 可由明确治理程序、版本和生效窗口调整，并保留前后因果和恢复边界 | 不能把 L0 约束调成无成本、无限量、无权限或无时间；参数变化不能静默改变既有 receipt、快照或历史结果 |
| **L2 Institution modules** | 市场、联盟、公司、战争制度、货币、税收、声誉、合同、法院等文明制度，以及玩家/Agent 在约束内创建的扩展 | 可以通过受治理、可审计、版本化的模块提供新制度行为、状态和事件；模块可竞争、退役或由 successor 接替 | 只能使用被授权的 L0/L1 能力，不能重写物理真值、治理底线、所有权或已结算历史；模块失效时必须有明确的待决、恢复或退役路径 |
| **L3 Agent strategy** | Agent 对目标、计划、资源取舍、模块选择和制度参与方式作出策略决定 | 可发现当前有效能力并组合策略，也可提出或部署受治理的制度模块 | 策略、提示词、缓存、推荐或离线计划都不是世界效果；必须经有效授权、权威校验和一次性结算才能产生 L0/L1/L2 结果 |

这四层保持 **world-first、emergence-first、persistent、auditable、extensible** 的同一口径：世界物理和历史先成立，文明制度在护栏内涌现，Agent 只能在可读且有代价的选择空间中行动；重连、snapshot restore、replay 或版本切换不得制造第二条时间线或第二次效果。玩家应当能分辨“物理/治理约束”“制度提供的服务”与“Agent 的策略建议”，但产品层不要求玩家理解底层实现细节。

#### 现有 native 制度能力的兼容定位与新增冻结

现有 native `Alliance`、`War`、`Governance`、`EconomicContract` 能力是当前可玩 vertical slice 和兼容读取/回放的基线。它们证明当前玩家路径可以被验证，不构成“凡是制度都应继续进入 native Action/State”的扩展先例；在迁移完成前，不得因追求架构纯粹而删除、重置或改变既有玩家结果。

从本 PRD 生效起，新增银行、公司、保险、私人货币、法院或同类文明制度时，默认**冻结新增 native 制度 action/state**。任何候选必须先完成以下分类，再由对应专业角色确认合同和验证范围：

- **Kernel keep**：不可绕过的世界不变量，留在 L0/L1 的产品边界内；
- **Module candidate**：应优先作为 L2 first-party 或 Agent-created module 试验，不直接扩张 native core；
- **Compatibility keep**：为现有 vertical slice、历史快照、回放或玩家连续性暂时保留，不视为未来扩展模板；
- **Defer**：当前证据不足、跨域风险过高或属于后期扩展，进入后续版本，不以临时 native 字段填补。

当前四个 native 制度能力采用以下**单一、互斥的产品分类**；每一行只有一个当前类别，类别之间没有继承、叠加或“既兼容又候选”的隐含优先级。该表是产品层的 canonical mapping，runtime/WASM 只需在各自合同中证明与之相容，不得用宽泛的制度示例覆盖它：

| Native capability | 当前唯一分类 | 产品含义与版本边界 |
| --- | --- | --- |
| `Alliance` | **Compatibility keep** | 保留现有 vertical slice、历史快照、回放与玩家结果；本版本不因兼容存在而扩大 native 语义。 |
| `War` | **Defer** | 不进入本版本的开放扩展或新增 native action/state；后续需新的跨角色证据和版本决策。 |
| `Governance` | **Kernel keep** | 仅就授权、政策、最终性、反滥用和世界边界的不可绕过约束留在 L0/L1 产品护栏；不得由 module 或 Agent 关闭。 |
| `EconomicContract` | **Compatibility keep** | 以 canonical 名称保留现有经济合同的可玩路径、旧事件/快照/replay 读取和结果连续性；不把兼容 surface 当作默认扩展模板。 |

`Alliance` 与 `EconomicContract` 初始都属于 `Compatibility keep`，以避免在试点尚未选定时产生两个开放候选。首个开放扩展试点只能在治理批准的 activation boundary 选择其中一个：被选者从 `Compatibility keep` **一次性**转为 `Module candidate`，另一个继续保持 `Compatibility keep`；选择、迁移、回滚或替换必须有版本化 proposal、fresh authorization、旧结果保留和 replay/snapshot/receipt 兼容证据。未选中的能力不能因共享制度概念、provider response 或 Agent 推荐而自动转为候选；`War` 与 `Governance` 也不会通过该试点规则隐式改类。这样任何时点都只有一个明确的 pilot candidate（或在选择前没有 candidate），其余能力各自保持单一分类。

首个开放扩展试点从 `Alliance` 或 `EconomicContract` 中择一，目标是验证现有制度的玩家语义能否在不扩大 Kernel 状态/动作集合的前提下，沿开放的 command/event/state 合同迁移或互操作。试点必须保留既有历史和玩家可玩路径，并由 `world-runtime`、WASM、Agent 与 QA 专业文档分别定义实现、ABI、能力发现和回放验证；本 PRD 不复制这些 schema。

### 版本优先级与渐进迁移

版本排序服从“先稳定边界，再证明开放扩展，最后扩大表达能力”的原则：

1. **P0 — 边界冻结与分类**：冻结新增 native 制度 action/state，建立现有 Action/State 的四类分类矩阵，并把每个跨层主张链接到产品/专业 owner；L0 物理、治理底线和 receipt/replay 连续性先于新制度数量。
2. **专题 P0 — 条件启动的单一制度试点**：仅在首局闭环需要或本阶段产品证据支持后续启动时，于合法激活边界选择一个最小开放扩展试点；`Alliance` 与 `EconomicContract` 保持既定兼容分类，War 延后，Governance 基础护栏保留。试点验证 replay、snapshot、版本/ABI 兼容和一次性 receipt 下的玩家可读、可恢复、可审计，并保留已有结果及读取/回放能力。
3. **P1 — 渐进执行与能力发现**：在专业域合同成熟后，逐步收敛单一执行管线、开放模块边界和受治理的 Agent capability discovery；保留兼容 wrapper/读取路径，直到迁移证据闭合。
4. **P2 — 后期结构扩展**：只有在单一时间线、扩展协议、恢复和可观测性证据成熟后，才评估更广泛的 World Database/ECS/Region/Partition 结构；它们不是当前产品承诺，也不能提前改变玩家的世界连续性。

每一阶段都必须满足“旧结果可读、新结果可追溯、失败可恢复、重复不生效”的迁移门槛。产品层只规定玩家与世界的可观察承诺；执行顺序、模块 ABI、快照编码、receipt 字段和共识最终性分别由专业 authority 定义。

## 3. 权威与冲突处理

| 产品层拥有 | 专业域权威 |
| --- | --- |
| 玩家目标、核心循环的产品结果、成长、资源压力与跨域世界不变量体验 | `doc/game/prd.md` 拥有玩法规则、moment-to-moment loop、数值平衡和专题验收；`doc/world-runtime/prd.md` 拥有权威验证、确定性状态与审计；`doc/world-simulator/prd.md` 拥有 Agent/LLM 与交互模拟合同；`doc/p2p/prd.md` 拥有网络、共识与治理技术边界 |

产品层不用新细则或数值静默改写专业域权威。跨域冲突由 `producer_system_designer` 协调，并按受影响合同邀请对应的 `gameplay_designer`、`runtime_engineer`、`agent_engineer` / `world-simulator` owner 或 `blockchain_ops_engineer` / P2P owner 形成显式裁决；不得只以 gameplay 判断覆盖执行、Agent、网络或治理专业结论。



## 既有统一生成合同的有限产品接收

本节为 [unified 原需求](../../world-simulator/scenario/unified-world-seed-fragment-runtime.prd.md#3-requirements)中已经存在的消费者承诺登记精确身份；逐卡原句来源限定其范围，不新增世界生成、出生、字段、参数或资源承载选择。已有专业目标不是新的实现完成声明；正式 profile 的 [四个未决问题](../../world-simulator/scenario/unified-world-seed-fragment-runtime.design.md#unified-decision-profile)继续未定，已采用本地子范围不重新待选。原产品组合、叶项状态与验收层级保持不变。

<a id="generated-resource-source"></a>
### 功能状态卡：生成地点与资源预算的同一来源

Trace ID: O7-REQ-112440b1-da55-4443-b513-58dbf9056511

- 主责范围与成功标准：可玩大世界地点和资源优先来自既有 Simulator chunk/fragment 生成；可采量使用原 fragment/chunk total/remaining 账本，不允许运行时临时重算。
- 原义接收来源：[原 R2](../../world-simulator/scenario/unified-world-seed-fragment-runtime.prd.md#r2-复用现有-chunkfragmentresource-生成)；本卡只接收该段玩家/Agent/入口结果，原专业约束、条件及排除项保持。
- 对应验收：沿原 [AC1](../../world-simulator/scenario/unified-world-seed-fragment-runtime.prd.md#5-acceptance-criteria) 中本卡对应切片，不能用其他 AC 的局部通过代签。
- 系统设计状态：部分覆盖
- 系统设计依据：[unified 原设计条款](../../world-simulator/scenario/unified-world-seed-fragment-runtime.design.md#32-seed-派生)；原目标已存在，正式接线与四项未决仍限制覆盖。
- 实现状态：未核实
- 实现与验证依据：2026-10-17 固定候选静态复核：本地chunk/fragment账本基础存在；正式profile生成来源和真实消费者provenance同候选组合未核实。 源码/测试定位：[replay.rs:430–465](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/replay.rs#L430-L465)；[fragment_physics.rs:94–130](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/tests/fragment_physics.rs#L94-L130)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：只接收 R2 的来源和可采量承诺；chunk_seed 随机根、seed offset、公式和生成参数仍由原专业合同拥有，不增加材料、数值或采矿前置。 对应正式 profile 的输入、权威材料、真实消费者输出和完整验收仍须同候选核实。

<a id="committed-resource-continuity"></a>
### 功能状态卡：正式资源的提交来源与余额连续性

Trace ID: O7-REQ-2b3769ae-078a-4123-a0de-e20405b42c7c

- 主责范围与成功标准：正式生成事实必须由链上可验证记录承载或引用（沿原 R6）；未提交 preview 不进入正式采矿、生产、provider 或交易账本；remaining 由链上 action/commit 驱动，刷新与 replay 不以本地重算恢复余额，readiness 证明同一 provenance。
- 原义接收来源：[原 R6](../../world-simulator/scenario/unified-world-seed-fragment-runtime.prd.md#r6-资源生成全部链上化)；本卡只接收该段玩家/Agent/入口结果，原专业约束、条件及排除项保持。
- 对应验收：沿原 [AC5/AC6/AC7/AC8](../../world-simulator/scenario/unified-world-seed-fragment-runtime.prd.md#5-acceptance-criteria) 中本卡对应切片，不能用其他 AC 的局部通过代签。
- 系统设计状态：部分覆盖
- 系统设计依据：[unified 原设计条款](../../world-simulator/scenario/unified-world-seed-fragment-runtime.design.md#43-资源变更权威)；原目标已存在，正式接线与四项未决仍限制覆盖。
- 实现状态：未核实
- 实现与验证依据：2026-10-17 固定候选静态复核：ChunkGenerated重建与FragmentsReplenished entries应用是simulator机制；不是正式链资源提交、余额与readiness闭环。 源码/测试定位：[replay.rs:430–465](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/replay.rs#L430-L465)；[fragment_physics.rs:94–130](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/tests/fragment_physics.rs#L94-L130)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：只接收 R6 原资源权威结果；具体 action/event 或专用 resource record 保持正式未定，字段/hash/版本和共识规则仍归专业 owner，不由本卡新增。 对应正式 profile 的输入、权威材料、真实消费者输出和完整验收仍须同候选核实。

## 4. 路线图

1. 首局可读：目标、动作、阻塞、反馈和下一步可见。
2. 后引导承接：首局进入可持续的阶段目标与成长压力。
3. 世界参与：个人行动、Agent 和区域系统在一致规则下产生长期影响。
4. 成熟世界成长：小规模玩家通过可恢复能力、区域服务与有限区域影响形成独立 leverage，而不是被迫依附强组织或重复 grind；文明尺度共同项目保持自愿扩展。

## 5. Done：成功标准与验收

<a id="sc-1"></a>
- SC-1：玩家在首局可识别当前目标、可执行动作、行动代价与下一步。
<a id="sc-2"></a>
- SC-2：核心循环完整呈现行动接受、推进、阻塞、反馈和结果。
<a id="sc-3"></a>
- SC-3：FirstSessionLoop 之后存在可达的 PostOnboarding 目标、压力与承接。
<a id="sc-4"></a>
- SC-4：世界规则、资源消耗与玩家结果可映射到专业 PRD-ID 和验证证据。
<a id="sc-5"></a>
- SC-5：玩家的间接战略动作具备授权资源因果、权威校验和可审计后果，界面或接口可读 target/action/cost/blocker/result/next/recovery；涌现关系与组织不绕过权限、治理或 anti-abuse。未支持细粒度请求的单个或多个 canonical 替代可比较其目标/范围、成本、风险、预计后果、可撤回性与推荐理由；确认前无世界效果，新增承诺或不可逆后果必须明确授权，拒绝、过期、权限变化、重连或重试不会静默执行、继承越界权限或产生重复结果，没有安全替代时明确停止。
<a id="sc-6"></a>
- SC-6：代表性间接控制流程证明玩家意图进入 Agent/策略决策，经权威规则与资源校验产生世界后果，并返回可解释结果与可执行的打断、纠正、下一步或恢复动作；任一专业域的局部 green 不能替代组合闭环。
<a id="sc-7"></a>
- SC-7：同一物理行动在 gameplay、runtime、Agent 与 Viewer 的粗粒度/表现映射中保持距离、顺序、成本和持久化结果一致，不产生第二条时间线或表现层真值；权威时间线在没有直接玩家输入时仍按当前世界规则继续推进，不冻结具体 tick 时长。
<a id="sc-8"></a>
- SC-8：同一条代表性 Data 路径可端到端证明：预览不授予访问或收益；owner、recipient / 使用主体、purpose、scope 与授权状态可读；提交到结算之间授权过期、撤销、变更或无法证明有效时，未结算请求原子拒绝或保持可读待决，不产生 Data sink、访问收益或隐性义务，并提供重新授权或替代路径；有效结算只产生一次 receipt 支持的授权使用或转移。重试、重连、重复提交和历史 receipt 重放不产生第二次结果或复活失效授权，后续撤销不抹除既有合法结果的 provenance。具体许可状态机、时钟、结算规则、幂等实现与副作用矩阵由专业域拥有。
<a id="sc-9"></a>
- SC-9：成熟世界样例证明小规模玩家在不立即依附 major power 的前提下，通过可归因贡献获得新选择、恢复弹性、议价位置或区域用途；失败保留 repair / rebuild / pivot，区域影响不越界为全局治理权。
<a id="sc-10"></a>
- SC-10：产品样例证明世界没有强制通关条件，但每个阶段成果具有完成边界、可归因世界后果与下一阶段方向；长期成果只在新增选择、恢复弹性、局部议价/协调位置或区域用途时成立，不能以库存、吞吐或重复次数冒充成长。
<a id="sc-11"></a>
- SC-11：代表性首局、后引导与成熟世界样例保持一个当前主目标与低负担的继续/分支/换向选择；作用域、canonical 转译、校验、治理、反支配与审计在后台执行，只有改变资源、权限、锁定、恢复或共同承诺时才以可读原因和替代路径进入前台。
<a id="sc-12"></a>
- SC-12：代表性首局、持续目标与成熟世界样例证明玩家可通过短命令/复盘和有边界的离线授权维持普通成长，同时可自愿进入较长的区域、外交或治理会话；玩家无需先掌握全部系统深度或保持持续在线，且高风险窗口、授权范围与恢复路径可读。
<a id="sc-13"></a>
- SC-13：代表性区域冲突与赛季样例证明攻击只在已声明的 charter 范围和登记参与者/暴露资产间发生，非参与者受保护；实体领地/战利品结算、可恢复损失、软赛季刷新和系统性恢复均保留同一世界时间线、身份与可审计因果。
<a id="sc-14"></a>
- SC-14：代表性协作样例证明直接人类沟通不自动绑定，而 Agent 代表在有效授权、接受与权威校验后可形成可审计合同；持续服务的违约/救济、随机无冲突本地争端程序、情境化可更新声誉和预声明 R&D 归因均不产生永久污点、隐性权力或对个人政治 credential 的转让。
<a id="sc-15"></a>
- SC-15：代表性组织解散与长期不活跃样例证明 charter 不越过个人资产、合同、退出、历史和 Agent 身份的保护底线；风险冻结、合同与托管处理、责任/成本、持续业务处置、剩余分配，以及通知、保护期、可恢复主张、申诉和有限后续处置均留在同一可审计世界因果链中，不产生静默没收、身份删除、历史重写或重复生效。受影响玩家与 Agent 能区分正常、保护、受限持续、待决 reclaim/申诉与已解决状态，并读到适用的异议、申诉、等待、重新规划或安全停止路径，不把未确认结果表示为已完成。
<a id="sc-16"></a>
- SC-16：代表性资源、资格或持续义务路径证明预览不产生世界效果；请求入队/pending 与资源/规则已校验的承诺明确分层，未决请求不产生 hold/sink 或生产顺位；已接受承诺可读出范围、未决后果和下一步；已结算结果有 receipt 支持。报价与提交之间发生竞争性状态变化时，提交按当前条件至多结算一次并产生 receipt，或原子拒绝且不产生 sink/义务；`electricity_after` 不被误读为运行 runway 或不停机保证。过期、撤销、失效、重连重试和历史 receipt 重放不会制造第二次 sink、免费资源、资格续期或隐藏欠费，且未结算部分与既有已发生后果按相应专业合同保留可读 provenance、拒绝/待决或恢复路径。
<a id="sc-17"></a>
- SC-17：代表性共同决策样例证明普通治理只受理明示的政策、公共财库与既有 charter 日常事项；宪制、基本权利与系统安全事项被原子拒绝或路由到独立轨道，不产生部分世界效果。治理主体聚合、控制人影响上限、最小数据公开审计、纠错/申诉与历史 receipt（GCB-7），以及 OC 外部转让、治理 snapshot/退出/解除绑定与单向额度桥的隔离（GCB-8）必须保持可审计、不可追溯改票/双投且不产生世界治理效果。普通提案、财库、charter、技术升级、局部多数、历史声望与紧急状态均不能绕过保护底线；其他宪制变更只有在公开影响说明、延迟/复核、适用超多数、跨区域或受影响主体确认、独立审计与程序申诉全部成立时才生效。
<a id="sc-18"></a>
- SC-18：代表性区域设施与受治理扩展样例证明玩家只能在可读压力、条件报价、有限作用域和维护/耗尽/退役边界内取得区域 leverage；创作者提案只有经治理审查和权威生效后才成为世界能力。报价后状态变化、重复/过期/重连或跨入口重试只产生一次权威结果或原子拒绝，待决请求不会被表达为已生效。
<a id="sc-19"></a>
- SC-19：代表性系统性危机样例证明 containment 仅限制扩散并保护基本连续性；恢复项目在同一权威世界时间线内由玩家、Agent、组织或区域参与，并按既有资源、权限、治理、反滥用和申诉边界结算。不存在 reset、历史重写、跳过申诉或选择性 bailout；受影响主体可读触发事实、作用范围、当前限制、恢复/申诉路径与下一步。
<a id="sc-20"></a>
- SC-20：代表性工业任务证明报价无世界效果，提交只会原子拒绝或创建一次可追溯承诺；任务在完成、阻塞或中止时按适用专业规则守恒处置投入、在制进度、产出与副产物。进入阻塞时，玩家能读懂最早根因、占用/消费、保留/损失、稳定窗口影响、下一次权威复查边界与真实可用的继续/等待、修复、改道、降载/拆分、重新规划或中止路径；每个展示候选逐项具备作用目标/范围、追加成本与仍占用、预计结果/复查时延、失败/损失风险、可撤回性及推荐理由，缺字段不得展示为可选；不存在安全路径时能明确停止并获得下一次可决策点。目标换向、停机、权限变化、重连、重试与 replay 不会静默取消/迁移任务、重复 sink/产出、释放或退款，或同时生成退款与完成结果；同一因果计划的恢复保持 root continuation，因果变更建立链接 revision/candidate 并从 `W=0` 重新计数。
<a id="sc-21"></a>
- SC-21：代表性多阶段流水线证明下游只能消费已经结算、实际到达适用账本且满足该阶段规格/品质适用范围的中间品；阶段/边身份、可用/预留/加工中/在途状态、批次适用结论与来源、有限缓冲及根因/派生 blocker 可追溯。来源未知、证据不足或不兼容的批次在下游 sink/进度前 fail closed，并让玩家区分适用、待验证/证据不足与不适用；不得静默推断、降级、混料、转换或免费 salvage。多输入阶段以 canonical join identity 绑定完整 parent-edge/receipt set；默认在同一快照齐套前无 input sink/WIP/进度，只有 profile 明示 staged intake 时才能逐 parent 单次消费并保留可读 remaining obligations，缺失/无效/过期/换线 parent 不得形成隐藏 partial kit 或复用 receipt。多个已接受意图争用同一阶段/边/buffer 时，基于同一权威快照产生完整/部分/延期/拒绝结果，不超额 hold、不靠提交顺序或重试插队；释放或过期只触发一次 fresh-snapshot 重评，仍有效意图不会被无说明地无限跳过。声明 end-to-end service window 时必须绑定 stage/edge/buffer/terminal reservation chain、权威时界与 none/soft/hard policy；各边界重验剩余义务，expiry/release/renewal 只生效一次且不靠重连/replay 延期，production on-time 不能冒充 late/expired delivery。声明 performance/diagnostic window 时必须绑定 candidate/config epoch 与 journal/state-root；submit/allocation 为每个 accepted outcome 原子冻结一次 immutable plan baseline，preview/atomic reject 不创建，后续 replan 只能创建 parent-linked revision，actual receipt 不跨 revision 或按新配置回填；计划/承诺/执行/生产/交付分层，backlog/join/WIP/transit/buffer/terminal buckets 互斥，receipt 对账损耗与补偿，primary root blocker 不与派生影响重复计数，cutover/replay 不混窗，缺证据显示 unknown 而非伪造。共享容量的 partial 不自动允许 partial recipe：每个 recipe/factory profile 声明合法执行单位、full-only/partial 策略、确定性量化与 residual 处置，结果可追溯 requested、committed、executed、unmet 及主/副产物总量；不足合法单位不扣料、不产半批、不计稳定进度。一个 cycle 的主产物与全部副产物必须属于同一 output bundle；默认在任何 output credit/progress 前原子预检并结算全部 branch，只有 profile 明示 split fan-out 时才能逐 branch 单次结算并保留 failed-leg pending/residual，不能因一条 branch 失败而丢失/复制另一条或伪造 bundle 完成。下游不可用或缓冲满时，上游按声明的背压规则保持、入缓冲或原子拒绝，不静默丢弃、瞬移、无限堆积或伪造完成。阶段/配方/设施/边的因果替换必须在单一 cutover 边界为旧 hold、在制、在途和缓冲批次产生一次显式处置，新候选不继承旧稳定进度、reservation、receipt 或里程碑；不支持的迁移 fail closed，支持的 handoff/conversion 保留父 receipt、数量与损耗。终端阶段必须把 production 与 delivery/settlement 分开：最终产物只进入 owner-bound 有限账本、明确交付承诺或受支持的 hold/quarantine，非 settlement receipt 不减少需求、不发放交付奖励或终端里程碑；终端失效保持有界 blocked/待交付或原子拒绝，不产生无限库存、隐式销毁/转卖或虚假交付。拆分/汇合、跨账本运输损耗、支持的隔离/复验/返工/替代去向、重连、重试、恢复与 replay 保持确定性资源守恒，每阶段至多一次 sink、产出和里程碑效果，并让玩家读懂吞吐、输入齐套、批量、输出 branch、批次用途、共享容量、服务窗口、诊断窗口、换线与终端交付机会成本及恢复选择。该标准还必须覆盖 extraction/replenishment→refinement→source-to-ledger handoff→首个 canonical consumable ledger input 的玩家路径：preview/source-settled/in-transit/arrived/applicable/blocked 各状态可读且不互相冒充；实际数量、损耗、owner/账本、parent provenance 与适用结论守恒可追溯；失败可按专业合同等待补充、补电、重报价、改道、隔离、返工、返还、补偿或放弃，不能自动退款/免费转化；重复 submit、arrival、重连、乱序与 replay 不复制 source effect、目的 credit、首个 input sink、稳定进度或奖励。产品承诺止于首个 canonical input，字段、公式、物流/账本算法、runtime/Viewer 实现由专业 authority 负责。

<a id="sc-22"></a>
- SC-22：代表性产线维护/计划停机样例证明排产前可比较 `maintain_before_run`、`run_at_risk`、`reduce_load` 与 `defer` 中真实支持的路径，并读到维护目标/范围、追加成本与仍占用、预计产出/复查时延、故障/损失风险、可撤回性与推荐理由；维护真值缺失时显示 `maintenance_not_tracked` 而不伪造安全继续。若 submit boundary 仍为 `unknown`，会产生 irreversible sink、停机/稳定影响或交付承诺的候选不得 selectable，必须 atomic reject 或保持有界 pending（不得产生隐式 hold）并给出下一次权威复查；只有专业合同真实支持且明确标注 `at-risk/unknown` 的路径可继续展示，preview 无 authority。报价无世界效果，提交按当前状态重验且前置漂移原子拒绝并保留原任务/未结算状态，成功只产生一次维护/排程结果，计划停机不伪造稳定/交付，重连、重试与 replay 不复制 sink、停机结果或里程碑。
<a id="sc-23"></a>
- SC-23：代表性 `BuildFactory` 路径证明建设报价只读且绑定 owner/site/kind/id、candidate/config/world revision 及稳定 quote correlation；玩家可比较 build-now、补料、移站、补电和延期，并逐项读到 owner-held electricity 与全部 construction inputs 的 kind/quantity/ledger/before/after、power mode、profile 选定的 output boundary、主要 blocker、下一复查点与首个工业目标关联。每个可选项都说明目标/范围、追加成本与仍占用、预计结果/复查时延、风险/损失、可撤回性与推荐理由，未实现或前置不可证明的选择不得 selectable。提交按当前权威状态重验，竞争性漂移只能产生一次当前条件 receipt 或无 sink 的原子拒绝；任一输入不足、地点/chunk/owner/kind/existing-or-pending ID、未知 authority 与未冻结 mode 不得伪装为可支付或已预留；四类 mode 各自的 sink/hold/revalidation/best-effort 和争用/unknown 边界可验证，重连、重试、重复提交、恢复与 replay 不复制 sink、设施、激活、发电或奖励。建设成本、配方电力、maintenance 与 battery runway 分层，PowerPlant 仅在 profile 声明的 output boundary 后生效；Viewer 与 pure API 对上述事实保持一致。该 SC 只冻结玩家承诺与验收，不冻结公式、字段、队列、UI 或当前实现完成声明。
<a id="sc-24"></a>
- SC-24：代表性配方生命周期证明 `pending_validation`、`validated_pending_admission`、`active`、`restricted`、`retiring`、`retired` 六状态及其转换条件可读，且生效前无世界效果；预览与本 SC 使用同一状态集，授权/兼容/证据问题作为 blocker reason 单独表达。每个 accepted 任务固定不可变 recipe version/authority，并与 factory capability、输入批次、物流路径、output bundle 和 terminal purpose 关联；`agent_engineer` 必须能验证 Agent 只能在授权作用域内提议/确认，不能以推荐或重试改变状态。改变输入/输出/副产物、规格/品质、阶段/边、power/logistics 或 terminal 资格等产出因果内容必须新建 parent-linked 版本/候选并从 `W=0` 开始，不能继承旧 receipt、reservation、队列优先级、进度或奖励；非因果元数据才可保留身份，无法证明则 fail closed。版本/授权/输入规格/factory fit/路线/容量/power/terminal 证据缺失或漂移时，在首个不可逆 sink 前原子拒绝或保持无效果待决，玩家可见 primary blocker、机会成本和复查点，不能同名替代、免费转换、静默降级或伪造退款。六状态动作矩阵必须分别覆盖新排程、accepted 未产生首个不可逆 sink、WIP、在途、buffer 和 terminal-pending：`active`/作用域内 `restricted` 才可 fresh revalidation 后排程；其余状态不可新排程；retiring/retired 不得自动迁移 successor。退役禁止新排程但保留历史；`retiring` 的未开始、WIP、在途、buffer、terminal-pending 任务分别按各动作适用 profile 取得一次受支持的 finish/hold/rework/salvage/return/改道/handoff/reject/终止处置，缺少 drain/finish 能力不禁用另有适用 profile 的合法安全处置；`retired` 仅保留历史读取/回放与已记录的最终处置，该通用动作清单不为其授予新生产或处置效果；terminal-pending 必须保留 recipient、destination、admission、capacity obligation 与 provenance，不得提前 settlement、静默改绑或重复释放容量义务。兼容 successor 需 fresh revalidation、明确授权和 parent-linked conversion，重连/retry/replay 不复活旧版本或复制效果。权威追踪同时覆盖 `PRD-WORLD_SIMULATOR-047` / `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md` 的批次/适用性/处置语义，以及目标 `PRD-WORLD_RUNTIME-043` / `doc/world-runtime/prd.md` 的 identity、parent linkage、幂等 finality 与 replay 边界。`test_tier_required` 覆盖上述生命周期、转换、漂移、退役、兼容、六类动作和 terminal-pending 单次处置/settlement 边界，`test_tier_full` 覆盖多阶段在制/在途/缓冲/terminal-pending、持久化/replay、Agent 重试和 Viewer/pure API/Agent parity；产品层不冻结 recipe ratio、授权角色、兼容算法、runtime schema、UI 或当前实现声明。
<a id="sc-25"></a>
- SC-25：代表性已建成工厂能力生命周期证明，玩家能在工厂仍为当前能力候选时比较 `upgrade_existing_capability`、`reconfigure_recipe_fit`、`run_at_current_capability`、`reduce_or_defer`、专业合同支持的 `retire_and_rebuild` 与 `defer`；每个可选项必须说明目标/作用域、追加资源/维护成本、停机或容量机会成本、对吞吐/物流/终端承诺的影响、受影响的 accepted-unstarted/WIP/in-transit/buffer/terminal-pending 工作、风险、可撤回性与下一次权威复查点。预览只读且不产生 hold、队列、输入 sink、退款或能力效果；提交必须按最新 owner、factory fit、recipe/规格、power、logistics、capacity、terminal 与治理 authority 重新校验，未知或未追踪 authority 只能显示 `unknown/not_tracked`，不得按零成本、安全或可兼容处理，涉及不可逆 sink 的候选必须原子拒绝或保持有界待决。
<a id="sc-26"></a>
- SC-26：代表性 `BuildFactory → factory-ready/recipe-missing → recipe lifecycle decision/admission → ScheduleRecipe → join → stage → transit → buffer → terminal` 路径在展示为当前主推荐前，Starter Industrial Feasibility Gate 必须从同一 fresh authority snapshot 返回 `candidate_available` 或 `no_safe_starter_chain`；`candidate_available` 绑定 stable starter-chain/candidate identity 与 authority snapshot/version，`current-evidence-backed`、`target-contract`、`unknown/not_tracked` 分别保持当前可证、未来目标和 fail-closed 边界，`no_safe_starter_chain` 必须保留 blocker、恢复路径与复查边界；test tiers 是验收目标而非 current pass，只有 fresh composite runtime + QA evidence 才能支持 current claim；通过该闸门的路径证明产品读面能把 factory/capability、recipe version、原料批次适用性、物流边/容量、power/maintenance、output bundle 与 terminal purpose 收束为 `准备/待开工`、`运行中`、`产出待交付`、`已结算` 四类玩家决策；每类都显示 primary blocker、已占用/消费价值、`next_action`、`next_recheck` 与 `progression_effect`，且 `BuildFactory` receipt、recipe preview/admission、`accepted`、`produced`、`buffer-held`、`terminal-pending` 不被误报为生产、交付或成长。任何生产奖励/能力里程碑至少等待首次 production receipt；`production-only` 的匹配 production receipt 完成一次首产物目标并保持 produced/undelivered，稳定条件另行升级 production-stable 且不减少交付需求，terminal-admission 目标只有取得匹配的 delivery/terminal settlement receipt 才能完成并打开交付型下一候选；缺料、批次/配方/工厂不兼容、物流/电力/终端失效与 unknown authority 保持可读 blocked/fail-closed 及真实恢复路径。因果换线创建 parent-linked 新目标并将 `W` 归零；同一计划继续保持 root continuation；Viewer、pure API、Agent 与 replay 对状态、根因、下一步和一次性完成结果一致。产品层不冻结 runtime schema、队列、配方/产率/价格公式或 UI；`game` 拥有目标节奏与平衡，M4/runtime 拥有 lifecycle/receipt/守恒/幂等，QA 拥有组合验证。
  - 改变产出因果的能力、支持 recipe version、输入规格、阶段/边、output branch、power/logistics 或 terminal fit 必须创建 parent-linked 新 capability candidate，并在单一 canonical cutover 边界生效；cutover 前的 hold、accepted-unstarted、WIP、in-transit、buffer、terminal-pending 与 receipt 保留旧 candidate 身份，cutover 后的新意图只能归属新 candidate，且新 candidate 从 `W=0` 开始，不继承旧 queue 顺位、reservation、稳定窗口、receipt、里程碑或奖励。只改变非因果展示元数据才可保留身份，无法证明时 fail closed。
  - `upgrade/reconfigure` 或 `retiring/retired` 不能静默迁移既有工作：accepted-unstarted 只能按 profile `hold/release/replan/terminate`，WIP 只能按 profile `finish/pause/hold/rework/salvage/terminate`，in-transit 只能使用已声明的 `finish/hold/return/reroute/reject`，buffer 只能使用已声明的 `hold/handoff/conversion/reject`，terminal-pending 只能继续原合法 commitment 或使用已声明的 `hold/return/reroute/handoff/reject`，并保留 recipient/destination/admission/capacity obligation/provenance；每项旧工作至多一个处置 receipt。`retiring/retired` 禁止新排程并保留历史；successor 不能自动继承旧任务，只有在明确授权、fresh revalidation 和 parent-linked conversion 成立时才可转换。不得免费补发产出、隐式退款、销毁/瞬移/转卖库存，或同时产生退款与完成。
  - 工厂能力的玩家可读状态类至少区分 `operational`、`reconfiguration_pending`、`reconfiguring`、`degraded`、`retiring` 与 `retired`；这些是产品表达，不冻结 runtime enum。`degraded` 只有在专业 profile 明确允许且标注 `at-risk` 时才能接受新工作。所有升级、退役、处置、释放和转换必须沿同一 factory/capability/recipe/lineage identity 至多生效一次；重连、重试、重复提交、Agent 推荐或 replay 不得重复 cutover、释放 hold、复制产出/奖励、改变稳定窗口或复活旧 candidate。该规则强化 world-first 的真实能力变化、emergence-first 的有边界玩家选择、persistent 的历史连续性、auditable 的 parent receipt/cutover/成本 provenance 与 extensible 的 profile/治理扩展边界。
   - `test_tier_required` 覆盖 preview 无效果、报价后 drift 的拒绝/重报价、上述五类旧工作 bucket 的逐项处置、升级/退役单次 cutover、recipe-fit 与 candidate/receipt/reservation/W 隔离、unknown authority、retired 不可复活，以及 Viewer 与 pure API 的状态/机会成本/下一动作一致性；其中 `terminal-pending` 必须验证 recipient/destination/admission/capacity obligation/provenance 保留、单次 disposition，且不得提前 settlement 或静默改绑。`test_tier_full` 覆盖并发工作与共享容量、连续升级、cutover 部分失败、crash/restore/replay、持久化、多阶段 WIP/transit/buffer/terminal-pending、Agent 重试和 successor conversion。产品承诺链接 `doc/game/prd.md`、`doc/world-runtime/prd.md`、`doc/world-simulator/m4/industrial-resource-flow-contract.prd.md` 与 `doc/testing/prd.md`，不复制其 runtime、WASM、物流算法或测试实现。


<a id="sc-27"></a>
- SC-27：代表性 `factory-ready/recipe-missing → recipe candidate discovery → recipe_selection_preview → ScheduleRecipe` 路径证明候选只从同一权威快照派生，并区分 `candidate_available`、`candidate_blocked` 与 `candidate_unknown`；候选能同时说明当前目标、recipe version/authority、factory fit、原料适用/缺口、物流/容量、power/maintenance、output bundle/终端、机会成本、主要风险、`next_action` 与 `next_recheck`。同名版本不合并；available/blocked/unknown 候选均可读取无副作用的 `ScheduleRecipe` 排程预览，blocked/unknown 只解释缺失前置与恢复。只有 `candidate_available` 且最终新鲜校验通过才能提交；选择 preview 不产生 activation、hold、input sink、queue、物流 reservation、产出、稳定进度或奖励；准入/原料/物流/电力/终端漂移必须重发现、重报价或原子拒绝，不能自动换配方。无候选时玩家获得明确原因与补料、补证、改路、改厂、等待或延期中的真实路径；选择、排程、生产、稳定与交付结果分层，重连、重试、恢复与 replay 不自动选择或复制效果，Viewer、pure API 与 Agent 对候选和下一步保持同义。产品层不冻结 recipe catalog、解锁树、兼容算法、runtime schema、UI 或当前实现完成声明。
<a id="sc-28"></a>
- SC-28：至少一份代表性配方执行档案把同一 candidate 的 factory capability、pinned recipe、两类 required raw-material inputs 及其 batch/quality/custody、effective logistics path/loss/capacity、canonical executable cycle、power mode、主/副产物 output bundle、terminal purpose 与 progression boundary 收成完整只读投影；缺失任一 authority 时为 `incomplete/unknown/blocked`，不宣称可排程、稳定或交付。单一 deterministic fixture 覆盖齐套成功、input 不适用、物流满后重评、power 漂移、mandatory output destination 失效以及 production-only/terminal-admission 两种完成边界；失败在首个不可逆 sink 前延期/原子拒绝，已发生 WIP/transit/branch effect 只按 profile 单次处置。未选路径不取得 hold/优先级/进度，因果改变建立 parent-linked 新 candidate 并从 `W=0` 开始；retry/reconnect/replay 不复制材料、容量、产出、交付或奖励，Viewer/pure API/Agent 对完整档案、blocker、机会成本和下一步保持同义；Agent 只可从同一权威候选与当前允许动作继续，unknown/blocked 不生成可提交动作，provider 建议不代替输入适用性或执行 receipt。产品层不新增 runtime schema，不冻结配比、产率、价格、容量、tick、队列或 UI。
<a id="sc-29"></a>
- SC-29：代表性 `BuildFactory` 选址路径证明同一工业目标下至少两个合法 site candidate 可比较原材料输入与产物/终端输出路径、容量/损耗、power 前置、终端适配、建设成本、交付风险和扩张机会成本，并区分 `site_available`、`site_blocked`、`site_unknown`。未固定 recipe 时，recipe-specific 输入/输出拓扑、容量/损耗与 terminal/power fit 仅为 advisory estimate，不得成为普遍建厂前置；只有 site-level facts 与 profile 明示的 construction prerequisite 进入 `BuildFactory` fresh revalidation，recipe 固定或进入 `ScheduleRecipe` 后再验证其余 topology。预览只读且不创建设施、扣资源、锁定 site/edge/buffer 或改变顺位；建设提交时只有 site-level facts、profile 明示的 construction prerequisite 或已固定 recipe 的建设前置发生漂移，才触发当前条件重报价或无副作用原子拒绝；未固定 recipe 的 advisory topology 漂移不阻断建厂，而在 recipe candidate discovery/`ScheduleRecipe` 时重读。改变产出因果的 site/topology 必须建立 parent-linked 新 factory/capability candidate 并从 `W=0` 开始，不得自动迁厂、改道、降级路线或迁移旧 accepted-unstarted/WIP/in-transit/buffer/terminal-pending 工作；terminal-pending 保留 recipient、destination、admission、capacity obligation 与 provenance 并只接受一次合法处置。玩家可读 primary blocker、机会成本、下一复查点与真实的等待容量、补料/补电、改路、改站点或延期路径，无合法恢复时明确停止。`test_tier_required` 覆盖双站点比较、上述建设前置报价后漂移、advisory topology 漂移不阻断建厂且在 recipe discovery/`ScheduleRecipe` 重读、因果换站点、五类旧工作不迁移及其 parity、无合法站点和 retry/reconnect/replay 单次效果；`test_tier_full` 覆盖拓扑争用、五类存量处置、恢复与 relocation/cutover profile。产品层不冻结选址/寻路算法、站点数值或 UI。
<a id="sc-31"></a>
- SC-31：代表性 demand goal 证明玩家在提交前能从同一权威快照比较目标量、已承诺/生产/交付量、canonical batch quantum、预计缺口/匹配/盈余、输入/电力/物流/buffer/terminal 占用、机会成本与下一复查点，并只看到专业合同真实支持的 full/reduced、补料/调运、持有盈余、停止/延期或 parent-linked 补产路径。Production receipt 不减少需求或发放交付奖励；profile 唯一声明的 matching delivery 或 terminal settlement 才更新目标满足量，非 matching settlement 不重复减少需求。目标已满足后不得由旧 schedule、后台循环或 Agent retry 静默追加生产；合法批量造成盈余时不得自动倾销、销毁、伪造成交或计为成长；matching settlement 不足时，选择补产形成一次可追溯 supplemental revision 并保留原 baseline、actual、损耗、已满足量与剩余缺口，选择停止或延期则保留 shortage 且不创建 revision。目标、库存、批量、产率或终端容量漂移触发重报价、无副作用原子拒绝或 profile 明示的有界 pending；重复 submit/delivery、重连、乱序、恢复与 replay 不复制生产、交付、需求减少、盈余处置、奖励或容量释放。Viewer、pure API 与 Agent 对 demand 状态、数量分层、blocker、动作与复查点保持同义；产品层不冻结预测、阈值、价格、自动补货/停机、订单簿、runtime schema、队列或 UI。 跨区市场还须把情报来源/新鲜度、报价与物理路线、到达、目的地存储接收、所有权及 escrow 里程碑分别表达；延误、损耗、拒收和争议不得凭前序状态结算，重连/replay 保留合法恢复与单次效果。
<a id="sc-30"></a>
- SC-30：代表性输入阻塞样例能区分同料换来源、profile 明示的异料替代与换配方，并比较等待/补料、合法替代、换配方、减量/延期或 pivot 的数量、比例/rounding/residual、损耗、power/物流/terminal 占用、已持有/消费价值、产出/value class、风险、`W` 影响与下一复查点。异料替代默认禁止；缺失 authority/provenance/ratio/适用性显示 `unknown/blocked` 或 `no_legal_substitute`，不会被 `degraded`、同名材料或 Agent 推荐包装为安全候选。预览无世界效果，明确确认后按新鲜权威状态只建立一次 decision；decision-only 样例无 sink/credit/conversion/`W`/奖励，只有 profile 明示且实际结算 conversion 时才另有一个 linked exactly-once settlement/provenance receipt。只有产出因果、质量、power、时间、副产物、terminal 与 value class 全部不变的 profile-declared substitute 才可保持 pinned recipe 并建立一个 child decision，其他替代或换配方建立 parent-linked 新 candidate 且 `W=0`。首个不可逆 sink 后只允许专业合同支持的单次 rework/conversion/return/hold/salvage；retry/reconnect/restore/replay 不复制 sink、conversion、receipt、产出、进度或奖励，也不能通过廉价替代、反复切换或拆分提交套利。
<a id="sc-32"></a>
- SC-32：代表性制度扩展样例必须使用本 PRD 的 canonical mapping，证明现有 native `Alliance`、`War`、`Governance`、`EconomicContract` 各自且仅各自落入一个 `Kernel keep`、`Module candidate`、`Compatibility keep` 或 `Defer` 分类；当前映射为 `Alliance=Compatibility keep`、`War=Defer`、`Governance=Kernel keep`、`EconomicContract=Compatibility keep`。未完成该互斥分类前不得新增 native 制度 action/state。`Alliance` 或 `EconomicContract` 只能择一在治理批准的 activation boundary 转为唯一 `Module candidate`，另一者保持 `Compatibility keep`；试点必须在不绕过 L0 物理守恒、L1 治理边界、CapabilityGrantV2 owner-bound budget 与 receipt/审计约束的情况下，保留旧快照、旧回放、既有玩家结果和单一权威时间线。玩家可见的预算 owner/payer、报价/可负担性与 `unknown/degraded` 状态必须可解释；preview、stale provider response、deny、无效果失败不扣预算，实际 effect 与预算 debit 必须随 receipt 原子且仅结算一次，retry/reconnect/restore/replay 不得重复扣费或发放 replay credit；预算单位、公式、wire/ABI 字段、runtime schema 与模块实现仍由专业 authority 定义。迁移验收覆盖 replay、snapshot、版本/ABI 兼容、单次结算、预算守恒与失败恢复；该验收只约束玩家可观察的连续性和跨域合同，不冻结 ECS、World Database、分片或模块 ABI。
<a id="sc-33"></a>
- SC-33：可选的 `industrial_externality` profile 必须把 `operational_mitigation` 限定为工厂/配方 operation 的局部、可审计义务；无 profile 的 legacy 路径不变，profile/作用范围/必要 authority 缺失或过期时为 `unknown/blocked`，不可选择不可逆运行。预览只读，提交必须 fresh revalidation；已支持的运行、减量/延期或 hold 路径只能至多产生一次 linked production 与 externality/mitigation disposition，且不改变 quality/custody、byproduct、maintenance、production/delivery settlement 或稳定窗口。WIP、in-transit、buffer、terminal-pending 均按原 root/revision 单次处置；terminal-pending 保留 quantity、recipient、destination、admission、capacity obligation 与 provenance，直到 matching settlement 或一次受支持处置，不得静默改绑或提前 settlement。玩家可读吞吐、缓释占用、机会成本、根因和复查点，但不能由此取得区域 credit、治理/claim 权利或改变共享环境真值；危机 containment、区域/系统性影响、污染/环境公式、积累/衰减/扩散、清理经济与 runtime/API/schema/UI/Agent 实现均另行定义。重复提交、重连、Agent retry、restore、乱序与 replay 不复制生产、外部性、progression 或 reward；Viewer、pure API 与 Agent 对状态、根因、成本、动作、旧工作处置和下一步同义。该标准是 target-only，只有 fresh M4/runtime + gameplay + QA composite evidence 才能支持 current claim。

<a id="sc-34"></a>
- SC-34：代表性连续性治理产品样例把 CR-3 的授权治理（GCB-001/002/003）、CR-4 的高影响保护与复核边界（GCB-006，并与紧急保供/危机恢复的既有产品语义相交但不重复）以及 CR-5 的长期经济审计与同候选发行证据（MW-10/MW-12、[`REQ-WR-MW-005`](mature-world-progression.prd.md#req-wr-mw-005) / [`AC-WR-MW-005`](mature-world-progression.prd.md#ac-wr-mw-005)、PE-002/003、AR-4、[`REQ-ENTRY-CANDIDATE-001`](../player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001) / [`AC-ENTRY-CANDIDATE-001`](../player-entry-distribution/access-modes-and-release-readiness.prd.md#ac-entry-candidate-001)、[`ENTRY-CLAIM-001/003`](../player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001)）分别绑定到稳定产品 REQ/AC；其中 MW-12 明确接收非 levy 资源 source/sink、关键经济变化、治理影响与异常不被局部 green 掩盖的产品边界。runtime、P2P、恢复、经济审计、QA 与公开 claim 必须来自同一候选证据窗口；单节点、局部或历史 green 不得代签，本文不保存当前 readiness 结论。

### 5.1 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| SC-1 | gameplay_designer | PRD-GAME-004 | `doc/game/prd.md` | 首局玩法与可读性证据 | test_tier_required |
| SC-2 | gameplay_designer | PRD-GAME-004 | `doc/game/prd.md` | micro-loop 端到端回归 | test_tier_required |
| SC-3 | gameplay_designer | PRD-GAME-007 | `doc/game/prd.md` | PostOnboarding 转换与持续游玩证据 | test_tier_required |
| SC-4 | qa_engineer | PRD-GAME-003 | `doc/game/prd.md` | PRD-ID 到发布验收证据的追踪检查 | test_tier_required |
| SC-5 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-002 / PRD-GAME-004 / PRD-GAME-013 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-P2P-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | `test_tier_required` 覆盖单/多候选比较、确认前无效果、拒绝/过期/权限变化与重试不重复；`test_tier_full` 覆盖跨入口并发确认、授权收缩或替换、原请求到替代与唯一 receipt 的可审计关联 | test_tier_full |
| SC-6 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / viewer_engineer | PRD-GAME-014 / PRD-WORLD_SIMULATOR-001 / PRD-WORLD_RUNTIME-001 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/world-runtime/prd.md` | 玩家意图、Agent 决策、权威后果与打断/纠正/恢复组合证据，含正式玩家 surface 的 S6 交互闭环 | test_tier_required |
| SC-7 | producer_system_designer / gameplay_designer / runtime_engineer / viewer_engineer | PRD-GAME-002 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md` | 无玩家直接输入时的持续时间线，以及物理真值与粗粒度/表现映射一致性审计，含 S6 表现层核对 | test_tier_required |
| SC-8 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-002 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | Data 预览无效果、授权中途失效无 sink/收益、单次结算、retry/reconnect/replay 不重复、既有 provenance 保留与恢复路径证据，含结构化 pure API 和 S6 正式玩家 surface 的状态/原因/下一步可读性 | test_tier_required |
| SC-9 | producer_system_designer / gameplay_designer / qa_engineer | PRD-GAME-015 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/testing/prd.md` | mature-world player leverage、anti-grind、恢复与有限区域影响 fresh sample；产品合同见本模块的 mature-world 专题分册 | test_tier_full |
| SC-10 | producer_system_designer / gameplay_designer / qa_engineer | PRD-GAME-007 / PRD-GAME-015 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/testing/prd.md`; 本模块的首局与成熟世界专题分册 | 阶段成果、三条长期抱负轴、anti-grind 与无强制终局的组合审计 | test_tier_required |
| SC-11 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / viewer_engineer | PRD-GAME-004 / PRD-GAME-007 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md` | 一个当前主目标、继续/分支/换向与仅在实质相关时显现的后台护栏组合证据 | test_tier_required |
| SC-12 | producer_system_designer / gameplay_designer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-007 / PRD-GAME-014 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 分层信息、短命令/复盘、已授权离线推进、可选深度会话与高风险有界响应的组合体验证据 | test_tier_required |
| SC-13 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / blockchain_ops_engineer / qa_engineer | PRD-GAME-002 / PRD-WORLD_RUNTIME-001 / PRD-P2P-001 / PRD-TESTING-003 | `doc/product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md`; `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 参战范围、离线防御、占领/提取、可恢复重建、赛季刷新与统一世界连续性的组合证据 | test_tier_full |
| SC-14 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / viewer_engineer / blockchain_ops_engineer / qa_engineer | PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-P2P-003 / PRD-TESTING-003 | `doc/product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md`; `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 授权/接受合同、atomic 与持续服务、争端 receipt/程序申诉、声誉/转让与 R&D provenance/份额的组合证据 | test_tier_full |
| SC-15 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / blockchain_ops_engineer / viewer_engineer / qa_engineer | PRD-GAME-002 / PRD-WORLD_RUNTIME-001 / PRD-P2P-003 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md`; `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; [world-simulator 权威](../../world-simulator/prd.md#active-requirement-baseline); `doc/testing/prd.md` | charter 保护底线、解散 waterfall、通知/保护期、estate 或可撤销 delegation、reclaim/appeal、持续业务处置和历史/receipt 连续性的组合证据；另承接 [OC-5](organization-continuity-dissolution-and-dormancy-protection.prd.md#5-组合验收) 的正常/保护/受限持续/待决/已解决状态权威对账与正式玩家 surface 可读性证据，覆盖受影响玩家/Agent 的当前状态、适用下一步和未确认结果边界；OC-5 状态可读性子项保持 `test_tier_required` | test_tier_full |
| SC-16 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-002 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 同一条资源/资格/持续义务路径的预览、入队/pending、专业域接受承诺、结算、失效、重试与 receipt 重放负例组合证据；必须含“报价后竞争性状态变化”的接受一次/原子拒绝样例，以及正式玩家 surface 对余额、runway/停机风险、预览/待决/已结算的区分，不伪造自动恢复 | test_tier_full |
| SC-17 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / viewer_engineer / qa_engineer | PRD-GAME-002 / PRD-WORLD_RUNTIME-001 / PRD-P2P-003 / PRD-TESTING-003 | `doc/product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md`; `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 普通事项白名单、排除事项原子拒绝/独立路由、宪制条件全满足/缺失、防绕过与玩家可读 receipt 因果，以及 GCB-7 的治理主体聚合/控制上限/最小数据审计/纠错申诉/历史 receipt、GCB-8 的 OC 转让与 snapshot/退出/额度桥隔离（不可追溯改票/双投、非治理桥）组合证据 | test_tier_full |
| SC-18 | producer_system_designer / gameplay_designer / runtime_engineer / wasm_platform_engineer / blockchain_ops_engineer / viewer_engineer / qa_engineer | PRD-GAME-016 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-010 / PRD-P2P-002 / PRD-TESTING-003 | `doc/product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md`; `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 区域设施生命周期、扩展治理准入、报价后状态变化、单次权威结果、待决表达与 replay/恢复身份一致性的组合证据 | test_tier_full |
| SC-19 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-002 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-P2P-001 / PRD-P2P-003 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md`; `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | containment 与恢复项目在同一世界时间线中的组合证据；无 reset/历史重写/选择性 bailout，保留 appeal、replay/provenance，并区分正式玩家 surface 的 containment、恢复中、已恢复与 blocked；详细 [AC-6](chartered-conflict-soft-seasons-and-recovery.prd.md#ac-6)/[AC-7](chartered-conflict-soft-seasons-and-recovery.prd.md#ac-7) 与实现合同由上述权威维护 | test_tier_full |
| SC-20 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-002 / PRD-GAME-004 / PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 正常完成、提交前状态变化、开始前/后的适用中止或无中止能力、设施/维护/权限中断、目标换向、重连/重试/replay 的资源守恒与单次结算证据；Viewer 与 pure API 对任务阶段、投入/产出、保留/损失、稳定产线影响和下一步保持一致，Agent 打断保留旧意图 handoff | test_tier_full |
| SC-21 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-002 / PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_SIMULATOR-001 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/game/gameplay/gameplay-industrial-pipeline-service-window-contract.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | `test_tier_required` 覆盖两阶段正常结算、低于规格、规格来源未知、下游缺料/缺电/物流未到达、缓冲满背压，以及 2–3 条 parent edges 按不同顺序到达的 join：atomic profile 在全部 parent 齐套前无 sink/WIP/progress，缺失/无效/规格不符 parent 保留可读 join_pending/held/unmet；supported staged profile 明示逐 parent 消费、remaining obligation 与 disposition，late arrival 只重评未决 parent，重复到达/retry/replay 只产生一个 join receipt；staged-intake 部分 hold 形成 wait cycle 或无法证明可收敛时，在新增 sink/WIP/progress 前 fail closed，cycle root、held/unmet、重评边界与 release/defer/terminate/compensation 路径可读，重评/释放/replay 不产生无界 hold、隐式回滚或第二次效果。至少两个已接受意图对同一阶段/边/buffer 的容量不足争用须在同一快照和 contender 集下不受提交/网络顺序影响，不超额 hold，完整/部分/延期/拒绝、根因、剩余需求、机会成本、重评条件和恢复动作可读；释放/过期只重评一次，retry/reconnect/replay 不插队或重复效果；一条 multi-tick stage→transit→buffer→terminal 路径分别验证 none/soft/hard window，所有 leases 有效时只结算一次，任一边界的 owner/route/capacity/expiry 漂移产生一次 late/hold/reject 与一次 release，无 orphan hold/隐藏 sink，生产准时但 transit/terminal 迟到时只有 production receipt；同一 journal/state-root 与 measurement boundary 在 replay 后得到相同 diagnostic snapshot，requested/committed/executed/produced/delivered 不混计，backlog/join/WIP/transit/buffer/terminal buckets 互斥，损耗/补偿只对账一次，强制 parent/power/edge/buffer/terminal blocker 稳定归因 primary root/secondary，cutover 分窗且缺失 plan/证据显示 unknown；full-only 与 profile-supported partial recipe 分别验证合法执行单位、量化/residual、输入/容量漂移、requested/committed/executed/unmet、主/副产物总量，partial capacity 不绕过 batch admissibility，等价 full-vs-split 工作不多计稳定进度/里程碑；主产物+副产物 bundle 全 branch 成功只结算一次，任一 branch owner/路线/容量/资格失败在 atomic policy 下无 output credit/进度，supported split policy 明示 settled/pending/residual 且恢复只重评未决 branch；分别在旧 hold、在制、在途与缓冲批次存在时执行因果换线，证明单一 cutover、旧/新身份分离、唯一处置、新窗口归零且无静默迁移/退款/丢失/重复效果；并覆盖终端 buffer 可用/已满、owner/需求失效、production 与 delivery receipt 分离、非 settlement 不解锁需求/奖励、稳定窗口按声明的 terminal-admission/window 策略计数。该层还必须覆盖 extraction/replenishment→refinement→source-to-ledger handoff→首个 canonical consumable ledger input：`preview/source-settled/in-transit/arrived/applicable/blocked` 的状态、原因与下一步可读且不能互相冒充，数量/损耗/owner/账本/parent provenance/适用性守恒且可追溯，失败恢复只使用专业合同支持的补充/补电/重报价/改道/隔离/返工/返还/补偿/放弃，重复 submit/arrival/重连/乱序/replay 不复制 source effect、目的 credit、首个 input sink、稳定进度或奖励。`test_tier_full` 覆盖长 measurement window/rollover、3+ stages/edges 的跨窗口链、3+ input joins 与 3+ output branches、跨阶段不同 batch unit、不同规格批次的拆分/汇合与共享边、跨账本运输损耗及副产物交付、parent/branch/window 乱序/重复/故障与 late capacity release、长时量化无累计漂移/争用无静默 starvation、staged-intake wait-cycle injection 与持久化/replay 后不产生 orphan/隐式回滚/重复效果、支持的隔离/复验/返工/替代去向与显式 handoff/conversion、重复换线、source/transit/destination 故障窗口、支持的 compensating receipt、持久化、边界 checkpoint 恢复/replay、Agent 重试，以及 Viewer、pure API 与 Agent 的适用结论、diagnostic/window/lease/join/batch/bundle/branch/分配/换线/终端结算结果、lineage、守恒和单次效果一致性 | test_tier_full |

| SC-22 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 维护/计划停机前的可比较取舍、维护真值缺失的诚实表达、提交重验、bounded pending 无隐式 hold、原任务/未结算状态保留、单次结果与稳定/交付影响，以及 Viewer/pure API/Agent 对各 work bucket 与 profile-specific `W` policy 的 parity | test_tier_required |
| SC-23 | producer_system_designer / gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-002 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_RUNTIME-043 / PRD-WORLD_SIMULATOR-001 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | BuildFactory quote 的只读/确定性、逐项成本与余额、四类 power mode/未冻结 authority、choices/blockers、quote correlation、fresh submit revalidation、single sink/receipt、profile output boundary、retry/reconnect/replay 与 Viewer/pure API parity | test_tier_required |
| SC-24 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_RUNTIME-043 / PRD-WORLD_SIMULATOR-001 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-industrial-recipe-lifecycle-decision-contract.prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | 六状态转换与预览同步、玩家动作/恢复/机会成本/进度奖励映射、无安全动作时的 `no_safe_fallback`、版本 pin、兼容性/授权/前置漂移 fail-closed、Agent 授权边界、退役对 accepted/WIP/in-transit/buffer/terminal-pending 的动作矩阵、terminal-pending identity/容量义务/单次处置与 settlement 边界、parent-linked conversion、runtime identity/幂等 finality/replay、持久化与 Viewer/pure API/Agent parity | test_tier_full |
| SC-25 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_RUNTIME-043 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-industrial-factory-capability-lifecycle-contract.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | `test_tier_required` 覆盖工厂能力预览无效果、成本/停机/容量机会成本、authority drift/unknown、parent-linked candidate 与单一 cutover、accepted-unstarted/WIP/in-transit/buffer/terminal-pending 逐项处置、retiring/retired 无新排程、successor 不自动迁移、W/queue/reservation/receipt/reward 隔离、retry/reconnect/replay 不重复或复活，以及 Viewer/pure API/Agent parity；`test_tier_full` 覆盖并发/连续升级、共享容量、部分失败、crash/restore/replay、持久化、多阶段工作与 successor conversion | test_tier_full |
| SC-26 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_RUNTIME-043 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | `test_tier_required` 覆盖 Gate-before-recommendation 的 `candidate_available`/`no_safe_starter_chain` 两结果、stable starter-chain/candidate identity 与 authority snapshot/version 绑定、`current-evidence-backed`/`target-contract`/`unknown/not_tracked` 切线、`no_safe_starter_chain` 的 blocker/恢复/复查边界及 fresh composite runtime + QA evidence 后才可作 current claim；并覆盖四类产品读面、同一三阶段路径的 blocker/占用/下一动作/复查/进度影响、production 与 delivery/terminal settlement 分离、结算后下一目标候选、因果换线 `W=0` 与 root continuation、unknown/fail-closed、Viewer/pure API/Agent/replay 一致及单次完成效果 | test_tier_required |
| SC-27 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | 配方候选发现的 `candidate_available`/`candidate_blocked`/`candidate_unknown`、同名版本隔离、目标/工厂/原料/物流/power/terminal 事实比较、只读 `recipe_selection_preview`、漂移后重发现或原子拒绝、blocked/unknown 候选的排程预览仍只读且可解释、仅可用候选经提交前 fresh revalidation 形成承诺、选择/排程/生产/稳定/交付分层，以及 Viewer/pure API/Agent/replay 不自动选择或复制效果 | test_tier_required |
| SC-28 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_RUNTIME-043 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | 一份两输入、一受限物流边、主/副产物与终端用途齐全的代表性配方执行档案；完整/unknown/blocked 判定、合法 cycle 与 power mode、join/物流/branch/terminal 失败恢复、production-only/terminal-admission 边界、parent-linked 因果换线、Viewer/pure API/Agent 对档案、blocker、下一步及完成边界的 parity、Agent 在 unknown/blocked 下不越过动作准入，以及 retry/reconnect/replay 幂等 | test_tier_full |
| SC-29 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_RUNTIME-043 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | 双站点候选比较与 `site_available/blocked/unknown`、输入/输出路径与容量/损耗/power/终端/机会成本可读、预览无副作用、提交 fresh revalidation、因果换站点 parent-linked candidate/`W=0`、无自动迁厂/改道、旧工作不静默迁移、Viewer/pure API/Agent parity、Agent recommendation/confirmation boundary 与 retry/reconnect/replay 单次效果；`test_tier_full` 覆盖拓扑争用、存量处置、恢复与 relocation/cutover profile | test_tier_full |
| SC-31 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-012 / PRD-GAME-014 / PRD-GAME-018 / PRD-P2P-003 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_RUNTIME-043 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/p2p/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | `test_tier_required` 覆盖 demand goal authority/goal_authority_ref、target/committed/produced/delivery-settled/terminal-settled/remaining 分层、batch quantum 与 shortage/matched/surplus/unknown、unknown/expired/conflict fail-closed、full/reduced/补料/调运/hold/stop/supplemental 取舍、profile matching settlement 单次减少需求、目标满足后显式停止、盈余不自动处置、matching settlement 不足时可选择一次 parent-linked 补产或保留 shortage 并停止/延期、非 matching settlement 不减需求、drift 重报价/原子拒绝及 retry/reconnect/replay 单次效果；`test_tier_full` 覆盖并发目标、跨窗口到期/取消、三阶段多批次/损耗、partial matching settlement（delivery 或 terminal）、补产 revision 链、容量争用、持久化/补偿与 Viewer/pure API/Agent parity；跨区 escrow/ownership 的 blockchain_ops_engineer 与 P2P authority 必须参与同候选验证；跨区发现/物流/escrow 的目标验收另见 [REQ-SC31-009](industrial-demand-goals-and-settlement.prd.md#req-sc31-009)、[AC-SC31-010](industrial-demand-goals-and-settlement.prd.md#ac-sc31-010)、[AC-SC31-011](industrial-demand-goals-and-settlement.prd.md#ac-sc31-011)；叶子要求与场景映射见 [`工业需求目标与生产交付结算`](industrial-demand-goals-and-settlement.prd.md#sc31-leaf-requirements) 的 [REQ-SC31-001](industrial-demand-goals-and-settlement.prd.md#req-sc31-001)..009 与 [AC-SC31-001](industrial-demand-goals-and-settlement.prd.md#ac-sc31-001)..011 | test_tier_full |
| SC-30 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_RUNTIME-043 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | `test_tier_required` 覆盖默认禁止异料替代、同料换来源/合法异料替代/换配方的候选隔离与机会成本、preview 无效果、decision-only 无 sink/credit/conversion/`W`/奖励、实际 conversion 的 linked exactly-once settlement/provenance receipt、ratio/rounding/residual 守恒、unknown/blocked 与 `degraded` 分离、same-recipe 全因果不变条件、causal change parent-linked candidate/`W=0`、stale/owner/quality/route 负例及 retry/reconnect/replay 幂等；`test_tier_full` 覆盖多阶段 join、并发替代批次争用、品质/保管漂移、in-transit/buffer、跨窗口 cutover、持久化恢复、Viewer/pure API/Agent parity 与反套利 | test_tier_full |
| SC-32 | producer_system_designer / gameplay_designer / runtime_engineer / wasm_platform_engineer / agent_engineer / blockchain_ops_engineer / qa_engineer | PRD-GAME-002 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-002 / PRD-WORLD_SIMULATOR-001 / PRD-P2P-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | canonical native capability 四类互斥 mapping 与唯一 pilot transition、冻结新增 native action/state、Alliance 或 EconomicContract 开放扩展，以及 CapabilityGrantV2 owner-bound budget 的玩家可见 owner/payer、quote/affordability、unknown/stale/deny no-debit、accepted effect + debit + receipt exactly-once、retry/replay no-credit/no-double-charge、旧玩家结果/快照/replay、版本/ABI、receipt 单次结算和失败恢复兼容性证据；不把 ECS、World Database 或分片当作本阶段完成条件 | test_tier_full |
| SC-33 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/game/gameplay/gameplay-industrial-externality-contract.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | `industrial_externality` / `operational_mitigation` profile 的 no-profile legacy unchanged、known/unknown/expired authority fail-closed、只读 preview、submit fresh revalidation、mitigation capacity 与 production/terminal 影响、externality facet 与 quality/custody/byproduct/maintenance 分离、root/revision/linked disposition exactly-once、WIP/transit/buffer/terminal-pending 单次处置、terminal-pending identity/容量义务与 settlement 边界、区域/危机治理隔离、Viewer/pure API/Agent parity，以及不冻结公式/扩散/清理经济/runtime schema 和 current claim 的 deterministic target evidence | test_tier_required |
| SC-34 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer / liveops_community | PRD-GAME-002 / PRD-GAME-015 / PRD-GAME-018 / PRD-WORLD_RUNTIME-001 / PRD-P2P-001 / PRD-P2P-003 / PRD-WORLD_SIMULATOR-042 / PRD-WORLD_SIMULATOR-045 / PRD-TESTING-003 | `doc/product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md`; `doc/product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md`; `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md`; `doc/product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md`; `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md`; `doc/product/player-entry-distribution/release-communications-and-public-claims.prd.md`; `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | CR-3/4/5 的产品边界分别由 GCB-001/002/003/006、ES-003/004、MW-10/MW-12、PE-002/003、AR-4、[REQ-ENTRY-CANDIDATE-001](../player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001)/[AC-ENTRY-CANDIDATE-001](../player-entry-distribution/access-modes-and-release-readiness.prd.md#ac-entry-candidate-001) 与 ENTRY-CLAIM-001/003 追踪；MW-12 补充非 levy 资源 source/sink、关键经济变化、治理影响与异常不被局部 green 掩盖的产品边界；同候选的 runtime/P2P/恢复/经济审计/QA/公开 claim 证据组合门禁仍由专业 authority 与根 README 维护，产品层不宣称当前放行 | test_tier_full |

## 6. Non-Goals

- 不在产品层冻结新的玩法细则、数值、掉落或成长曲线。
- 不把分布式执行、任意 WASM 或全局治理包装成当前玩家默认能力。
- 不复制 `game` 专题 PRD、project 任务或测试步骤。
- 不提供自由配方编辑器、客户端上传即生效、自动配方替换或跨版本免费材料转换。
- 不冻结配方比例、产率/损耗、授权/版税公式、兼容算法、审批角色、runtime schema、队列或 UI；本节也不宣称配方治理、退役迁移或当前工业闭环已经实现。
- 不冻结工厂能力状态的 runtime 枚举、升级/降级/退役成本、停机/吞吐/维护公式、自动迁移或补偿算法；工厂 candidate、cutover、receipt、幂等与恢复实现仍由 `world-runtime`、M4 工业合同及其他专业 authority 拥有。
- 不定义站点产权/租约/区域 charter 治理、选址或寻路算法、地形/车队模拟、容量/损耗/电力/建设数值、自动迁厂/改道或 relocation 实现；这些分别由区域治理、`world-runtime`/M4、`game` 与相关专业合同拥有。
- 不进行一次性 ECS/World Database 重构，不把所有 native 制度或 `WorldState`/`Action` 在单个版本内推倒重写；开放扩展必须沿分类、试点和兼容门槛渐进迁移。
- 不在本阶段立即引入 Region/Partition/Shard 或多条世界时间线；单一 canonical world timeline 仍是产品承诺。
- 不因模块化或 WASM 扩展而弱化 L0 物理不变量、L1 治理/反滥用边界、所有权权限校验、receipt 单次结算、snapshot/replay 连续性或历史审计。
- 不把现有 native `Alliance`、`War`、`Governance`、`EconomicContract` 兼容基线误删、重置或包装成无限扩展先例；迁移未完成前，旧玩家结果和可玩路径必须保持可读。

## 7. 根成功标准的语义归属与状态

本节只为既有 SC-1 至 SC-34 补充追踪，不增删产品承诺。§1–5 的原文、适用条件及 §5.1 的专业 owner、证据和测试层级继续有效。组合要求复用既有叶子的唯一 Trace；同义的预览/漂移/幂等/恢复/多入口条件不因出现在多个 SC 而另发身份。只有未被现有叶子拥有的独立范围在 §7.2 声明一次 Trace；其验收仍是原 SC 中对应条件，不另造一套 AC。

SC-25 的 capability/cutover/五类旧工作补充条款在排版上紧随 SC-26，语义仍归属工厂能力生命周期；不能当作首局叶子已覆盖它们。映射不等于设计、实现或组合验收完成。状态只对所列范围成立，不能用一个局部 green 代签其他义务或整个 SC。

### 7.1 全部 SC 的主责与组合关系

| 原成功标准 | 复用的既有要求 | 根独立范围与组合验收 |
| --- | --- | --- |
| [SC-1](#sc-1) | [REQ-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#req-first-industrial-001)（Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502）；[REQ-FIRST-INDUSTRIAL-007](first-session-and-continuation.prd.md#req-first-industrial-007)（Trace refs: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812）；[REQ-WR-PE-001](playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)（Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 首局/后引导/跨阶段概述及专业总纲继续适用，工业样本不外推所有场景。 |
| [SC-2](#sc-2) | [REQ-FIRST-INDUSTRIAL-004](first-session-and-continuation.prd.md#req-first-industrial-004)（Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2）；[REQ-WR-IA-001](indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)（Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 首局/后引导/跨阶段概述及专业总纲继续适用，工业样本不外推所有场景。 |
| [SC-3](#sc-3) | [REQ-FIRST-INDUSTRIAL-008](first-session-and-continuation.prd.md#req-first-industrial-008)（Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3）；[REQ-WR-MW-001](mature-world-progression.prd.md#req-wr-mw-001)（Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 首局/后引导/跨阶段概述及专业总纲继续适用，工业样本不外推所有场景。 |
| [SC-4](#sc-4) | 没有将相邻叶子身份扩大为本条全部义务。 | 追踪验收判据：要求到专业 PRD-ID 与真实证据可回链，由 §5.1 owner 负责；不是新增玩法功能。 |
| [SC-5](#sc-5) | [REQ-WR-IA-001](indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)（Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185）；[REQ-WR-IA-003](indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)（Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376） | [canonical 替代比较](#root-sc5-altcompare)；[替代的明确授权与无效果预览](#root-sc5-altauth)；[替代失效与单次结果](#root-sc5-altrepeat)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-6](#sc-6) | [REQ-WR-IA-001](indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)（Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185）；[REQ-WR-IA-002](indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)（Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63）；[REQ-WR-IA-003](indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)（Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376）；[REQ-WR-PE-002](playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002)（Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 |
| [SC-7](#sc-7) | 没有将相邻叶子身份扩大为本条全部义务。 | [持续的单一权威时间线](#root-sc7-time)；[跨粒度物理真值一致](#root-sc7-scale)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-8](#sc-8) | 没有将相邻叶子身份扩大为本条全部义务。 | [Data 授权的可读预览](#root-sc8-dataread)；[Data 中途失效与合法结算](#root-sc8-datacommit)；[Data 重试与历史 provenance](#root-sc8-datareplay)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-9](#sc-9) | [REQ-WR-MW-001](mature-world-progression.prd.md#req-wr-mw-001)（Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce）；[REQ-WR-MW-002](mature-world-progression.prd.md#req-wr-mw-002)（Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182）；[REQ-WR-MW-004](mature-world-progression.prd.md#req-wr-mw-004)（Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 |
| [SC-10](#sc-10) | [REQ-WR-MW-001](mature-world-progression.prd.md#req-wr-mw-001)（Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce）；[REQ-WR-PE-001](playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)（Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3）；[REQ-FIRST-INDUSTRIAL-008](first-session-and-continuation.prd.md#req-first-industrial-008)（Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 |
| [SC-11](#sc-11) | [REQ-FIRST-BRANCH-001](first-session-and-continuation.prd.md#req-first-branch-001)（Trace refs: O7-REQ-4c622b94-22bd-4559-9595-6b134bdc00b1）；[REQ-FIRST-BRANCH-002](first-session-and-continuation.prd.md#req-first-branch-002)（Trace refs: O7-REQ-2e82fef6-bbb5-43b6-a7d0-833905e8e8a3）；[REQ-WR-IA-002](indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)（Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 首局/后引导/跨阶段概述及专业总纲继续适用，工业样本不外推所有场景。 |
| [SC-12](#sc-12) | [REQ-WR-IA-002](indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)（Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63） | [普通成长不要求持续在线](#root-sc12-session)；[高风险响应与授权恢复可读](#root-sc12-window)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-13](#sc-13) | [REQ-WR-CC-001](chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)（Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3）；[REQ-WR-CC-002](chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)（Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c）；[REQ-WR-CC-003](chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003)（Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168）；[REQ-WR-CC-004](chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)（Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d）；[REQ-WR-CC-005](chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-005)（Trace refs: O7-REQ-ce9fbace-7fc6-4973-8bcf-8b58e11b3339） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 |
| [SC-14](#sc-14) | [REQ-WR-CR-001](communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001)（Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f）；[REQ-WR-CR-002](communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)（Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d）；[REQ-WR-CR-003](communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)（Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 |
| [SC-15](#sc-15) | [REQ-WR-OC-001](organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001)（Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a）；[REQ-WR-OC-002](organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002)（Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3）；[REQ-WR-OC-003](organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003)（Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 |
| [SC-16](#sc-16) | [REQ-WR-IA-001](indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)（Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185）；[REQ-FIRST-INDUSTRIAL-005](first-session-and-continuation.prd.md#req-first-industrial-005)（Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48）；[REQ-WR-AOS-001](agent-ownership-and-stewardship.prd.md#req-wr-aos-001)（Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf）；[REQ-WR-AOS-002](agent-ownership-and-stewardship.prd.md#req-wr-aos-002)（Trace refs: O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287）；[REQ-WR-CR-001](communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001)（Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f）；[REQ-SC31-006](industrial-demand-goals-and-settlement.prd.md#req-sc31-006)（Trace refs: O7-REQ-544d08be-9d56-4106-bd80-89c46786bfdf） | [通用 pending 不取得资源或生产顺位](#root-sc16-pending)；[报价余额与运行保障分离](#root-sc16-runway)。领域承诺、合同和需求漂移特例继续复用左列；通用接受/结算表达、重验和幂等不另发身份。 |
| [SC-17](#sc-17) | [REQ-WR-GCB-001](governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)（Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b）；[REQ-WR-GCB-002](governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-002)（Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06）；[REQ-WR-GCB-003](governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-003)（Trace refs: O7-REQ-4f6597ff-8e34-4d12-bb60-060df0e09595）；[REQ-WR-GCB-004](governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)（Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd）；[REQ-WR-GCB-005](governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)（Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5）；[REQ-WR-GCB-006](governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)（Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 |
| [SC-18](#sc-18) | [REQ-WR-GR-001](governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001)（Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8）；[REQ-WR-GR-002](governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)（Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911）；[REQ-WR-GR-003](governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)（Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 |
| [SC-19](#sc-19) | [REQ-WR-CC-003](chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003)（Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168）；[REQ-WR-GCB-006](governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)（Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8） | 复用 CC-003 的 [AC-6](chartered-conflict-soft-seasons-and-recovery.prd.md#ac-6)（containment、禁止 reset/历史重写/选择性 bailout）与 [AC-7](chartered-conflict-soft-seasons-and-recovery.prd.md#ac-7)（恢复结果与下一步），并保留 GCB-006 的高影响动作边界；不另发危机身份。 |
| [SC-20](#sc-20) | [REQ-FIRST-INDUSTRIAL-005](first-session-and-continuation.prd.md#req-first-industrial-005)（Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48） | [工业任务的投入与结果处置](#root-sc20-jobdispose)；[工业任务续接与因果换向](#root-sc20-jobcontinue)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-21](#sc-21) | [REQ-FIRST-INDUSTRIAL-003](first-session-and-continuation.prd.md#req-first-industrial-003)（Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51）；[REQ-SC31-002](industrial-demand-goals-and-settlement.prd.md#req-sc31-002)（Trace refs: O7-REQ-3c8dcb96-cb2b-4ec8-824a-5cb50b75f060）；[REQ-SC31-003](industrial-demand-goals-and-settlement.prd.md#req-sc31-003)（Trace refs: O7-REQ-ebb96559-33a9-4029-af82-a709c71225e0）；[REQ-SC31-004](industrial-demand-goals-and-settlement.prd.md#req-sc31-004)（Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3） | [批次适用与已到达输入](#root-sc21-batch)；[多输入 join 与 partial kit](#root-sc21-join)；[共享容量公平分配与重评](#root-sc21-capacity)；[端到端 service window](#root-sc21-service)；[冻结计划与实际诊断归因](#root-sc21-diagnostic)；[合法执行单位与 partial recipe](#root-sc21-quantum)；[同 cycle 多输出原子结算](#root-sc21-fanout)；[有限缓冲与背压](#root-sc21-backpressure)；[工业 cutover 的旧工作处置](#root-sc21-cutover)；[有限终端账本与失效处置](#root-sc21-terminal)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 source/refinement→handoff 的既有准备义务复用 FIRST-INDUSTRIAL-003；跨阶段组合仍须全部满足。 |
| [SC-22](#sc-22) | 没有将相邻叶子身份扩大为本条全部义务。 | [维护与停机的受支持选择](#root-sc22-maintchoice)；[维护 unknown 的提交边界](#root-sc22-maintunknown)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-23](#sc-23) | [REQ-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#req-first-industrial-002)（Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95）；[REQ-FIRST-INDUSTRIAL-005](first-session-and-continuation.prd.md#req-first-industrial-005)（Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48） | [建设报价身份与承诺包](#root-sc23-buildquote)；[四类建设 power mode 边界](#root-sc23-buildpower)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-24](#sc-24) | [REQ-FIRST-INDUSTRIAL-005](first-session-and-continuation.prd.md#req-first-industrial-005)（Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48） | [六状态配方准入](#root-sc24-recipestate)；[任务不可变配方版本与绑定](#root-sc24-recipeversion)；[配方生命周期六类工作处置](#root-sc24-recipedispose)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-25](#sc-25) | 没有将相邻叶子身份扩大为本条全部义务。 | [已建成工厂能力选择](#root-sc25-factorychoice)；[工业任务续接与因果换向](#root-sc20-jobcontinue) + [工业 cutover 的旧工作处置](#root-sc21-cutover)（组合复用，未新发工厂 cutover 身份）；[五类存量工作合法处置](#root-sc25-factorydispose)；[工厂状态与单次变更](#root-sc25-factorystate)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-26](#sc-26) | [REQ-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#req-first-industrial-001)（Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502）；[REQ-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#req-first-industrial-002)（Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95）；[REQ-FIRST-INDUSTRIAL-003](first-session-and-continuation.prd.md#req-first-industrial-003)（Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51）；[REQ-FIRST-INDUSTRIAL-004](first-session-and-continuation.prd.md#req-first-industrial-004)（Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2）；[REQ-FIRST-INDUSTRIAL-005](first-session-and-continuation.prd.md#req-first-industrial-005)（Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48）；[REQ-FIRST-INDUSTRIAL-006](first-session-and-continuation.prd.md#req-first-industrial-006)（Trace refs: O7-REQ-d0041aa9-1dfb-46a0-95e0-67417b39bfa3）；[REQ-FIRST-INDUSTRIAL-007](first-session-and-continuation.prd.md#req-first-industrial-007)（Trace refs: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812）；[REQ-FIRST-INDUSTRIAL-008](first-session-and-continuation.prd.md#req-first-industrial-008)（Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 |
| [SC-27](#sc-27) | [REQ-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#req-first-industrial-001)（Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502）；[REQ-FIRST-INDUSTRIAL-005](first-session-and-continuation.prd.md#req-first-industrial-005)（Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48） | [同快照配方候选发现](#root-sc27-discover)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-28](#sc-28) | [REQ-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#req-first-industrial-002)（Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95）；[REQ-FIRST-INDUSTRIAL-004](first-session-and-continuation.prd.md#req-first-industrial-004)（Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2）；[REQ-FIRST-INDUSTRIAL-005](first-session-and-continuation.prd.md#req-first-industrial-005)（Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48）；[REQ-WR-PE-002](playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002)（Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1）；[REQ-WR-PE-003](playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003)（Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07） | [完整代表性执行档案](#root-sc28-profile)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 原 deterministic fixture 及正负情境为组合验收，未新发“测试样本功能”身份。 |
| [SC-29](#sc-29) | 没有将相邻叶子身份扩大为本条全部义务。 | [同目标双站点比较](#root-sc29-sitecompare)；[advisory topology 与建造前置分离](#root-sc29-siteadvisory)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 因果换站点及五类旧工作还须满足本根的工业续接、能力cutover/存量保护；不因重连而绕过。 |
| [SC-30](#sc-30) | 没有将相邻叶子身份扩大为本条全部义务。 | [合法替代与 no_legal_substitute](#root-sc30-subchoice)；[替代决定与实际 conversion 分离](#root-sc30-subsettle)；[替代因果与防套利](#root-sc30-subcause)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-31](#sc-31) | [REQ-SC31-001](industrial-demand-goals-and-settlement.prd.md#req-sc31-001)（Trace refs: O7-REQ-bfd4b2f4-6b21-4c4a-961d-387b220e762f）；[REQ-SC31-002](industrial-demand-goals-and-settlement.prd.md#req-sc31-002)（Trace refs: O7-REQ-3c8dcb96-cb2b-4ec8-824a-5cb50b75f060）；[REQ-SC31-003](industrial-demand-goals-and-settlement.prd.md#req-sc31-003)（Trace refs: O7-REQ-ebb96559-33a9-4029-af82-a709c71225e0）；[REQ-SC31-004](industrial-demand-goals-and-settlement.prd.md#req-sc31-004)（Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3）；[REQ-SC31-005](industrial-demand-goals-and-settlement.prd.md#req-sc31-005)（Trace refs: O7-REQ-62129528-2238-4733-8b8e-d226dc40943d）；[REQ-SC31-006](industrial-demand-goals-and-settlement.prd.md#req-sc31-006)（Trace refs: O7-REQ-544d08be-9d56-4106-bd80-89c46786bfdf）；[REQ-SC31-007](industrial-demand-goals-and-settlement.prd.md#req-sc31-007)（Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd）；[REQ-SC31-008](industrial-demand-goals-and-settlement.prd.md#req-sc31-008)（Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3）；[REQ-SC31-009](industrial-demand-goals-and-settlement.prd.md#req-sc31-009)（Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 |
| [SC-32](#sc-32) | 没有将相邻叶子身份扩大为本条全部义务。 | [native 制度互斥分类与冻结](#root-sc32-institution)；[唯一治理批准的制度试点](#root-sc32-pilot)；[owner-bound budget 玩家读面](#root-sc32-budgetread)；[预算与世界效果原子单次提交](#root-sc32-budgetcommit)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-33](#sc-33) | [REQ-WR-PE-002](playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002)（Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1）；[REQ-WR-PE-003](playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003)（Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07） | [可选外部性 profile 与 legacy 边界](#root-sc33-extscope)；[外部性处置与其他事实隔离](#root-sc33-extdispose)。其余通用重复、恢复和入口条件复用左列/原 SC，不重复分配身份。 |
| [SC-34](#sc-34) | [REQ-WR-GCB-001](governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)（Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b）；[REQ-WR-GCB-002](governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-002)（Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06）；[REQ-WR-GCB-003](governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-003)（Trace refs: O7-REQ-4f6597ff-8e34-4d12-bb60-060df0e09595）；[REQ-WR-GCB-006](governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)（Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8）；[REQ-WR-ES-003](market-normal-state-and-emergency-supply.prd.md#req-wr-es-003)（Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1）；[REQ-WR-ES-004](market-normal-state-and-emergency-supply.prd.md#req-wr-es-004)（Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3）；[REQ-WR-MW-005](mature-world-progression.prd.md#req-wr-mw-005)（Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5）；[REQ-WR-PE-002](playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002)（Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1）；[REQ-WR-PE-003](playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003)（Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07）；[REQ-ENTRY-CANDIDATE-001](../player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001)（Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b）；[REQ-ENTRY-CLAIM-001](../player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001)（Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff）；[REQ-ENTRY-CLAIM-003](../player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003)（Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385） | 组合验收：按原 SC 的完整适用条件核对所列主责义务，不新造概括性身份；当前各叶子状态分别成立，不加总为完成。 原 AR-4、MW-10/MW-12 为组合来源；公开claim服从 entry 主责和同候选专业证据。 |

### 7.2 既存根独立范围的状态卡

以下状态仅表示实际核定深度。`未核实` 不等于 `未设计` 或 `未实现`，更不是技术豁免。相关专业 PRD、架构导航或目标合同的存在不单独证明设计已覆盖；专业 owner 后续补具体方案或核定既有精确双向承接时再更新对应状态。真实执行和组合验收另有固定候选/环境证据。

<a id="root-sc5-altcompare"></a>
#### 功能状态卡：canonical 替代比较

Trace ID: O7-REQ-d76b49a6-01a7-46db-968b-23bfd35fc295

- 主责范围：仅 [SC-5](#sc-5) 的“未支持细粒度请求的单个或多个安全 canonical 候选必须可比较目标/范围、成本、风险、后果、可撤回性与推荐理由；没有安全替代时停止”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [玩法总纲设计](../../game/gameplay/gameplay-top-level-design.design.md#2-设计结构)、[间接控制设计](../../game/gameplay/gameplay-indirect-control-agency-contract.design.md#agency-interruption-hooks)。总纲是微/中/长循环与接线导航，间接控制设计提供意图/因果/恢复保证；这些存在性与局部叶子覆盖尚不能证明本独立范围具有精确分配、所有外部依赖与验证闭环。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：在已核正式canonical/legacy chat与第一方native SDK范围，producer直接goal或已授权工具选择；不存在本适用对象的候选生成/比较consumer。原子工具拒绝及abort保留，不能替代候选为空的专业停止。 证据：[sc5](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/doc/product/world-rules-core-gameplay/prd.md#L645-L646)；[chain-entry](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/viewer/runtime_live/control_plane/canonical_agent_chat.rs#L8-L131)；[legacy-entry](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/viewer/runtime_live/control_plane/agent_chat.rs#L37-L321)；[sdk-tools](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_agent_harness/src/native_task_tools.rs#L41-L155)；[sdk-policy](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_agent_harness/src/native_task_tools.rs#L208-L241)；[private-route](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/world_service_private_goal.rs#L389-L431)；[goal-ui](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_viewer/software_safe_src/viewer_canonical_goal_module.js#L1-L75)。
- 核对日期与基线：2026-10-17；固定 main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，使用源逐Git blob校验。相对e27f4d9业务源码无变化，复用旧生产闭包而非旧结论；#4865未合代码不计。完整SC-5/SC-32与§5.1适用条件已读。仅静态审查，未运行模型、Runtime、业务测试或部署；测试源码不标PASS。
- 剩余范围：未支持细粒度原请求→安全单/多canonical候选及全部比较字段；无安全替代停止。当前正式canonical/legacy chat与第一方native SDK；不包含未经提供的外部自定义adapter/artifact，也不是所有未来模块的全仓否定。 SC-5 required与full的跨入口并发、权限收缩/替换、原请求到唯一receipt关联尚无专业实现闭包。 实施待安排： [Refs #4867](https://github.com/nil-studio/oasis7/issues/4867)；本状态审查不关闭该任务。

<a id="root-sc5-altauth"></a>
#### 功能状态卡：替代的明确授权与无效果预览

Trace ID: O7-REQ-7326a105-57ef-4cf4-823c-2755c539671b

- 主责范围：仅 [SC-5](#sc-5) 的“确认前无世界效果；新增承诺或不可逆后果取得对应范围明确授权，不能隐式继承越界权限”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [玩法总纲设计](../../game/gameplay/gameplay-top-level-design.design.md#2-设计结构)、[间接控制设计](../../game/gameplay/gameplay-indirect-control-agency-contract.design.md#agency-interruption-hooks)。总纲是微/中/长循环与接线导航，间接控制设计提供意图/因果/恢复保证；这些存在性与局部叶子覆盖尚不能证明本独立范围具有精确分配、所有外部依赖与验证闭环。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：当前签名chat直接接受goal；SDK通用quote只读，工具执行按既有授权；未形成安全替代预览→玩家确认→新增范围授权命令。provider advisory 的 proposed producer 仅 cfg(test)；普通chat生产虽连续记录 proposed/submitted/accepted，但同一次record调用不构成独立玩家确认。 证据：[owner-admission](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/world_service/agent_chat.rs#L15-L140)；[chat-lifecycle](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/agent_intent.rs#L382-L639)；[intent-writer](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/agent_intent_publication.rs#L10-L48)；[advisory](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/agent_intent_terminal.rs#L80-L166)；[sdk-tools](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_agent_harness/src/native_task_tools.rs#L41-L155)；[chain-entry](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/viewer/runtime_live/control_plane/canonical_agent_chat.rs#L8-L131)；[private-route](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/world_service_private_goal.rs#L389-L431)。
- 核对日期与基线：2026-10-17；固定 main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，使用源逐Git blob校验。相对e27f4d9业务源码无变化，复用旧生产闭包而非旧结论；#4865未合代码不计。完整SC-5/SC-32与§5.1适用条件已读。仅静态审查，未运行模型、Runtime、业务测试或部署；测试源码不标PASS。
- 剩余范围：确认前无世界效果；新承诺/不可逆后果对应范围明确授权。当前正式canonical/legacy chat与第一方native SDK；不包含未经提供的外部自定义adapter/artifact，也不是所有未来模块的全仓否定。 SC-5 required与full的跨入口并发、权限收缩/替换、原请求到唯一receipt关联尚无专业实现闭包。 实施待安排： [Refs #4867](https://github.com/nil-studio/oasis7/issues/4867)；本状态审查不关闭该任务。

<a id="root-sc5-altrepeat"></a>
#### 功能状态卡：替代失效与单次结果

Trace ID: O7-REQ-ea9640b0-fb39-4fde-9ec1-69e25c448ba5

- 主责范围：仅 [SC-5](#sc-5) 的“拒绝、过期、权限变化、并发确认、重连或重试不静默执行或重复结果；原请求、替代和结果保持可审计关联”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [玩法总纲设计](../../game/gameplay/gameplay-top-level-design.design.md#2-设计结构)、[间接控制设计](../../game/gameplay/gameplay-indirect-control-agency-contract.design.md#agency-interruption-hooks)。总纲是微/中/长循环与接线导航，间接控制设计提供意图/因果/恢复保证；这些存在性与局部叶子覆盖尚不能证明本独立范围具有精确分配、所有外部依赖与验证闭环。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：普通chat nonce/replacement/digest和原operation恢复、receipt完成已存在；专业替代身份及确认消费路径不成立，不能据通用重放给本卡partial。 证据：[owner-admission](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/world_service/agent_chat.rs#L15-L140)；[owner-read](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/world_service/agent_chat.rs#L142-L226)；[intent-producer](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/agent_intent.rs#L102-L190)；[chat-lifecycle](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/agent_intent.rs#L382-L639)；[intent-writer](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/agent_intent_publication.rs#L10-L48)；[terminal](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/agent_intent_terminal.rs#L168-L358)；[sdk-recover](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_agent_harness/src/native_task_sdk.rs#L99-L138)；[goal-ui](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_viewer/software_safe_src/viewer_canonical_goal_module.js#L1-L75)。
- 核对日期与基线：2026-10-17；固定 main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，使用源逐Git blob校验。相对e27f4d9业务源码无变化，复用旧生产闭包而非旧结论；#4865未合代码不计。完整SC-5/SC-32与§5.1适用条件已读。仅静态审查，未运行模型、Runtime、业务测试或部署；测试源码不标PASS。
- 剩余范围：拒绝/过期/权限变化/并发/重连/retry单次且原请求→替代→结果审计关联。当前正式canonical/legacy chat与第一方native SDK；不包含未经提供的外部自定义adapter/artifact，也不是所有未来模块的全仓否定。 SC-5 required与full的跨入口并发、权限收缩/替换、原请求到唯一receipt关联尚无专业实现闭包。 实施待安排： [Refs #4867](https://github.com/nil-studio/oasis7/issues/4867)；本状态审查不关闭该任务。

<a id="root-sc7-time"></a>
#### 功能状态卡：持续的单一权威时间线

Trace ID: O7-REQ-83d8d112-36ea-4481-9fcc-47ee0c0dcbd4

- 主责范围：仅 [SC-7](#sc-7) 的“没有直接玩家输入时仍按世界规则推进；不冻结 tick 时长或产生第二条时间线”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [Runtime 事务模型](../../world-runtime/design.md#621-explicit-transaction-and-typed-delta-model)、[处置与幂等](../../world-runtime/design.md#622-disposition-and-idempotency-contract)、[M4 预览到承诺](../../world-simulator/m4/industrial-resource-flow-contract.design.md#64-预览到承诺的关联)。现有事务与报价设计提供相关机制，但其上游分配不等于本条全跨域玩家结果的精确承接；Data 的 purpose/scope/撤销或跨粒度物理真值不能从通用事务原子性推导。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 有界源码/记录续核：formal successor绑定qualified parent snapshot/time与原committed context，旧tick不可重写。 NodeRuntimeExecutionDriver begin/finish→World formal execution→committed-context step。 formal retention开启时要求精确已资格父状态及前进height；普通step不能插入同一formal执行。 直接支持单一权威logical timeline子范围；普通step即使队列空也处理due jobs/cycles，但没有据此证明正式网络无人输入会持续产生committed tick。 证据：[formal_execution_tick.rs:15–91](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/formal_execution_tick.rs#L15-L91)；[step.rs:378–414](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/step.rs#L378-L414)；[step.rs:510–553](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/step.rs#L510-L553)；[driver_formal_tick.rs:1–36](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/driver_formal_tick.rs#L1-L36)；[driver.rs:615–632](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs#L615-L632)；[driver.rs:747–756](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs#L747-L756)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：无人玩家输入的正式调度持续性、tick时长/暂停语义与实际恢复样本仍未验证；freshDevLocal/bootstrap不代签formal。

<a id="root-sc7-scale"></a>
#### 功能状态卡：跨粒度物理真值一致

Trace ID: O7-REQ-5514d066-856a-49e9-9c34-ad0ea3408c99

- 主责范围：仅 [SC-7](#sc-7) 的“同一行动在 gameplay/runtime/Agent/Viewer 的粗粒度与表现映射保持距离、顺序、成本、持久化结果一致”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未核实
- 系统设计依据：已核对 [Runtime 事务模型](../../world-runtime/design.md#621-explicit-transaction-and-typed-delta-model)、[处置与幂等](../../world-runtime/design.md#622-disposition-and-idempotency-contract)、[M4 预览到承诺](../../world-simulator/m4/industrial-resource-flow-contract.design.md#64-预览到承诺的关联)。现有事务与报价设计提供相关机制，但其上游分配不等于本条全跨域玩家结果的精确承接；Data 的 purpose/scope/撤销或跨粒度物理真值不能从通用事务原子性推导。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 有界源码/记录续核：Viewer mapping从Runtime cell复制pos/body/resources，清空旧seed agents与module visuals。 runtime_state_to_simulator_model→Viewer model；可见两层投影，尚不是同一行动四层成本/距离对账。 Runtime state为投影来源；seed fallback不得被当formal真值。 位置与资源映射实现可定位，但行动距离、先后、成本、持久化跨Gameplay/Agent/Viewer一致性未逐一对应。 证据：[mapping.rs:38–95](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/mapping.rs#L38-L95)；[replay.rs:285–310](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/replay.rs#L285-L310)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：仍需同一真实行动跨四层producer/consumer与重放结果；不以复制字段或物理fixture断言完整跨粒度一致。

<a id="root-sc8-dataread"></a>
#### 功能状态卡：Data 授权的可读预览

Trace ID: O7-REQ-6bd89114-455c-490b-9b21-425fa90df866

- 主责范围：仅 [SC-8](#sc-8) 的“预览不授予访问或收益；owner、recipient/使用主体、purpose、scope、授权状态可读”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [Runtime 事务模型](../../world-runtime/design.md#621-explicit-transaction-and-typed-delta-model)、[处置与幂等](../../world-runtime/design.md#622-disposition-and-idempotency-contract)、[M4 预览到承诺](../../world-simulator/m4/industrial-resource-flow-contract.design.md#64-预览到承诺的关联)。现有事务与报价设计提供相关机制，但其上游分配不等于本条全跨域玩家结果的精确承接；Data 的 purpose/scope/撤销或跨粒度物理真值不能从通用事务原子性推导。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 有界源码/记录续核：self_collection预览明确collector=owner=recipient、data_use及self_owned_no_grant_required，并计算电力与Data。 handle_collect_data Preflight→data_collection_preflight→CollectDataPreflight响应。 collector来自当前授权绑定与quote state；仅self-owned无需grant分支，非跨主体purpose/scope授权预览。 Preflight直接返回报价，不产生资源/授权/收益效果；不宣称所有内部状态无变化，authorize_collect_data在预览也消费auth nonce，因此仅证明无资源/授权收益效果。 证据：[player_gameplay.rs:49–101](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/player_gameplay.rs#L49-L101)；[collect_data.rs:112–140](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/collect_data.rs#L112-L140)；[collect_data.rs:274–308](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/collect_data.rs#L274-L308)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：跨主体owner/recipient/purpose/scope、当前grant状态可读及全部入口预览语义未闭合；self_collection不能代签跨主体访问授权。

<a id="root-sc8-datacommit"></a>
#### 功能状态卡：Data 中途失效与合法结算

Trace ID: O7-REQ-ebcc34f8-5db5-46f7-85fb-30b44b8b797f

- 主责范围：仅 [SC-8](#sc-8) 的“提交至结算间授权失效或不能证明有效时原子拒绝或可读待决，无 sink、收益或隐性义务；合法结算只产生一次 receipt 支持的使用/转移”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [Runtime 事务模型](../../world-runtime/design.md#621-explicit-transaction-and-typed-delta-model)、[处置与幂等](../../world-runtime/design.md#622-disposition-and-idempotency-contract)、[M4 预览到承诺](../../world-simulator/m4/industrial-resource-flow-contract.design.md#64-预览到承诺的关联)。现有事务与报价设计提供相关机制，但其上游分配不等于本条全跨域玩家结果的精确承接；Data 的 purpose/scope/撤销或跨粒度物理真值不能从通用事务原子性推导。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定候选静态复核：成功Data合同结算的实际action转换在会计转移前重新检查当前creator→counterparty permission，缺失即ActionRejected，直接支持结算时失效拒绝子范围；purpose/scope、未知授权待决、全部receipt单次使用/转移与跨域原子性尚未核清。 源码/测试定位：[data_access_control.rs:297–370](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/data_access_control.rs#L297-L370)；[data_access_control.rs:371–450](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/data_access_control.rs#L371-L450)；[action_to_event_policy_contract.rs:1080–1118](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_policy_contract.rs#L1080-L1118)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：提交至结算间授权失效或不能证明有效时原子拒绝或可读待决，无 sink、收益或隐性义务；合法结算只产生一次 receipt 支持的使用/转移的精确专业分配/验证关系、当前实现和实际验收仍由 §5.1 对应 owner 核定；不得以根 SC 的局部或历史结果代签。

<a id="root-sc8-datareplay"></a>
#### 功能状态卡：Data 重试与历史 provenance

Trace ID: O7-REQ-c549ec3e-5dc6-4e27-9285-4e326b0fc5d3

- 主责范围：仅 [SC-8](#sc-8) 的“重试、重连、重复提交、历史 receipt 重放不复活授权或复制结果；后续撤销保留既有合法 provenance，提供重新授权或替代路径”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [Runtime 事务模型](../../world-runtime/design.md#621-explicit-transaction-and-typed-delta-model)、[处置与幂等](../../world-runtime/design.md#622-disposition-and-idempotency-contract)、[M4 预览到承诺](../../world-simulator/m4/industrial-resource-flow-contract.design.md#64-预览到承诺的关联)。现有事务与报价设计提供相关机制，但其上游分配不等于本条全跨域玩家结果的精确承接；Data 的 purpose/scope/撤销或跨粒度物理真值不能从通用事务原子性推导。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 有界源码/记录续核：Authenticated Data collection把当前player/key nonce与资源变更一起准备，nonce不前进即拒绝。 DataCollectedAuthenticated→PreparedEconomyDataEvent→状态install，DataAccessGranted/Revoked另走permission set更新。 结算前重验签名、唯一当前starter player/key→collector绑定；不是历史receipt授权。 直接支持已签名Data采集重复结果拒绝子范围；撤销会移除当前grantee，但历史provenance及重授权界面尚未证实。 证据：[economy_data_publication.rs:83–155](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/economy_data_publication.rs#L83-L155)；[economy_data_publication.rs:166–206](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/economy_data_publication.rs#L166-L206)；[economy_data_publication.rs:242–260](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/economy_data_publication.rs#L242-L260)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：grant撤销后历史receipt不复活权限、合法provenance保留和可读重新授权/替代仍需专门链；不把self_collection nonce当所有Data access重放证明。

<a id="root-sc12-session"></a>
#### 功能状态卡：普通成长不要求持续在线

Trace ID: O7-REQ-036e47a9-59dd-4ef7-a99b-a942544d723a

- 主责范围：仅 [SC-12](#sc-12) 的“短命令/复盘和有界离线授权可维持普通成长；较长区域、外交、治理会话是自愿扩展，不要求先掌握全部系统”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未核实
- 系统设计依据：已核对 [玩法总纲设计](../../game/gameplay/gameplay-top-level-design.design.md#2-设计结构)、[间接控制设计](../../game/gameplay/gameplay-indirect-control-agency-contract.design.md#agency-interruption-hooks)。总纲是微/中/长循环与接线导航，间接控制设计提供意图/因果/恢复保证；这些存在性与局部叶子覆盖尚不能证明本独立范围具有精确分配、所有外部依赖与验证闭环。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 有界源码/记录续核：委托action_scope、当前grant与预算验证是执行能力边界，不是普通成长可持续证明。 agent_delegation生产host按scope选择grant；Viewer/首局续接设计提供相邻入口。 现有grant范围/期间约束不能替代玩家选择或声明离线收益。 已核到有界授权基础；没有同一正式成长profile的短会话→离线→回流实际结果链。 证据：[agent_delegation.rs:476–520](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/agent_delegation.rs#L476-L520)；[gameplay-top-level-design.design.md:62–82](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/game/gameplay/gameplay-top-level-design.design.md#L62-L82)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：普通成长可持续、无需先掌握全部系统、自愿长会话的真实消费者与窗口观察仍缺；不由有界grant推出可玩性通过。

<a id="root-sc12-window"></a>
#### 功能状态卡：高风险响应与授权恢复可读

Trace ID: O7-REQ-890efa7a-b296-4d48-8b11-7199ad4e200e

- 主责范围：仅 [SC-12](#sc-12) 的“高风险窗口可预期且有界，授权范围与恢复路径可读；不以持续在线换取基本成长、恢复或资格”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未核实
- 系统设计依据：已核对 [玩法总纲设计](../../game/gameplay/gameplay-top-level-design.design.md#2-设计结构)、[间接控制设计](../../game/gameplay/gameplay-indirect-control-agency-contract.design.md#agency-interruption-hooks)。总纲是微/中/长循环与接线导航，间接控制设计提供意图/因果/恢复保证；这些存在性与局部叶子覆盖尚不能证明本独立范围具有精确分配、所有外部依赖与验证闭环。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 有界源码/记录续核：范围受限grant与拒绝结果存在；高风险响应窗口的产品producer未定位。 agent_delegation验证→调用方拒绝；不是高风险窗口通知/恢复消费者。 当前scope校验与过期约束不可等同不强迫在线的基本资格保证。 未将恢复文案或一般授权错误当成高风险时间窗可预期和有界。 证据：[agent_delegation.rs:319–367](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/agent_delegation.rs#L319-L367)；[gameplay-top-level-design.design.md:62–82](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/game/gameplay/gameplay-top-level-design.design.md#L62-L82)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：高风险窗口/恢复路径、普通成长与资格无需持续在线须分别取得当前profile证据；政策义务已写入不等于技术已查清。

<a id="root-sc16-pending"></a>
#### 功能状态卡：通用 pending 不取得资源或生产顺位

Trace ID: O7-REQ-67149303-7f84-47ad-830f-8d655b6a64c7

- 主责范围：仅 [SC-16](#sc-16) 与[原跨资源阶段表/条件报价](#跨资源承诺、失效与重复提交边界) 的“尚未通过所属专业域当前资源/规则校验的请求送达、入队或 pending，不产生资源 hold、sink、设施、生产顺位或持续义务；不把 transport 的 accepted 命名当成领域承诺”。验收仍沿原 SC、原阶段表及报价段的成功/失败条件；不新增 AC、资源数值或运行保证。合同、需求漂移和首个认领的具体特例继续复用 §7.1 的叶子主责。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 预览到承诺](../../world-simulator/m4/industrial-resource-flow-contract.design.md#64-预览到承诺的关联)及[报价计算](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)。M4 §6.4 区分预览、pending和Runtime颁发的承诺；现有分配只声明IA001/首局局部消费，不能据此证明所有资源/资格/设施路径的入队都没有hold/sink/生产顺位。 本轮重读确认上述设计有相关阶段/制度条款，但尚无本卡全部独立义务的精确反向分配与验证关系；不由相邻叶子或泛 SC-32 文字提升本卡设计状态。
- 实现与验证依据：2026-10-17 有界源码/记录续核：World submit仅分配action id并入pending ActionEnvelope；领域preflight/事件发布在step执行。 submit_action/submit_recipe_action_with_origin→pending queue→action_to_event→preflight→accepted/event publication。 入队不新增资源权限；enforce_pending_action_limit仍可能改变队列，不称无任何状态变化。 该直接queue路径不写资源hold/sink、设施或已接受领域生产顺位；collect_data服务ack明确received并等待committed outcome。 证据：[actions.rs:9–52](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/actions.rs#L9-L52)；[step.rs:386–405](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/step.rs#L386-L405)；[collect_data.rs:103–110](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/collect_data.rs#L103-L110)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：所有上游pre-submit/资源域、队列公平与设施持续义务仍需逐入口；不能把transport accepted当领域承诺。

<a id="root-sc16-runway"></a>
#### 功能状态卡：报价余额与运行保障分离

Trace ID: O7-REQ-84a63552-d68d-48eb-80dc-23cff7531242

- 主责范围：仅 [SC-16](#sc-16) 与[原跨资源阶段表/条件报价](#跨资源承诺、失效与重复提交边界) 的“electricity_after 仅为当次报价预计扣减后余额，不等同 Agent 或设施运行 runway，也不能单独保证持续生产或不停机；两者影响当前决定时分别说明余额、runway/风险及真实恢复或暂缓路径”。验收仍沿原 SC、原阶段表及报价段的成功/失败条件；不新增 AC、资源数值或运行保证。合同、需求漂移和首个认领的具体特例继续复用 §7.1 的叶子主责。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 预览到承诺](../../world-simulator/m4/industrial-resource-flow-contract.design.md#64-预览到承诺的关联)及[报价计算](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)。M4报价设计可定位owner电力与账本计算，产品根已区分余额/runway；尚未核定所有适用quote消费者、专业runway来源及风险/恢复解释的精确设计和验证关系。
- 实现与验证依据：2026-10-17 有界源码/记录续核：排产报价电力来自Runtime builder余额−canonical plan，battery runway从Agent power/idle cost独立计算。 schedule_recipe_quote→kernel runtime signals→ScheduleRecipeQuotePreflight→ScheduleRecipeQuoteCard并列展示余额与runway。 验签/会话/当前owner及factory builder一致，报价无排产；runway不是电力余额保证。 已实现余额与runway分离的直接读面；producer low/elevated与restore_power_before_scheduling未匹配UI normal/critical/shutdown与restore_power文案，风险恢复消费存在边界。 证据：[schedule_recipe_quote.rs:13–49](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/schedule_recipe_quote.rs#L13-L49)；[schedule_recipe_quote.rs:186–214](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/schedule_recipe_quote.rs#L186-L214)；[actions_schedule_quote.rs:148–190](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/actions_schedule_quote.rs#L148-L190)；[schedule_recipe_quote_card.jsx:3–19](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_card.jsx#L3-L19)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：全部quote、formal权威来源及风险/真实恢复路径仍未闭合；静态字段/枚举差异未执行为运行失败。 风险/恢复 token 显示错配跟踪 [#4846](https://github.com/nil-studio/oasis7/issues/4846)（静态定位、未运行复现，开发另行安排）；本卡不自动关闭该问题。

<a id="root-sc20-jobdispose"></a>
#### 功能状态卡：工业任务的投入与结果处置

Trace ID: O7-REQ-eacc6e2c-a75b-4c2b-b32b-f24e15832f95

- 主责范围：仅 [SC-20](#sc-20) 的“报价无效果、提交单次承诺或原子拒绝；完成/阻塞/中止按专业规则守恒处置投入、在制、产物和副产物，不同时退款与完成”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定基线静态追链：在既有完成路径单次守恒处置之外，本轮核清模块生产的 product_validation 拒绝会发 FactoryProductionBlocked；准备层记录 consumed_lost（投入及电力已消费且损失）、清除 pending 并记 settled，重复一致处置幂等、冲突拒绝；Viewer gameplay snapshot 输出当前请求者的 consumed/lost 与 next_action。该失败处置是原任务投入与结果处置的直接子范围。 [economy.rs:650–704](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/economy.rs#L650-L704)；[recipe_lifecycle.rs:579–721](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L579-L721)；[gameplay_snapshot.rs:340–358](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L340-L358)；[recipe_lifecycle.rs:369–577](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L369-L577)；[publication.rs:198–220](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/publication.rs#L198-L220)；[publication.rs:398–446](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/publication.rs#L398-L446)。本轮未运行业务测试、正式入口或真人验收；测试定义及此前结构 CI 不能作为本卡验收通过。
- 核对日期与基线：2026-10-17；需求与源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；复查 main `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5` 仅文档增量，相关源码未变；本轮仅静态追链。
- 剩余范围：未核清其他中止/取消、WIP 退款、全 bucket 处置、parent-linked 补偿与跨入口完整验收；本轮 product_validation 的 next_recheck 为 None，不证明所有有界恢复义务。

<a id="root-sc20-jobcontinue"></a>
#### 功能状态卡：工业任务续接与因果换向

Trace ID: O7-REQ-15abc2d8-2dc7-43ed-bb20-895dc11e8452

- 主责范围：仅 [SC-20](#sc-20) 的“同因果恢复保持 root continuation，因果变更建立链接 revision/candidate 且 W=0；换向/停机/权限变化/重连/replay 不静默迁移取消或重复效果”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：当前默认/内置模块生产路径静态闭合：现有job snapshot与私有Task goal_revision各有自己的身份；工业root/child revision并未进入该schema或生产写点，因此恢复同一job不等于恢复本卡工业因果关系。 [authority-root](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-runtime/prd.md#L398-L409)；[job-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L189-L220)；[native-recipe-command](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L435-L584)；[recipe-pause](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L15-L75)；[private-goal](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/game/goal.rs#L1-L60)；[restore](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/persistence.rs#L732-L766)；[viewer-projection](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L291-L375)。本轮未运行业务测试、模型或部署；测试定义与历史CI不代签本卡。
- 核对日期与基线：2026-10-17；需求卡固定PR4844 source head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`，实现固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；相关source blob与前轮 `e90829da59bb8a064820d4e4ce3688018a351fb8`一致并逐blob复验。仅静态审读。
- 剩余范围：在本卡完整适用范围内交付并验证：同因果恢复保持工业root continuation；因果变更形成parent-linked revision/candidate且W=0；换向/停机/权限变化/重连/replay不静默迁移取消或重复效果。结论限当前仓库默认/内置模块，不推断未提供的线上自定义artifact；原SC与§5.1验收义务全部保留。 实施待安排： [Refs #4856](https://github.com/nil-studio/oasis7/issues/4856)；本状态审查不关闭该任务。

<a id="root-sc21-batch"></a>
#### 功能状态卡：批次适用与已到达输入

Trace ID: O7-REQ-93f437ea-48eb-4f07-9af2-751faac09ec7

- 主责范围：仅 [SC-21](#sc-21) 的“下游只消费已结算且到达适用账本、满足规格/品质的中间品；未知/不兼容在 sink/进度前 fail closed，不推断、降级、混料或免费 salvage”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 第四轮生产链静态复核：已核清显式绑定物流路径所对应材料 kind 的 ScheduleRecipe 输入子链：MaterialTransitCompleted 核 pending 身份、到期、实际收货与损耗后才写目的账本和 completed path；排程检查 completed route、目标 input ledger 与材料 kind，path 分配拒绝未完成/耗尽/不足并与实际材料、电力 sink 原子安装。该证据直接支持这些已绑定 kind 的已到达账本输入准入，不再只引用普通完成 receipt。 精确源码：[material_transit.rs:185–271](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L185-L271)；[action_to_event_economy.rs:581–735](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L581-L735)；[logistics_path_authority.rs:24–141](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/logistics_path_authority.rs#L24-L141)；[recipe_lifecycle.rs:203–310](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L203-L310)；[publication.rs:198–220](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/publication.rs#L198-L220)；[publication.rs:398–446](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/publication.rs#L398-L446)；[schedule_recipe_quote.rs:19–40](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_proto/src/viewer/schedule_recipe_quote.rs#L19-L40)。本轮生产切片的交付 PR 映射尚未核清；本轮未执行业务测试；本判定基于直接实现子范围，不把未运行作为保留未知的理由，也不以相邻机制代签本卡。
- 核对日期与基线：2026-10-17；源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，逐文件 blob 与 main `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5` 核对同版；文档叠加基线 `aca7acec18d23ebc1d6ac444d7fdf4630380b389`；业务测试与 GUI 实测未执行。
- 剩余范围：本子范围仅限显式绑定的 completed route/path 所对应 kind；path_ids 为空时不分配路径 authority，allocator 只遍历 paths_by_kind、不要求每个 required kind 都有路径，因此未绑定 kind 也不能据此证明已到达或具有 batch provenance。规格/quality/custody 的权威判定、未知/不适用三态及玩家读面、跨阶段隔离/复验/返工仍未闭合；不得把 kind 相等当作品质适用。 原 SC/§5.1 适用验收全部保留；运行与跨入口完整验收仍待执行。

<a id="root-sc21-join"></a>
#### 功能状态卡：多输入 join 与 partial kit

Trace ID: O7-REQ-6e7590d6-923d-407a-93df-eef373cccaea

- 主责范围：仅 [SC-21](#sc-21) 的“canonical join 绑定完整 parent-edge/receipt set；默认齐套前无 sink/WIP/进度；显式 staged intake 逐 parent 单次消费并保留 remaining obligations，缺失/换线不得隐藏 partial kit 或复用 receipt”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：当前默认/内置模块生产路径静态闭合：path allocator确实扣settled path remaining quantity，但required parent set/join identity不能由consume stack或path id推导；atomic材料扣减在普通job范围，不是本卡canonical join直接子合同。 [authority-root](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-runtime/prd.md#L398-L409)；[m4-authority](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-simulator/m4/industrial-resource-flow-contract.prd.md#L78-L110)；[plan-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L215-L230)；[recipe-sink](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L270-L350)；[recipe-complete](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L369-L577)；[restore](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/persistence.rs#L732-L766)。本轮未运行业务测试、模型或部署；测试定义与历史CI不代签本卡。
- 核对日期与基线：2026-10-17；需求卡固定PR4844 source head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`，实现固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；相关source blob与前轮 `e90829da59bb8a064820d4e4ce3688018a351fb8`一致并逐blob复验。仅静态审读。
- 剩余范围：在本卡完整适用范围内交付并验证：canonical join绑定完整稳定parent-edge/receipt set及candidate/batch；staged-intake互等/cycle fail-closed及可读单次disposition保留原SC正文义务；默认完整齐套验证前不sink/WIP/进度；仅显式staged intake逐parent单次消费并保留remaining obligations；缺失/失效/换线/重放保留partial kit，不复用receipt。结论限当前仓库默认/内置模块，不推断未提供的线上自定义artifact；原SC与§5.1验收义务全部保留。 实施待安排： [Refs #4856](https://github.com/nil-studio/oasis7/issues/4856)；本状态审查不关闭该任务。

<a id="root-sc21-capacity"></a>
#### 功能状态卡：共享容量公平分配与重评

Trace ID: O7-REQ-7f40ee7a-192f-4579-a69d-9946a6e4d5bf

- 主责范围：仅 [SC-21](#sc-21) 的“同一权威快照为争用意图给完整/部分/延期/拒绝；不超额 hold、不靠提交/重试插队，释放/过期单次重评且有效意图不被无说明无限跳过”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：当前默认/内置模块生产路径静态闭合：原SC正文L240适用前提是两个以上已接受且有效的下游意图进入同一allocation domain。当前提交逐请求按工厂pending数返回FactoryBusy；物理TransferMaterial单action按route.available/reserved+amount准入、完成释放reservation，到期job排序只决定完成处理。已核实际Action→path resolver→transit reducer→install，该链没有accepted下游contender集合及映射，故不把单action不超配扩成该前提内直接子义务；不额外加root前提。 [m4-authority](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-simulator/m4/industrial-resource-flow-contract.prd.md#L78-L110)；[native-recipe-command](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L435-L584)；[recipe-start](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L183-L220)；[recipe-producer](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/economy.rs#L373-L433)；[job-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L189-L220)；[transit-admission](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_core.rs#L553-L610)；[transit-reserve](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L44-L115)；[transit-release](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L276-L285)；[transit-install](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L382-L410)；[api-projection](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/world_service/projection.rs#L10-L140)。本轮未运行业务测试、模型或部署；测试定义与历史CI不代签本卡。
- 核对日期与基线：2026-10-17；需求卡固定PR4844 source head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`，实现固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；相关source blob与前轮 `e90829da59bb8a064820d4e4ce3688018a351fb8`一致并逐blob复验。仅静态审读。
- 剩余范围：在本卡完整适用范围内交付并验证：同一权威snapshot评完整contender集，给full/partial/defer/reject；总hold不超额且稳定身份决定公平顺序，不靠提交/重试插队；释放/过期单次重评并保留identity/lineage；有效意图不得无限无说明跳过，读面给理由/下一复查。结论限当前仓库默认/内置模块，不推断未提供的线上自定义artifact；原SC与§5.1验收义务全部保留。 实施待安排： [Refs #4856](https://github.com/nil-studio/oasis7/issues/4856)；本状态审查不关闭该任务。

<a id="root-sc21-service"></a>
#### 功能状态卡：端到端 service window

Trace ID: O7-REQ-a3562631-35fa-4581-86e3-589e530ad253

- 主责范围：仅 [SC-21](#sc-21) 的“绑定 stage/edge/buffer/terminal reservation chain、权威时界与 none/soft/hard；每边界重验，expiry/release/renewal 不重复或靠 replay 延长，production on-time 不冒充 delivery”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：当前默认/内置模块生产路径静态闭合：ready_at只是单job到期时间；事件不含window/lease/terminal/delivery identity，故既无reservation chain也不能生成边界窗口结果。 [authority-root](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-runtime/prd.md#L398-L409)；[m4-authority](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-simulator/m4/industrial-resource-flow-contract.prd.md#L78-L110)；[job-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L189-L220)；[recipe-producer](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/economy.rs#L373-L433)；[recipe-complete](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L369-L577)；[restore](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/persistence.rs#L732-L766)。本轮未运行业务测试、模型或部署；测试定义与历史CI不代签本卡。
- 核对日期与基线：2026-10-17；需求卡固定PR4844 source head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`，实现固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；相关source blob与前轮 `e90829da59bb8a064820d4e4ce3688018a351fb8`一致并逐blob复验。仅静态审读。
- 剩余范围：在本卡完整适用范围内交付并验证：独立window绑定stage/edge/buffer/terminal mandatory reservation chain；none/soft/hard与权威time/order，每个irreversible边界重验；expiry/release/renewal单次且replay不延长；production on-time不冒充delivery finality。结论限当前仓库默认/内置模块，不推断未提供的线上自定义artifact；原SC与§5.1验收义务全部保留。 实施待安排： [Refs #4856](https://github.com/nil-studio/oasis7/issues/4856)；本状态审查不关闭该任务。

<a id="root-sc21-diagnostic"></a>
#### 功能状态卡：冻结计划与实际诊断归因

Trace ID: O7-REQ-3373ec45-3ce8-4539-8bdd-f85acca2c252

- 主责范围：仅 [SC-21](#sc-21) 的“diagnostic window 绑定 candidate/config epoch 与 journal/state-root；accepted 原子冻结 baseline，replan 链接 revision，actual 不跨 revision 回填，互斥 bucket、损耗补偿和主/派生 blocker 不重计，缺证据 unknown”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：当前默认/内置模块生产路径静态闭合：job冻结plan以及last_completed_canonical_snapshot支持普通配方一致性；没有diagnostic window和工业revision归属、baseline/bucket聚合producer，Viewer当前factory blocker不是本卡diagnostic账本。 [authority-root](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-runtime/prd.md#L398-L409)；[m4-authority](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-simulator/m4/industrial-resource-flow-contract.prd.md#L78-L110)；[job-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L189-L220)；[recipe-sink](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L270-L350)；[recipe-complete](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L369-L577)；[viewer-projection](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L291-L375)；[restore](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/persistence.rs#L732-L766)。本轮未运行业务测试、模型或部署；测试定义与历史CI不代签本卡。
- 核对日期与基线：2026-10-17；需求卡固定PR4844 source head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`，实现固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；相关source blob与前轮 `e90829da59bb8a064820d4e4ce3688018a351fb8`一致并逐blob复验。仅静态审读。
- 剩余范围：在本卡完整适用范围内交付并验证：window绑定candidate/config epoch与journal/state-root；accepted原子冻结baseline，replan链接revision；actual不跨revision回填，互斥bucket；损耗补偿与主/派生blocker不重计，缺证据unknown。结论限当前仓库默认/内置模块，不推断未提供的线上自定义artifact；原SC与§5.1验收义务全部保留。 实施待安排： [Refs #4856](https://github.com/nil-studio/oasis7/issues/4856)；本状态审查不关闭该任务。

<a id="root-sc21-quantum"></a>
#### 功能状态卡：合法执行单位与 partial recipe

Trace ID: O7-REQ-baa1e6b2-1a46-47a3-b75c-a0e9cf8b154c

- 主责范围：仅 [SC-21](#sc-21) 的“recipe/factory profile 明示执行单位、full/partial、量化和 residual；requested/committed/executed/unmet 可追溯，不足合法单位不扣料、产半批或计稳定进度”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 第四轮生产链静态复核：内置 native 路径用 canonical recipe ID 和正 u32 批数构造整批配比，输入/主副产物/电力按相同整数缩放；零批在 plan 构造和实际 ScheduleRecipe/reducer 均拒绝，入料不足先拒绝而非偷偷执行半批。Viewer 请求拒绝非正整数并展示批数、成品与电力，实际 action 使用同一 builtin plan。 精确源码：[recipe_catalog.rs:7–24](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs#L7-L24)；[recipe_catalog.rs:127–136](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs#L127-L136)；[gameplay_actions.rs:225–239](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/gameplay_actions.rs#L225-L239)；[action_to_event_economy.rs:490–520](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L490-L520)；[action_to_event_economy.rs:703–735](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L703-L735)；[recipe_lifecycle.rs:217–260](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L217-L260)；[schedule_recipe_quote_request.js:1–8](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_request.js#L1-L8)；[schedule_recipe_quote.rs:196–230](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/schedule_recipe_quote.rs#L196-L230)；[schedule_recipe_quote_card.jsx:10–18](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_card.jsx#L10-L18)。本轮生产切片的交付 PR 映射尚未核清；本轮未执行业务测试；本判定基于直接实现子范围，不把未运行作为保留未知的理由，也不以相邻机制代签本卡。
- 核对日期与基线：2026-10-17；源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，逐文件 blob 与 main `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5` 核对同版；文档叠加基线 `aca7acec18d23ebc1d6ac444d7fdf4630380b389`；业务测试与 GUI 实测未执行。
- 剩余范围：只确认内置 native 整批子范围，不将其泛化为所有 module/custom profile。专业 full-only/partial 声明、requested/committed/executed/unmet 全层对账和 residual 处置及其跨入口投影仍未闭合；模块模板的整数减批不等于完整 profile 策略。 原 SC/§5.1 适用验收全部保留；运行与跨入口完整验收仍待执行。

<a id="root-sc21-fanout"></a>
#### 功能状态卡：同 cycle 多输出原子结算

Trace ID: O7-REQ-885f0bc7-d049-49f2-bff4-a372ee06489c

- 主责范围：仅 [SC-21](#sc-21) 的“主/副产物属于同 output bundle，默认全部 branch 原子预检/结算；只有显式 split fan-out 可逐 branch 单次结算并保留 failed-leg pending/residual，不丢失复制或伪造完成”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定基线静态追链：已核清普通 RecipeCompleted 完成路径：produce 与 byproducts 同属一个 pending job/完成 receipt，二者先与启动承诺一致性比较，再合并同 kind 数量检查非法值与 i64 溢出，准备全部账本更新后经 publication 一次安装；settled job 防重复结算。这直接覆盖同任务主/副产物合并预检、原子入账与单次完成子范围。这里的 output capacity helper 是数值容量检查，不能称物理 buffer/terminal 容量已实现。 [step.rs:735–749](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/step.rs#L735-L749)；[economy.rs:684–704](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/economy.rs#L684-L704)；[recipe_lifecycle.rs:369–577](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L369-L577)；[apply_domain_event_industry_helpers.rs:87–119](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/apply_domain_event_industry_helpers.rs#L87-L119)；[publication.rs:198–220](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/publication.rs#L198-L220)；[publication.rs:398–446](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/publication.rs#L398-L446)。本轮未运行业务测试、正式入口或真人验收；测试定义及此前结构 CI 不能作为本卡验收通过。
- 核对日期与基线：2026-10-17；需求与源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；复查 main `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5` 仅文档增量，相关源码未变；本轮仅静态追链。
- 剩余范围：仍未核清多目的地 branch 的权威 output-bundle 身份、物理容量/终端准入、显式 split fan-out、failed-leg pending/residual 与全部恢复边界；需相应生产调用链及固定候选负例/端到端验收。

<a id="root-sc21-backpressure"></a>
#### 功能状态卡：有限缓冲与背压

Trace ID: O7-REQ-65e2849e-3b51-4440-a5dd-78cdd7c0692a

- 主责范围：仅 [SC-21](#sc-21) 的“下游不可用或缓冲满按声明规则保持、入缓冲或原子拒绝，不静默丢弃、瞬移、无限堆积或伪造完成”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：当前默认/内置模块生产路径静态闭合：原SC正文L238限定每条中间品边：下游不可用或buffer满时拒绝新的上游承诺。现有显式route不可用/满可在物理运输launch前拒绝并保持源材料，但该单transit guard不观察recipe的下游阶段/目的buffer，也不反馈阻止新的上游生产承诺；recipe completion仍在普通账本checked_add后直接产出。数值溢出拒绝不是物理有限buffer，本卡适用范围内背压producer尚未建立。 [m4-authority](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-simulator/m4/industrial-resource-flow-contract.prd.md#L78-L110)；[plan-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L215-L230)；[recipe-producer](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/economy.rs#L373-L433)；[recipe-complete](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L369-L577)；[output-capacity](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/apply_domain_event_industry_helpers.rs#L87-L119)；[transit-admission](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_core.rs#L553-L610)；[route-plan](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/logistics.rs#L214-L242)；[transit-reserve](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L44-L115)；[logistics-write](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L151-L271)。本轮未运行业务测试、模型或部署；测试定义与历史CI不代签本卡。
- 核对日期与基线：2026-10-17；需求卡固定PR4844 source head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`，实现固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；相关source blob与前轮 `e90829da59bb8a064820d4e4ce3688018a351fb8`一致并逐blob复验。仅静态审读。
- 剩余范围：在本卡完整适用范围内交付并验证：每条中间品边声明有限buffer和确定性背压规则；下游不可用/满时保持未消费投入、入有余量buffer或原子拒绝；不丢弃/瞬移/无限堆积/自动改道/伪造下游完成。结论限当前仓库默认/内置模块，不推断未提供的线上自定义artifact；原SC与§5.1验收义务全部保留。 实施待安排： [Refs #4856](https://github.com/nil-studio/oasis7/issues/4856)；本状态审查不关闭该任务。

<a id="root-sc21-cutover"></a>
#### 功能状态卡：工业 cutover 的旧工作处置

Trace ID: O7-REQ-38521519-d028-4768-abc5-723e239192c4

- 主责范围：仅 [SC-21](#sc-21) 的“单一 cutover 为旧 hold、在制、在途和缓冲批次产生一次显式处置；不支持的迁移 fail closed，支持的 handoff/conversion 保留父 receipt、数量与损耗”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 共同因果条件：[工业任务续接与因果换向](#root-sc20-jobcontinue)（Trace refs: O7-REQ-15abc2d8-2dc7-43ed-bb20-895dc11e8452）；这里不重复声明其身份。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：当前默认/内置模块生产路径静态闭合：工厂pause及普通recipe pending/settled、transit receipt各独立工作；没有cutover boundary或old/new candidate parent linkage，不能把停机或重新ScheduleRecipe称作cutover。 [authority-root](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-runtime/prd.md#L398-L409)；[m4-authority](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-simulator/m4/industrial-resource-flow-contract.prd.md#L78-L110)；[job-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L189-L220)；[recipe-pause](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L15-L75)；[recipe-complete](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L369-L577)；[logistics-write](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L151-L271)；[restore](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/persistence.rs#L732-L766)。本轮未运行业务测试、模型或部署；测试定义与历史CI不代签本卡。
- 核对日期与基线：2026-10-17；需求卡固定PR4844 source head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`，实现固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；相关source blob与前轮 `e90829da59bb8a064820d4e4ce3688018a351fb8`一致并逐blob复验。仅静态审读。
- 剩余范围：在本卡完整适用范围内交付并验证：单一权威cutover区分旧/新候选并固定稳定顺序；旧hold/WIP/transit/buffer各显式单次处置；不支持迁移fail closed；支持handoff/conversion保留父receipt、旧新身份、数量/损耗/原因；恢复与replay不重复释放/生产/交付或重开旧候选。结论限当前仓库默认/内置模块，不推断未提供的线上自定义artifact；原SC与§5.1验收义务全部保留。 实施待安排： [Refs #4856](https://github.com/nil-studio/oasis7/issues/4856)；本状态审查不关闭该任务。

<a id="root-sc21-terminal"></a>
#### 功能状态卡：有限终端账本与失效处置

Trace ID: O7-REQ-012b7904-2468-48c6-996d-14698aa7f798

- 主责范围：仅 [SC-21](#sc-21) 的“最终产物仅进入 owner-bound 有限账本、明确交付承诺或受支持的 hold/quarantine；终端失效保持有界 blocked/待交付或原子拒绝，不产生无限库存”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：当前默认/内置模块生产路径静态闭合：factory output ledger绑定与单job完成receipt确已存在，但该ledger没有产品意义的有限capacity/disposition；i64上界不是terminal admission。不得以owner-bound一词将缺少“有限”的整个子承诺算已实现。 [authority-root](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-runtime/prd.md#L398-L409)；[m4-authority](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-simulator/m4/industrial-resource-flow-contract.prd.md#L78-L110)；[recipe-start](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L183-L220)；[recipe-complete](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L369-L577)；[output-capacity](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/apply_domain_event_industry_helpers.rs#L87-L119)；[logistics-write](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L151-L271)；[restore](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/persistence.rs#L732-L766)。本轮未运行业务测试、模型或部署；测试定义与历史CI不代签本卡。
- 核对日期与基线：2026-10-17；需求卡固定PR4844 source head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`，实现固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；相关source blob与前轮 `e90829da59bb8a064820d4e4ce3688018a351fb8`一致并逐blob复验。仅静态审读。
- 剩余范围：在本卡完整适用范围内交付并验证：最终产物仅进入owner-bound有限ledger/buffer、明确delivery或支持hold/quarantine；terminal owner/容量/资格失效时有界blocked/待交付或产出前原子拒绝；production与delivery/market/service/需求/奖励settlement分离；无无限库存与重复容量释放，恢复保留义务。结论限当前仓库默认/内置模块，不推断未提供的线上自定义artifact；原SC与§5.1验收义务全部保留。 实施待安排： [Refs #4856](https://github.com/nil-studio/oasis7/issues/4856)；本状态审查不关闭该任务。

<a id="root-sc22-maintchoice"></a>
#### 功能状态卡：维护与停机的受支持选择

Trace ID: O7-REQ-cfbe031b-99ce-419b-a3b4-bc63e066e085

- 主责范围：仅 [SC-22](#sc-22) 的“排产前比较 maintain_before_run/run_at_risk/reduce_load/defer 中实际支持者，并说明成本/占用、产出/复查、风险、可撤回性与推荐理由”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定候选静态复核：FactoryMaintained生产准备校验operator/正数parts/耐久上限，实际扣硬件并修复durability，直接覆盖受支持维护选择；维护/停机/替换/回收四类玩家比较、撤回和全部副作用边界尚未闭合。 源码/测试定位：[factory_lifecycle.rs:574–645](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L574-L645)；[factory_lifecycle.rs:662–757](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L662-L757)；[factory_lifecycle.rs:41–73](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L41-L73)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：排产前比较 maintain_before_run/run_at_risk/reduce_load/defer 中实际支持者，并说明成本/占用、产出/复查、风险、可撤回性与推荐理由的精确专业分配/验证关系、当前实现和实际验收仍由 §5.1 对应 owner 核定；不得以根 SC 的局部或历史结果代签。

<a id="root-sc22-maintunknown"></a>
#### 功能状态卡：维护 unknown 的提交边界

Trace ID: O7-REQ-2684e207-9424-41df-93a5-80bd8f541251

- 主责范围：仅 [SC-22](#sc-22) 的“maintenance_not_tracked 不伪造安全；unknown 时不可逆候选不得 selectable，原子拒绝或无隐式 hold 的有界 pending；只有明确 profile 支持的 at-risk/unknown 路径可显示”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 第四轮生产链静态复核：已核清实际 Viewer 报价链的直接展示子义务：ScheduleRecipeQuoteGameplayPanel 接到 quote card，卡片明确显示“本次预估未跟踪维护压力”，并把风险标为电力风险；server 从当前 runtime 形成 preflight，经 DTO 到该读面。该“不伪造维护已跟踪”的限定展示已实现。 精确源码：[actions_schedule_quote.rs:183–226](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/actions_schedule_quote.rs#L183-L226)；[schedule_recipe_quote.rs:196–230](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/schedule_recipe_quote.rs#L196-L230)；[schedule_recipe_quote.rs:19–40](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_proto/src/viewer/schedule_recipe_quote.rs#L19-L40)；[legacy_core.js:198–207](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/legacy_core.js#L198-L207)；[viewer_quote_protocol_facade.js:36–39](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_quote_protocol_facade.js#L36-L39)；[schedule_recipe_quote_state.js:3–8](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_state.js#L3-L8)；[viewer_app.jsx:2698–2705](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2698-L2705)；[gameplay_quote_panels.jsx:9–14](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/gameplay_quote_panels.jsx#L9-L14)；[schedule_recipe_quote_card.jsx:4–18](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_card.jsx#L4-L18)；[action_to_event_economy.rs:435–585](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L435-L585)。本轮生产切片的交付 PR 映射尚未核清；本轮未执行业务测试；本判定基于直接实现子范围，不把未运行作为保留未知的理由，也不以相邻机制代签本卡。
- 核对日期与基线：2026-10-17；源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，逐文件 blob 与 main `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5` 核对同版；文档叠加基线 `aca7acec18d23ebc1d6ac444d7fdf4630380b389`；业务测试与 GUI 实测未执行。
- 剩余范围：未核清 maintenance unknown 对不可逆候选的 selectable/提交 gate、无隐式 hold 的有界 pending/next_recheck 与 profile 明示 at-risk 路径。现有 simulator quote 用 unchanged/continue_production 等字段，实际 ScheduleRecipe 准入没有在已审链中消费 maintenance unknown authority；UI 告知不能代签提交安全或 pure API/Agent 同义。 原 SC/§5.1 适用验收全部保留；运行与跨入口完整验收仍待执行。

<a id="root-sc23-buildquote"></a>
#### 功能状态卡：建设报价身份与承诺包

Trace ID: O7-REQ-4507fa71-95c4-404a-969a-c15c08dbeccc

- 主责范围：仅 [SC-23](#sc-23) 的“报价只读绑定 owner/site/kind/id、candidate/config/world revision、quote correlation；比较真实建造/补料/移站/补电/延期，公开全部输入、电力、boundary、blocker 和目标关联”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定候选静态复核：公开construction quote在私有World clone上校验唯一grant、绑定decision、调用真实action_to_event，读取材料/电力和site authority返回conditional报价，直接支持无写入报价与承诺包子范围；多候选比较、correlation与所有入口字段闭环仍未核清。 源码/测试定位：[builtin_factory_quote.rs:10–100](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/builtin_factory_quote.rs#L10-L100)；[receipts.rs:1–90](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/economy_factory_lifecycle/receipts.rs#L1-L90)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：报价只读绑定 owner/site/kind/id、candidate/config/world revision、quote correlation；比较真实建造/补料/移站/补电/延期，公开全部输入、电力、boundary、blocker 和目标关联的精确专业分配/验证关系、当前实现和实际验收仍由 §5.1 对应 owner 核定；不得以根 SC 的局部或历史结果代签。

<a id="root-sc23-buildpower"></a>
#### 功能状态卡：四类建设 power mode 边界

Trace ID: O7-REQ-8931e5de-af90-4c6a-8c2e-d21930e1a4ef

- 主责范围：仅 [SC-23](#sc-23) 的“各 mode 的 sink/hold/revalidation/best-effort/争用/unknown 分别验证；建设、配方、维护和 battery runway 分层，PowerPlant 仅 profile output boundary 后生效，入口事实同义”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定候选静态复核：真实建设action生成power obligation；FactoryBuildStarted准备层明确要求StartOnlySink、校验canonical power profile修订/金额/mode一致，并从builder实际扣Electricity，直接覆盖StartOnlySink子范围；其他三种mode、争用/unknown、PowerPlant output boundary及跨入口分层尚未核清。 源码/测试定位：[builtin_factory_quote.rs:10–100](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/builtin_factory_quote.rs#L10-L100)；[receipts.rs:1–90](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/economy_factory_lifecycle/receipts.rs#L1-L90)；[action_to_event_economy_factory.rs:195–321](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy_factory.rs#L195-L321)；[factory_lifecycle.rs:290–344](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L290-L344)；[factory_lifecycle.rs:390–410](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L390-L410)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：各 mode 的 sink/hold/revalidation/best-effort/争用/unknown 分别验证；建设、配方、维护和 battery runway 分层，PowerPlant 仅 profile output boundary 后生效，入口事实同义的精确专业分配/验证关系、当前实现和实际验收仍由 §5.1 对应 owner 核定；不得以根 SC 的局部或历史结果代签。

<a id="root-sc24-recipestate"></a>
#### 功能状态卡：六状态配方准入

Trace ID: O7-REQ-e912cdd3-0222-4a88-b920-983a4118a6d1

- 主责范围：仅 [SC-24](#sc-24) 的“pending_validation/validated_pending_admission/active/restricted/retiring/retired 及转换可读；blocker reason 不新增状态，生效前无效果，Agent 只在授权内提议确认”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：当前默认/内置模块生产路径静态闭合：RecipeProfileV1实际是四字段调参profile，治理和module release写同一map，schedule只读stage_gate/tags；模块发布状态不等于配方六状态。通用operator gate不代签六状态workflow内Agent权限承诺。 [profile-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L75-L86)；[profile-govern](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy_profiles.rs#L154-L201)；[profile-write](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/module_release_transition.rs#L605-L623)；[profile-install](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/module_release_transition.rs#L61-L69)；[profile-release](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/module_actions/release_support.rs#L512-L532)；[profile-bootstrap](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/resources.rs#L888-L898)；[native-recipe-command](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L435-L584)；[profile-projection](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/projection.rs#L730-L748)；[restore](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/persistence.rs#L732-L766)。本轮未运行业务测试、模型或部署；测试定义与历史CI不代签本卡。
- 核对日期与基线：2026-10-17；需求卡固定PR4844 source head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`，实现固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；相关source blob与前轮 `e90829da59bb8a064820d4e4ce3688018a351fb8`一致并逐blob复验。仅静态审读。
- 剩余范围：在本卡完整适用范围内交付并验证：pending_validation/validated_pending_admission/active/restricted/retiring/retired与转换可读；blocker reason不新增状态；生效前无recipe效果；Agent仅在授权内提议/确认。结论限当前仓库默认/内置模块，不推断未提供的线上自定义artifact；原SC与§5.1验收义务全部保留。 实施待安排： [Refs #4856](https://github.com/nil-studio/oasis7/issues/4856)；本状态审查不关闭该任务。

<a id="root-sc24-recipeversion"></a>
#### 功能状态卡：任务不可变配方版本与绑定

Trace ID: O7-REQ-70481f2f-9b55-4b99-afde-b4d451843d1d

- 主责范围：仅 [SC-24](#sc-24) 的“每个 accepted 任务固定不可变 recipe version/authority，并与 factory capability、输入批次、物流路径、output bundle 和 terminal purpose 关联”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 共同因果条件：[工业任务续接与因果换向](#root-sc20-jobcontinue)（Trace refs: O7-REQ-15abc2d8-2dc7-43ed-bb20-895dc11e8452）；这里不重复声明其身份。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定候选静态复核：RecipeStarted 将任务的 recipe ID、产物、输出账本与物流路线/路径等保存为 RecipeJobState；RecipeCompleted 要求其与 pending commitment 一致，直接支持任务配方 ID、产物与物流路径绑定的一部分。CommittedRecipeOrigin 是可选的共识提交关联元数据，不提供 recipe profile version，也不能作为配方 authority 的证据；每个 accepted 任务的不可变版本/authority、factory capability、输入批次谱系及 terminal purpose 的完整关联仍未核清。源码/测试定位：[state.rs:189–220](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state.rs#L189-L220)；[recipe_lifecycle.rs:330–353](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L330-L353)；[recipe_lifecycle.rs:438–454](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L438-L454)；[events.rs:78–103](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/events.rs#L78-L103)；[actions.rs:15–40](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/actions.rs#L15-L40)；[receipts.rs:1–90](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/economy_factory_lifecycle/receipts.rs#L1-L90)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：核清每个 accepted 任务的不可变 recipe version/authority、factory capability、输入批次谱系与 terminal purpose 的完整关联，以及已有任务配方 ID、产物和物流路径绑定在全部适用入口、失败与恢复条件下的验收；精确专业分配/验证关系仍由 §5.1 对应 owner 核定，不得以根 SC 的局部或历史结果代签。

<a id="root-sc24-recipedispose"></a>
#### 功能状态卡：配方生命周期六类工作处置

Trace ID: O7-REQ-df2f1a58-f435-4e86-b6c1-810f13f8267d

- 主责范围：仅 [SC-24](#sc-24) 的“新排程、accepted未sink、WIP、transit、buffer、terminal-pending 分别按状态和 profile 处置；retiring/retired 禁止新排程保留历史，successor 不自动迁移，terminal义务不静默改绑或提前结算”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：当前默认/内置模块生产路径静态闭合：配方治理不产生retiring/retired/successor状态；ScheduleRecipe没有按这些状态分类consumer。retired_factory检查属于设施回收，product_validation单job consumed_lost属于失败处置，都不构成本卡recipe lifecycle六类工作处置。 [profile-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L75-L86)；[profile-write](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/module_release_transition.rs#L605-L623)；[profile-release](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/module_actions/release_support.rs#L512-L532)；[native-recipe-command](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L435-L584)；[recipe-pause](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L15-L75)；[recipe-complete](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L369-L577)；[logistics-write](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L151-L271)；[profile-projection](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/projection.rs#L730-L748)；[restore](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/persistence.rs#L732-L766)。本轮未运行业务测试、模型或部署；测试定义与历史CI不代签本卡。
- 核对日期与基线：2026-10-17；需求卡固定PR4844 source head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`，实现固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；相关source blob与前轮 `e90829da59bb8a064820d4e4ce3688018a351fb8`一致并逐blob复验。仅静态审读。
- 剩余范围：在本卡完整适用范围内交付并验证：新排程/accepted未sink/WIP/transit/buffer/terminal-pending按recipe状态与profile分别处置；retiring/retired阻止新排程且保留历史；successor不自动迁移旧工作；terminal数量/recipient/destination/admission/capacity/provenance不静默改绑或提前结算。结论限当前仓库默认/内置模块，不推断未提供的线上自定义artifact；原SC与§5.1验收义务全部保留。 实施待安排： [Refs #4856](https://github.com/nil-studio/oasis7/issues/4856)；本状态审查不关闭该任务。

<a id="root-sc25-factorychoice"></a>
#### 功能状态卡：已建成工厂能力选择

Trace ID: O7-REQ-8a215d9f-cefc-4d06-82f3-935c44454558

- 主责范围：仅 [SC-25](#sc-25) 的“比较真实 upgrade/reconfigure/run_current/reduce_or_defer/retire_and_rebuild/defer，明确成本、占用、产出、时延、风险、可撤回性及推荐理由”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定main静态生产链复核（当前仓库默认/内置合同）：当前动作只提供独立维护/回收/暂停/排程；M4 factory模板返回建厂decision，现有producer未建立这组可比较能力候选。 单动作报价与job不含能力候选和五类存量承诺；目标root/window/terminal等仍明确未具备。 普通quote确只读且动作会重验，但缺能力选择身份/选择提交consumer；不把相邻报价当该前提内交付。 正向入口与实际写点：[action-family](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/world_action/action.rs#L486-L560)；[abi-profile](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L48-L127)；[viewer-consumer](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_card.jsx#L10-L28)；[recipe-quote](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/world_quote/recipe.rs#L2-L78)；[factory-quote](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/world_quote/factory.rs#L2-L112)；[job-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L189-L220)；[authority-root](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-runtime/prd.md#L398-L409)；[quote-producer](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/builtin_recipe_quote.rs#L11-L119)；[native-recipe-command](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L435-L584)；[module-recipe-command](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/economy.rs#L129-L370)。本轮未执行业务/GUI/Agent验收；不把既有普通quote、维护/回收、profile治理、job去重或文档治理PR当作本卡已交付子义务。
- 核对日期与基线：2026-10-17；实现源码固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；当前PR #4844需求候选 `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；148份来源逐Git blob核对、250/250 runtime与37内置producer同版。只读静态审查，业务/GUI/模型执行和部署未执行。
- 剩余范围：实现本卡专业合同：当前工厂能力的upgrade/reconfigure/run_current/reduce_or_defer/retire_and_rebuild/defer比较；逐候选成本、机会成本、影响五类存量、撤回性、风险/recheck；只读预览→fresh authority提交/unknown fail closed。原SC及§5.1全部适用验收保持，需随后取得同候选Viewer/pure API/Agent与恢复/重放证据。结论限当前默认内置路径，不覆盖未提供的线上自定义artifact。

<a id="root-sc25-factorydispose"></a>
#### 功能状态卡：五类存量工作合法处置

Trace ID: O7-REQ-aa02140c-4e3d-411e-97ba-28ad19830c5b

- 主责范围：仅 [SC-25](#sc-25) 的“accepted-unstarted/WIP/transit/buffer/terminal-pending 各仅 profile支持的单次处置；retiring/retired 禁新排程，successor需授权fresh conversion，不静默退款/销毁/复制或提前terminal settlement”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 第四轮生产链静态复核：已核清退役禁止新排程的直接子义务：回收 dispatcher 与 reducer 均拒绝仍有 pending recipe 的工厂；成功后移除 factory、记录 retired identity，RecipeStarted reducer 拒绝 retired factory。现有回收不会把在制 recipe 静默当作完成或退款。 精确源码：[action_to_event_economy.rs:356–435](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L356-L435)；[factory_lifecycle.rs:694–713](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L694-L713)；[factory_lifecycle.rs:744–813](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L744-L813)；[recipe_lifecycle.rs:148–160](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L148-L160)；[apply_domain_event_industry.rs:25–40](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/apply_domain_event_industry.rs#L25-L40)；[publication.rs:198–220](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/publication.rs#L198-L220)。本轮生产切片的交付 PR 映射尚未核清；本轮未执行业务测试；本判定基于直接实现子范围，不把未运行作为保留未知的理由，也不以相邻机制代签本卡。
- 核对日期与基线：2026-10-17；源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，逐文件 blob 与 main `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5` 核对同版；文档叠加基线 `aca7acec18d23ebc1d6ac444d7fdf4630380b389`；业务测试与 GUI 实测未执行。
- 剩余范围：这只覆盖无 active recipe 时回收及 retired 禁新排程，不是五类存量逐项处置。accepted-unstarted/WIP/transit/buffer/terminal-pending 的 profile 处置 receipt、retiring 阶段、successor 授权/fresh conversion 与 terminal recipient/admission/capacity/provenance 保持链仍未闭合。 原 SC/§5.1 适用验收全部保留；运行与跨入口完整验收仍待执行。

<a id="root-sc25-factorystate"></a>
#### 功能状态卡：工厂状态与单次变更

Trace ID: O7-REQ-df369b38-4338-4207-b6e0-60cfba3f12cb

- 主责范围：仅 [SC-25](#sc-25) 的“operational/reconfiguration_pending/reconfiguring/degraded/retiring/retired 产品状态可读；degraded 新工作须 profile at-risk；upgrade/retire/release/cutover/replay 不重复或复活旧candidate”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 第四轮生产链静态复核：已核清 native 工厂回收的单次变更/不复活子范围：FactoryRecycled 与持久 recycle receipt 比对，冲突拒绝；同 retired identity 重放不再 credit，成功持久化 retired set；BuildFactory 准入和 build-start/built reducer 均拒绝复活 retired ID。 精确源码：[factory_lifecycle.rs:122–156](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L122-L156)；[factory_lifecycle.rs:672–690](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L672-L690)；[factory_lifecycle.rs:797–813](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L797-L813)；[factory_lifecycle.rs:205–211](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L205-L211)；[factory_lifecycle.rs:472–478](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/factory_lifecycle.rs#L472-L478)；[action_to_event_economy_factory.rs:75–83](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy_factory.rs#L75-L83)；[industry_state.rs:196–239](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_state.rs#L196-L239)；[publication.rs:398–446](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/publication.rs#L398-L446)。本轮生产切片的交付 PR 映射尚未核清；本轮未执行业务测试；本判定基于直接实现子范围，不把未运行作为保留未知的理由，也不以相邻机制代签本卡。
- 核对日期与基线：2026-10-17；源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，逐文件 blob 与 main `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5` 核对同版；文档叠加基线 `aca7acec18d23ebc1d6ac444d7fdf4630380b389`；业务测试与 GUI 实测未执行。
- 剩余范围：现有 FactoryProductionStatus 为 Idle/Running/Blocked/Paused，不等于六类 capability 产品状态；尚未核清六状态读面、degraded at-risk 新工作、upgrade/reconfigure canonical cutover 和 candidate/旧 reservation/W 隔离。回收幂等不代签全部生命周期。 原 SC/§5.1 适用验收全部保留；运行与跨入口完整验收仍待执行。

<a id="root-sc27-discover"></a>
#### 功能状态卡：同快照配方候选发现

Trace ID: O7-REQ-d0c8d71e-dae0-405f-8b01-c6d573eaf719

- 主责范围：仅 [SC-27](#sc-27) 的“factory-ready/recipe-missing 后候选来自同权威快照，available/blocked/unknown 互异且同名版本不合并；目标、fit、输入、物流、电力、maintenance、output/terminal、风险/下一步可读”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定main静态生产链复核（当前仓库默认/内置合同）：固定首局gate与按调用者recipe ID选一项的quote不生成可发现版本集合；同名版本分离与三态producer未交付。 已有DTO只表达单项排程与短缺/电力，不能承载版本候选全部authority。 已有Task base/delivery drift是单quote范围；缺discovery result identity和available选择consumer，不能以task pin替代。 正向入口与实际写点：[starter-gate](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/starter_industrial.rs#L27-L210)；[recipe-quote](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/world_quote/recipe.rs#L2-L78)；[abi-profile](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L48-L127)；[abi-plan](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L209-L244)；[viewer-dto](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_proto/src/viewer/schedule_recipe_quote.rs#L5-L40)；[viewer-consumer](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_card.jsx#L10-L28)；[task-quote](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/world_service_private_recipe_quote.rs#L5-L78)；[task-quote-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_client_api/src/world_service/task_recipe_quote.rs#L14-L89)；[native-recipe-command](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L435-L584)。本轮未执行业务/GUI/Agent验收；不把既有普通quote、维护/回收、profile治理、job去重或文档治理PR当作本卡已交付子义务。
- 核对日期与基线：2026-10-17；实现源码固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；当前PR #4844需求候选 `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；148份来源逐Git blob核对、250/250 runtime与37内置producer同版。只读静态审查，业务/GUI/模型执行和部署未执行。
- 剩余范围：实现本卡专业合同：factory-ready/recipe-missing驱动同快照版本化候选及三态；每候选完整fit/input/logistics/power/maintenance/output/terminal/risk/recheck；available选择fresh提交；blocked/unknown只读且漂移重发现。原SC及§5.1全部适用验收保持，需随后取得同候选Viewer/pure API/Agent与恢复/重放证据。结论限当前默认内置路径，不覆盖未提供的线上自定义artifact。

<a id="root-sc28-profile"></a>
#### 功能状态卡：完整代表性执行档案

Trace ID: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117

- 主责范围：仅 [SC-28](#sc-28) 的“同 candidate 的 factory/pinned recipe、两类原料batch/quality/custody、物流、cycle、power、主副产物bundle、terminal与progression完整只读投影；缺authority显示incomplete/unknown/blocked”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定main静态生产链复核（当前仓库默认/内置合同）：存在材料kind/quantity、周期、电力和主副产物；required-parent batch/custody、bundle/terminal identity是明确目标；未组成完整profile producer。 报价的rejection和维护not tracked不等于完整profile completeness gating；profile-specific DTO/consumer未交付。 现有production milestone不是完整profile terminal-admission，两完成边界/因果新candidate不能由job完成代签。 正向入口与实际写点：[abi-profile](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L48-L127)；[abi-plan](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L209-L244)；[job-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L189-L220)；[recipe-quote](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/world_quote/recipe.rs#L2-L78)；[authority-root](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-runtime/prd.md#L398-L409)；[viewer-dto](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_proto/src/viewer/schedule_recipe_quote.rs#L5-L40)；[viewer-consumer](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_card.jsx#L10-L28)；[api-projection](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/world_service/projection.rs#L10-L140)；[recipe-write](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L325-L577)；[logistics-write](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L151-L271)；[viewer-projection](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L291-L375)。本轮未执行业务/GUI/Agent验收；不把既有普通quote、维护/回收、profile治理、job去重或文档治理PR当作本卡已交付子义务。
- 核对日期与基线：2026-10-17；实现源码固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；当前PR #4844需求候选 `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；148份来源逐Git blob核对、250/250 runtime与37内置producer同版。只读静态审查，业务/GUI/模型执行和部署未执行。
- 剩余范围：实现本卡专业合同：同candidate完整factory/pinned recipe/两类input batch-quality-custody/logistics/cycle/power/output/terminal/progression投影；任一authority缺失即incomplete/unknown/blocked，不宣称可排程或交付；两类完成边界、无选择无hold、因果新candidate W=0及三入口同义。原SC及§5.1全部适用验收保持，需随后取得同候选Viewer/pure API/Agent与恢复/重放证据。结论限当前默认内置路径，不覆盖未提供的线上自定义artifact。

<a id="root-sc29-sitecompare"></a>
#### 功能状态卡：同目标双站点比较

Trace ID: O7-REQ-982fc046-c8dc-43d1-bc0c-8b6bd3e0244b

- 主责范围：仅 [SC-29](#sc-29) 的“至少两个合法site比较输入/输出路径、容量损耗、电力、terminal、建设成本、交付风险和扩张机会成本，available/blocked/unknown可分”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定main静态生产链复核（当前仓库默认/内置合同）：单site建造quote可给真实成本/authority，但没有同目标site候选集合及比较producer。 建设DTO只给建设事实与成本；缺同目标输入输出拓扑与terminal/机会成本聚合。 既有读面不消费site比较DTO，不把注册站点或手填单站点报价称为比较交付。 正向入口与实际写点：[factory-quote](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/world_quote/factory.rs#L2-L112)；[factory-admission](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy_factory.rs#L7-L323)；[abi-profile](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L48-L127)；[authority-root](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-runtime/prd.md#L398-L409)；[viewer-consumer](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_card.jsx#L10-L28)；[api-projection](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/world_service/projection.rs#L10-L140)；[viewer-projection](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L291-L375)。本轮未执行业务/GUI/Agent验收；不把既有普通quote、维护/回收、profile治理、job去重或文档治理PR当作本卡已交付子义务。
- 核对日期与基线：2026-10-17；实现源码固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；当前PR #4844需求候选 `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；148份来源逐Git blob核对、250/250 runtime与37内置producer同版。只读静态审查，业务/GUI/模型执行和部署未执行。
- 剩余范围：实现本卡专业合同：同工业目标至少两个合法site比较且三态分离；input/output paths、capacity/loss、power/terminal、风险与扩张机会成本；玩家同义可读选择、明确blocker/recheck/恢复。原SC及§5.1全部适用验收保持，需随后取得同候选Viewer/pure API/Agent与恢复/重放证据。结论限当前默认内置路径，不覆盖未提供的线上自定义artifact。

<a id="root-sc29-siteadvisory"></a>
#### 功能状态卡：advisory topology 与建造前置分离

Trace ID: O7-REQ-328d5c5c-24d4-4bf6-bfd5-d1f968c9fdd2

- 主责范围：仅 [SC-29](#sc-29) 的“未固定recipe的特定topology仅advisory；只有site facts/profile construction prerequisite进入BuildFactory重验，advisory漂移不阻断建厂而在recipe discovery/ScheduleRecipe重读”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定main静态复核：原根PRD建造提交条款直接要求fresh site/profile验证和进入ScheduleRecipe后读取recipe topology，不冻结runtime schema。当前[BuildFactory实际准入](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy_factory.rs#L97-L323)从当前authority验证owner/access、location/chunk、factory profile、建设电力和材料，并冻结site/profile义务；[ScheduleRecipe实际准入](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L528-L735)另行读取recipe fit/stage、input ledger与显式物流path authority。两者直接支持本卡上述有限子范围，故部分实现，不要求特定DTO名字才承认。单site quote没有完整advisory/recipe-bound construction分类；[固定Assembler额外gate](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy_factory.rs#L7-L13)还依赖starter feasibility，因此不能证明所有unpinned advisory漂移不阻建造。普通quote与profile字段不能代签该剩余跨阶段语义。当前仅静态审读，未取得本卡同候选全量运行证据；本轮未执行业务/GUI/Agent验收。
- 核对日期与基线：2026-10-17；实现源码固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；当前PR #4844需求候选 `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；148份来源逐Git blob核对、250/250 runtime与37内置producer同版。只读静态审查，业务/GUI/模型执行和部署未执行。
- 剩余范围：补齐未固定recipe时advisory estimate与site/profile-declared construction prerequisite的权威分类及unknown/blocked读面；证明仅建设前置漂移才重报价/原子拒绝、unpinned advisory漂移不阻BuildFactory，并在discovery/ScheduleRecipe重读完整input/output/capacity/power/terminal topology；处理固定Assembler额外feasibility gate适用边界。保留原SC/§5.1验收，不把当前site/profile与排程子范围扩大为完整合同。

<a id="root-sc30-subchoice"></a>
#### 功能状态卡：合法替代与 no_legal_substitute

Trace ID: O7-REQ-8a79f47e-f132-4467-8d09-d0ceacaac868

- 主责范围：仅 [SC-30](#sc-30) 的“区分同料换来源、显式异料替代、换配方；比较数量/ratio/rounding/residual、损耗、power/物流/terminal、value、风险/W/复查；默认禁止异料，未知不伪装degraded安全”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定main静态生产链复核（当前仓库默认/内置合同）：当前RecipeProfile及plan为固定材料配比/输入，既有动作无该分类producer或candidate DTO。 单项排程成本可读，但非替代候选的比例、residual或全产出影响比较。 现有材料不足或固定kind检查不能当成profile-declared substitution资格判断；该结果consumer未交付。 正向入口与实际写点：[abi-profile](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L48-L127)；[abi-plan](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L209-L244)；[action-family](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/world_action/action.rs#L486-L560)；[recipe-quote](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/world_quote/recipe.rs#L2-L78)；[viewer-dto](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_proto/src/viewer/schedule_recipe_quote.rs#L5-L40)；[viewer-consumer](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_card.jsx#L10-L28)；[native-recipe-command](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L435-L584)。本轮未执行业务/GUI/Agent验收；不把既有普通quote、维护/回收、profile治理、job去重或文档治理PR当作本卡已交付子义务。
- 核对日期与基线：2026-10-17；实现源码固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；当前PR #4844需求候选 `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；148份来源逐Git blob核对、250/250 runtime与37内置producer同版。只读静态审查，业务/GUI/模型执行和部署未执行。
- 剩余范围：实现本卡专业合同：同料换来源/显式异料替代/换配方权威分类；ratio/rounding/residual、损耗/占用/value/risk/W/recheck可比较；默认禁止异料，缺authority/provenance返回unknown/blocked/no_legal_substitute。原SC及§5.1全部适用验收保持，需随后取得同候选Viewer/pure API/Agent与恢复/重放证据。结论限当前默认内置路径，不覆盖未提供的线上自定义artifact。 实施待安排： [Refs #4858](https://github.com/nil-studio/oasis7/issues/4858)；本状态审查不关闭该任务。

<a id="root-sc30-subsettle"></a>
#### 功能状态卡：替代决定与实际 conversion 分离

Trace ID: O7-REQ-300c6cbb-49f5-4858-a749-309991d5a5d0

- 主责范围：仅 [SC-30](#sc-30) 的“preview无效果，确认fresh后一次decision；decision-only无sink/credit/conversion/W/reward；实际conversion须profile支持且有linked exactly-once settlement/provenance”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定main静态生产链复核（当前仓库默认/内置合同）：当前readonly quote只有ScheduleRecipe preview，实际动作产RecipeStarted直接sink；没有替代决定的持久记录或consumer。 现有生产启动/完成是sink/credit路径，不是无效果替代decision；不能将普通报价幂等视为替代确认。 profile没有substitution/conversion政策，当前per-job写点无该linked receipt；通用回放不补造字段。 正向入口与实际写点：[quote-producer](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/builtin_recipe_quote.rs#L11-L119)；[action-family](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/world_action/action.rs#L486-L560)；[job-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L189-L220)；[native-recipe-command](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_economy.rs#L435-L584)；[recipe-write](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L325-L577)；[abi-profile](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L48-L127)；[authority-root](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-runtime/prd.md#L398-L409)。本轮未执行业务/GUI/Agent验收；不把既有普通quote、维护/回收、profile治理、job去重或文档治理PR当作本卡已交付子义务。
- 核对日期与基线：2026-10-17；实现源码固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；当前PR #4844需求候选 `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；148份来源逐Git blob核对、250/250 runtime与37内置producer同版。只读静态审查，业务/GUI/模型执行和部署未执行。
- 剩余范围：实现本卡专业合同：替代preview无效果→确认fresh后唯一decision-only；decision-only无sink/credit/conversion/W/reward；仅profile支持实际conversion且独立linked exactly-once settlement/provenance。原SC及§5.1全部适用验收保持，需随后取得同候选Viewer/pure API/Agent与恢复/重放证据。结论限当前默认内置路径，不覆盖未提供的线上自定义artifact。 实施待安排： [Refs #4858](https://github.com/nil-studio/oasis7/issues/4858)；本状态审查不关闭该任务。

<a id="root-sc30-subcause"></a>
#### 功能状态卡：替代因果与防套利

Trace ID: O7-REQ-84ea0d88-f33b-4ac1-9722-ca4892df9152

- 主责范围：仅 [SC-30](#sc-30) 的“仅全产出因果不变的profile替代保留pinned recipe并建child decision，其余新candidate W=0；首sink后仅支持的单次处置，重试/切换/拆分不复制或套利”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [M4 工业设计 §6](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)与[恢复映射](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。这些设计精确承接的是报价净缺口/消费账本、首局同参数提交和未知结果恢复；其分配表显式排除完整工业执行系统。对应 M4 专业 PRD 有更广目标，不能据此将本独立义务的系统设计判为完整。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17 固定main静态生产链复核（当前仓库默认/内置合同）：当前plan固定产出、电力、时长，不提供质量/terminal/value-class全因果等价判定或替代child身份。 Runtime正面说明当前无root/revision/parent-child，ordinary committed_recipe_origin不是专业替代candidate lineage。 当前recipe/transit各自settle去重；无替代policy、lineage与互斥conversion/处置consumer，故不能代签替代套利边界。 正向入口与实际写点：[abi-profile](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L48-L127)；[abi-plan](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_wasm_abi/src/economy.rs#L209-L244)；[job-schema](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L189-L220)；[authority-root](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/doc/world-runtime/prd.md#L398-L409)；[recipe-write](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L325-L577)；[action-family](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/world_action/action.rs#L486-L560)；[logistics-write](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L151-L271)。本轮未执行业务/GUI/Agent验收；不把既有普通quote、维护/回收、profile治理、job去重或文档治理PR当作本卡已交付子义务。
- 核对日期与基线：2026-10-17；实现源码固定main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`；当前PR #4844需求候选 `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；148份来源逐Git blob核对、250/250 runtime与37内置producer同版。只读静态审查，业务/GUI/模型执行和部署未执行。
- 剩余范围：实现本卡专业合同：全产出因果等价判定才保留pinned recipe和child decision；其他替代/换配方parent-linked新candidate W=0；首sink后按profile单次rework/conversion/return/hold/salvage并防切换拆分套利。原SC及§5.1全部适用验收保持，需随后取得同候选Viewer/pure API/Agent与恢复/重放证据。结论限当前默认内置路径，不覆盖未提供的线上自定义artifact。 实施待安排： [Refs #4858](https://github.com/nil-studio/oasis7/issues/4858)；本状态审查不关闭该任务。

<a id="root-sc32-institution"></a>
#### 功能状态卡：native 制度互斥分类与冻结

Trace ID: O7-REQ-90614e34-bbc7-42df-b355-7ce0b34ccdc3

- 主责范围：仅 [SC-32](#sc-32) 的“Alliance=Compatibility keep、War=Defer、Governance=Kernel keep、EconomicContract=Compatibility keep；未完成互斥分类不得新增native制度action/state”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未核实
- 系统设计依据：已核对 [Runtime 制度迁移](../../world-runtime/design.md#61-institution-migration-test)、[事务模型](../../world-runtime/design.md#621-explicit-transaction-and-typed-delta-model)。Runtime 已有制度迁移/事务架构；产品四类分类、唯一 pilot、owner/payer 读面与 budget 单次结算需分别核对，架构 current 不等于每条玩家义务已获完整专业承接。本卡不将相关设计的存在表述为本要求已完整覆盖。 本轮重读确认上述设计有相关阶段/制度条款，但尚无本卡全部独立义务的精确反向分配与验证关系；不由相邻叶子或泛 SC-32 文字提升本卡设计状态。
- 实现与验证依据：mapping已固定而非待冻结；真实native actions、prepared写点和module发布分工明确。剩余未知是完整Action/State归属与新增变更的专业确认/冻结实践，不是缺一个runtime枚举或自动阻断器；当前既有写点不单独证明全部兼容/冻结义务。 证据：[contract](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/doc/product/world-rules-core-gameplay/prd.md#L549-L582)；[sc32](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/doc/product/world-rules-core-gameplay/prd.md#L706-L750)；[native-dispatch](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/state.rs#L1098-L1139)；[native-writer](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/state/apply_domain_event_gameplay.rs#L7-L50)；[contract-action](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/event_processing/action_to_event_policy_contract.rs#L793-L1003)；[checker](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/scripts/product-doc-governance-check.py#L172-L360)；[migration](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/doc/world-runtime/design.md#L125-L176)。
- 核对日期与基线：2026-10-17；固定 main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，使用源逐Git blob校验。相对e27f4d9业务源码无变化，复用旧生产闭包而非旧结论；#4865未合代码不计。完整SC-5/SC-32与§5.1适用条件已读。仅静态审查，未运行模型、Runtime、业务测试或部署；测试源码不标PASS。
- 剩余范围：四个既有native制度精确互斥分类，未分类不得新增native action/state。未核范围具体保留，不把政策、设计和通用能力提升为制度迁移事实。 需按实际变更批准记录或已选pilot manifest/activation、旧结果连续性证据重新核对；不以缺运行证据单独否定源码。

<a id="root-sc32-pilot"></a>
#### 功能状态卡：唯一治理批准的制度试点

Trace ID: O7-REQ-f081f96d-f95a-4538-b0b7-3edf23ff772a

- 主责范围：仅 [SC-32](#sc-32) 的“Alliance或EconomicContract择一于合法activation转Module candidate，另一保持兼容；不绕过L0/L1，旧snapshot/replay/玩家结果/单世界连续，版本ABI和失败恢复须验证”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未核实
- 系统设计依据：已核对 [Runtime 制度迁移](../../world-runtime/design.md#61-institution-migration-test)、[事务模型](../../world-runtime/design.md#621-explicit-transaction-and-typed-delta-model)。Runtime 已有制度迁移/事务架构；产品四类分类、唯一 pilot、owner/payer 读面与 budget 单次结算需分别核对，架构 current 不等于每条玩家义务已获完整专业承接。本卡不将相关设计的存在表述为本要求已完整覆盖。 本轮重读确认上述设计有相关阶段/制度条款，但尚无本卡全部独立义务的精确反向分配与验证关系；不由相邻叶子或泛 SC-32 文字提升本卡设计状态。
- 实现与验证依据：当前政策允许选择前无candidate；已审通用release与module writer不产生可识别制度选择。实际唯一governed manifest/activation及相应连续证据未建立，条件未启动与代码缺失分别保留，不推定已经迁移，不因未选直接判未实现。 证据：[contract](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/doc/product/world-rules-core-gameplay/prd.md#L549-L582)；[migration](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/doc/world-runtime/design.md#L125-L176)；[release](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/module_actions/release_actions.rs#L206-L327)；[release-approval](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/module_actions/release_support.rs#L102-L139)；[module-writer](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/module_runtime_publication.rs#L11-L103)；[governance](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/governance_meta_publication.rs#L238-L401)；[native-writer](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/state/apply_domain_event_gameplay.rs#L7-L50)。
- 核对日期与基线：2026-10-17；固定 main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，使用源逐Git blob校验。相对e27f4d9业务源码无变化，复用旧生产闭包而非旧结论；#4865未合代码不计。完整SC-5/SC-32与§5.1适用条件已读。仅静态审查，未运行模型、Runtime、业务测试或部署；测试源码不标PASS。
- 剩余范围：条件满足后选择唯一Alliance/EconomicContract，合法activation、另一兼容，L0/L1/ownerbudget/receipt及旧snapshot/replay/ABI恢复。未核范围具体保留，不把政策、设计和通用能力提升为制度迁移事实。 需按实际变更批准记录或已选pilot manifest/activation、旧结果连续性证据重新核对；不以缺运行证据单独否定源码。

<a id="root-sc32-budgetread"></a>
#### 功能状态卡：owner-bound budget 玩家读面

Trace ID: O7-REQ-96cb1655-2ca3-445f-af2e-9f113e6f3e8e

- 主责范围：仅 [SC-32](#sc-32) 的“owner/payer、quote、affordability、unknown/degraded可解释；执行预算与Electricity/Data等资源分离，不隐式转移payer或预算”。这里的 execution budget 是能力执行计量，不替代玩家委托给 Agent 的授权累计额度。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [Runtime 制度迁移](../../world-runtime/design.md#61-institution-migration-test)、[事务模型](../../world-runtime/design.md#621-explicit-transaction-and-typed-delta-model)。Runtime 已有制度迁移/事务架构；产品四类分类、唯一 pilot、owner/payer 读面与 budget 单次结算需分别核对，架构 current 不等于每条玩家义务已获完整专业承接。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17固定main只读续核：已定位直接CapabilityGrantV2 execution-budget链，不再仅依赖module所有权测试。CapabilityBudgetAccount按subject+grant_id键保存remaining/reserved/spent，与artifact-owner Data/Electricity费用分离；Agent owner_binding/generation由live identity校验。Catalog携带subject/audience/能力scope，authorization receipt携带budget_before/after；governed quote仅units及模块/schema/有效期。以上是局部底层读出/quote切片，catalog未投影预算、quote未给owner/payer或affordability四态，未定位玩家端完整消费。 定位：[capability_authorization](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/capability_authorization.rs#L150-L166)；[capability_catalog](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/capability_catalog.rs#L111-L218)；[cognition_command](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/cognition_command.rs#L19-L65)；[capability_authorization](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/capability_authorization.rs#L216-L260)。
- 核对日期与基线：2026-10-17；需求/设计/静态实现固定 `6463975178f3d32ef6d8e19e14f0b24f1c7f9471`；引用源码逐文件 Git blob 校验。本轮未运行测试或产品验收。
- 剩余范围：补全Agent及Module/System的owner/payer映射、能力scope/是否计费/预算影响、affordable/insufficient/unknown/degraded与复查修复路径，贯通真实Viewer/API或能力预览/提交结果；不得以原始账本getter、snapshot或Data/Electricity资源费替代玩家execution-budget读面。保留SC-32适用条件和test_tier_full；本次未运行。

<a id="root-sc32-budgetcommit"></a>
#### 功能状态卡：预算与世界效果原子单次提交

Trace ID: O7-REQ-70e55a1f-7087-41e8-918d-7d1989c0155a

- 主责范围：仅 [SC-32](#sc-32) 的“preview/stale/deny/无效果失败不debit，actual effect+debit+receipt原子一次；retry/replay不重复扣费/credit，owner失配或unknown不能透支”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：已核对 [Runtime 制度迁移](../../world-runtime/design.md#61-institution-migration-test)、[事务模型](../../world-runtime/design.md#621-explicit-transaction-and-typed-delta-model)。Runtime 已有制度迁移/事务架构；产品四类分类、唯一 pilot、owner/payer 读面与 budget 单次结算需分别核对，架构 current 不等于每条玩家义务已获完整专业承接。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：2026-10-17固定main只读续核：execute_trusted_module_command在sandbox前按subject/grant账户检查和reservation，staged输出验证后按确定性logical units结算，CommandCommitted将budget/grant/nonce/authorization receipt/effect links同批准备发布；post-prepare失败、insufficient、owner失配、nonce重试和snapshot/replay均有直接源码/定向fixture。governed wrapper另有preview/no-effect免费门，但调用定位仅本身和测试，adapter直接trusted路径未经过该门。排队effect及授权receipt不能替代外部effect已完成；专业设计亦明确持久提交/outbox/恢复整体仍partial。 定位：[capability_authorization_command](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/capability_authorization_command.rs#L263-L416)；[cognition_command](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/world/cognition_command.rs#L72-L194)；[provider_loopback_adapter](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/simulator/provider_loopback_adapter.rs#L16-L100)；[capability_grant_v2](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7/src/runtime/tests/capability_grant_v2.rs#L1080-L1127)。
- 核对日期与基线：2026-10-17；需求/设计/静态实现固定 `6463975178f3d32ef6d8e19e14f0b24f1c7f9471`；引用源码逐文件 Git blob 校验。本轮未运行测试或产品验收。
- 剩余范围：继续贯通全部适用生产入口preview/stale/deny/no-effect无扣账与零预算操作；核准实际effect和receipt边界、外部effect completion及持久unknown恢复，给出同候选test_tier_full单次结算/重连/restore/replay/no-credit证据。已定位直接实现但全义务未闭合，故部分实现而非仅欠验收的待验证。

<a id="root-sc33-extscope"></a>
#### 功能状态卡：可选外部性 profile 与 legacy 边界

Trace ID: O7-REQ-e6f4ffa7-0b1b-4d0e-b7a3-8ca38c8d0244

- 主责范围：仅 [SC-33](#sc-33) 的“只适用明确industrial_externality/operational_mitigation profile；legacy无profile不变，scope/authority缺失或过期unknown/blocked不可选不可逆动作，不能获得区域或治理权”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [外部性专业合同](../../game/gameplay/gameplay-industrial-externality-contract.prd.md#4-currenttarget-evidence-cutline)、[M4 工业设计](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)。专业合同明确是 target-contract，并说明当前证据没有 factory/recipe externality profile、mitigation ledger 或 industrial impact receipt；M4 报价设计不是这些机制的系统设计。尚不能断言全仓无实现，也不能将专业 PRD 充作完整技术设计。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：已读当前default/builtin profile→plan→recipe写链；没有工业外部性scope/authority分支。radiation source、crisis containment和标签不能代替该可选facet。 [profiles](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_wasm_abi/src/economy.rs#L75-L122)；[recipe-template](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_builtin_wasm_modules/_templates/m4_recipe_module_template.rs#L27-L84)；[dispatcher](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/event_processing.rs#L379-L478)；[publication](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/event_processing/publication.rs#L170-L221)；[externality-authority](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/doc/game/gameplay/gameplay-industrial-externality-contract.prd.md#L1-L53)；[restore](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/persistence.rs#L735-L785)。仅当前默认/内置路径静态审读，未运行业务测试或正式玩家验收。
- 核对日期与基线：2026-10-17；需求固定 PR #4844 `5d03f1fcda523b071c2d75f634165af344ca7b6b`；实现固定 main `e27f4d990bc4d481097c003ff75afc582fd8f65d`；所引源码逐 Git blob 复验，与前轮同源部分复用；无新运行验收。
- 剩余范围：明确industrial_externality/operational_mitigation profile才启用：实际RecipeProfile/FactoryProfile及内置M4 plan没有此facet与启用consumer；legacy无profile不变：现有legacy仍运行是邻域既有事实；没有引入新facet后的分支/兼容合同，不能用未实施即未变化作partial；scope/authority缺失过期unknown/blocked且不授区域治理权：当前排程guard处理普通工厂/配方/材料/电力，不读取mitigation authority，无法对声明该profile的case作要求的判断。结论限当前仓库默认/内置及已审正式路径；未提供的线上自定义artifact不在本轮结论范围。 实施待安排： [Refs #4862](https://github.com/nil-studio/oasis7/issues/4862)；本状态审查不关闭该任务。

<a id="root-sc33-extdispose"></a>
#### 功能状态卡：外部性处置与其他事实隔离

Trace ID: O7-REQ-bd83785c-d414-4bd7-b291-41a4d7272aa3

- 主责范围：仅 [SC-33](#sc-33) 的“preview只读，submit fresh，linked production/externality disposition一次；quality/custody/byproduct/maintenance/settlement/W不被改写，WIP/transit/buffer/terminal保持root/revision及义务单次处置”。验收沿用原 SC 对应的适用条件、成功/失败边界和 §5.1 测试层级；不创建新 AC 或缩小原文。
- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：已核对 [外部性专业合同](../../game/gameplay/gameplay-industrial-externality-contract.prd.md#4-currenttarget-evidence-cutline)、[M4 工业设计](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)。专业合同明确是 target-contract，并说明当前证据没有 factory/recipe externality profile、mitigation ledger 或 industrial impact receipt；M4 报价设计不是这些机制的系统设计。尚不能断言全仓无实现，也不能将专业 PRD 充作完整技术设计。本卡不将相关设计的存在表述为本要求已完整覆盖。
- 实现与验证依据：外部性disposition与生产/终端的独立事实链尚未建立；工业recipe/transit普通恢复只承接自身合同。 [profiles](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_wasm_abi/src/economy.rs#L75-L122)；[recipe-template](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_builtin_wasm_modules/_templates/m4_recipe_module_template.rs#L27-L84)；[publication](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/event_processing/publication.rs#L170-L221)；[loss-write](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L580-L700)；[logistics-write](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L44-L115)；[externality-authority](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/doc/game/gameplay/gameplay-industrial-externality-contract.prd.md#L1-L53)；[snapshot](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/persistence.rs#L306-L380)；[restore](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/persistence.rs#L735-L785)。仅当前默认/内置路径静态审读，未运行业务测试或正式玩家验收。
- 核对日期与基线：2026-10-17；需求固定 PR #4844 `5d03f1fcda523b071c2d75f634165af344ca7b6b`；实现固定 main `e27f4d990bc4d481097c003ff75afc582fd8f65d`；所引源码逐 Git blob 复验，与前轮同源部分复用；无新运行验收。
- 剩余范围：preview只读、submit fresh、linked production/externality各一次：普通quote与生产receipt存在，但没有externality intent/disposition身份及linked写点；不把通用只读和job exactly-once当外部性完成；quality/custody/byproduct/maintenance/settlement/W互不改写：不存在外部性处置producer可证明隔离；当前材料/维护/物流分别写不等同新facet隔离已交付；WIP/transit/buffer/terminal保持root/revision和既有义务单次处置：普通pending job/transit有独立结算，没有externality profile变化的root/revision/WIP及terminal处置consumer。结论限当前仓库默认/内置及已审正式路径；未提供的线上自定义artifact不在本轮结论范围。 实施待安排： [Refs #4862](https://github.com/nil-studio/oasis7/issues/4862)；本状态审查不关闭该任务。


## 8. 既有节点经济合同的结果接收

本节只为已经由下列 P2P 专业 SC/AC/DEC 采用的节点贡献、受限资产兑电和创世控制结果登记产品位置与唯一身份，不新增经济功能或面向所有玩家的收益承诺。原适用角色、阶段、前提、成功与拒绝条件、局部 PRD-ID、历史记录和演进计划全部保留。经济参数、公式、预算、额度、recipient/controller、vesting、签名规则、approved ledger、actual-value review、QA 和执行 authority 仍分别由原 P2P/runtime、README reward governance 与 QA 拥有；本根与 game 不取得新的调参、批准、发放或执行权。根只引用参数真值，不复制数值表。

消费/组合说明：[参与与权益分离](../player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001)与[同候选发行门禁](../player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001)继续拥有各自结果，本节不重复声明其身份。基础设施只提供[同输入确定性与原子拒绝](../world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)子范围，不能代替以下经济结果；epoch/node幂等不等同互斥intent lineage。PowerCredit路径不改starter支持、liquid starter OC或任何原有资源反补贴限制。下列条款不声称当前兑付、mint、奖励发放或生产可用；未取得完整证据前保持未核实。

<a id="req-wr-economic-points-eligibility"></a>
### REQ-WR-ECONOMIC-POINTS-ELIGIBILITY：基础义务与合格额外贡献分离

Trace ID: O7-REQ-118e8c96-669b-4b11-bbec-a32831803e16

适用范围与成功标准：为自身 Agent 模拟是基础义务，不直接获奖励；代跑离线节点、世界维护、有效存储和合格在线仅按原专业规则进入积分计算。 仅接收[既有专业义务](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-eligibility)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[基础义务与合格额外贡献分离](#req-wr-economic-points-eligibility)；验收沿[原专业接受关系](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-eligibility)。
- 对应验收：[AC-WR-ECONOMIC-POINTS-ELIGIBILITY](#ac-wr-economic-points-eligibility)。
- 系统设计状态：未核实
- 系统设计依据：[基础义务与合格额外贡献分离有限设计](../../p2p/node/node-contribution-points.design.md#des-economic-points-eligibility)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：真实build_settlement仅计算代跑/维护，self_sim用于基础义务惩罚；完整采样可信性与所有角色画像运行未核实。 源码/测试定位：[node_points.rs:175–300](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L175-L300)；[node_points_runtime.rs:325–381](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points_runtime.rs#L325-L381)；[reward_runtime_worker.rs:325–349](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L325-L349)；[node_points.rs:557–580](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L557-L580)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不能推出所有玩家获益、实际采样可信或公开网络安全。

<a id="ac-wr-economic-points-eligibility"></a>
### AC-WR-ECONOMIC-POINTS-ELIGIBILITY：基础义务与合格额外贡献分离的原验收

Trace refs: O7-REQ-118e8c96-669b-4b11-bbec-a32831803e16；覆盖要求：[REQ-WR-ECONOMIC-POINTS-ELIGIBILITY](#req-wr-economic-points-eligibility)。

在[原专业条件与场景](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-eligibility)适用的固定候选中逐项核验：为自身 Agent 模拟是基础义务，不直接获奖励；代跑离线节点、世界维护、有效存储和合格在线仅按原专业规则进入积分计算。 原验收成功/拒绝条件及层级不变，实际证据至少保留原贡献画像、输入、资格与得分对账；没有对应完整证据时保持未核实。不能推出所有玩家获益、实际采样可信或公开网络安全。本验收不授权执行经济操作。

<a id="req-wr-economic-points-allocation"></a>
### REQ-WR-ECONOMIC-POINTS-ALLOCATION：固定双池的确定性且不重复分配

Trace ID: O7-REQ-1bd30efb-22b7-403e-94f6-fe0a2efde4a2

适用范围与成功标准：两个固定 epoch 池分别按原合格正得分分配，独立存储池启用时主池排除存储权重；允许未分配余额，保持守恒、排序、惩罚效果和累计单调。 仅接收[既有专业义务](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-allocation)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[固定双池的确定性且不重复分配](#req-wr-economic-points-allocation)；验收沿[原专业接受关系](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-allocation)。
- 对应验收：[AC-WR-ECONOMIC-POINTS-ALLOCATION](#ac-wr-economic-points-allocation)。
- 系统设计状态：未核实
- 系统设计依据：[固定双池的确定性且不重复分配有限设计](../../p2p/node/node-contribution-points.design.md#des-economic-points-allocation)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：settle_epoch分别调用主/存储池分配并checked累计；3节点2epoch、严格阈值与独立存储池全部配置运行未核实。 源码/测试定位：[node_points.rs:175–300](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L175-L300)；[node_points_runtime.rs:325–381](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points_runtime.rs#L325-L381)；[reward_runtime_worker.rs:325–349](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L325-L349)；[node_points.rs:557–580](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L557-L580)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不调整预算/权重/阈值，不证明实际数值平衡。

<a id="ac-wr-economic-points-allocation"></a>
### AC-WR-ECONOMIC-POINTS-ALLOCATION：固定双池的确定性且不重复分配的原验收

Trace refs: O7-REQ-1bd30efb-22b7-403e-94f6-fe0a2efde4a2；覆盖要求：[REQ-WR-ECONOMIC-POINTS-ALLOCATION](#req-wr-economic-points-allocation)。

在[原专业条件与场景](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-allocation)适用的固定候选中逐项核验：两个固定 epoch 池分别按原合格正得分分配，独立存储池启用时主池排除存储权重；允许未分配余额，保持守恒、排序、惩罚效果和累计单调。 原验收成功/拒绝条件及层级不变，实际证据至少保留逐epoch/node得分、两池分配及预算差额；没有对应完整证据时保持未核实。不调整预算/权重/阈值，不证明实际数值平衡。本验收不授权执行经济操作。

<a id="req-wr-economic-points-recovery"></a>
### REQ-WR-ECONOMIC-POINTS-RECOVERY：epoch结算与恢复不重复入账

Trace ID: O7-REQ-54617172-fa9b-4efe-a058-b327ce348407

适用范围与成功标准：同一 epoch/node 结算幂等；采样或快照写入不直接增加奖励，恢复、重复tick与replay只能复现结果；不可读collector状态须可观测降级，不是资产对账成功。 仅接收[既有专业义务](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-recovery)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[epoch结算与恢复不重复入账](#req-wr-economic-points-recovery)；验收沿[原专业接受关系](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-recovery)。
- 对应验收：[AC-WR-ECONOMIC-POINTS-RECOVERY](#ac-wr-economic-points-recovery)。
- 系统设计状态：未核实
- 系统设计依据：[epoch结算与恢复不重复入账有限设计](../../p2p/node/node-contribution-points.design.md#des-economic-points-recovery)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：worker实际载入/保存collector snapshot，collector恢复ledger/epoch/cursor，mint准备层拒绝相同epoch/node重复记录，直接覆盖恢复与去重子义务；多节点崩溃窗口、降级后重复提交与replay全矩阵仍未核清。 源码/测试定位：[node_points.rs:175–300](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L175-L300)；[node_points_runtime.rs:325–381](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points_runtime.rs#L325-L381)；[reward_runtime_worker.rs:325–349](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L325-L349)；[node_points.rs:557–580](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L557-L580)；[reward_runtime_worker.rs:216–231](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L216-L231)；[reward_runtime_worker.rs:546–573](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L546-L573)；[node_points_runtime.rs:275–323](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points_runtime.rs#L275-L323)；[node_points_settlement_publication.rs:137–160](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L137-L160)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；runtime closure草案未完整吸收；不证明已结算资产安全恢复或生产custody。

<a id="ac-wr-economic-points-recovery"></a>
### AC-WR-ECONOMIC-POINTS-RECOVERY：epoch结算与恢复不重复入账的原验收

Trace refs: O7-REQ-54617172-fa9b-4efe-a058-b327ce348407；覆盖要求：[REQ-WR-ECONOMIC-POINTS-RECOVERY](#req-wr-economic-points-recovery)。

在[原专业条件与场景](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-recovery)适用的固定候选中逐项核验：同一 epoch/node 结算幂等；采样或快照写入不直接增加奖励，恢复、重复tick与replay只能复现结果；不可读collector状态须可观测降级，不是资产对账成功。 原验收成功/拒绝条件及层级不变，实际证据至少保留恢复前后ledger、settlement及降级记录；没有对应完整证据时保持未核实。runtime closure草案未完整吸收；不证明已结算资产安全恢复或生产custody。本验收不授权执行经济操作。

<a id="req-wr-economic-points-asset-gates"></a>
### REQ-WR-ECONOMIC-POINTS-ASSET-GATES：积分与独立资产路径保持门禁

Trace ID: O7-REQ-45b12bd9-9d07-4bbf-86bc-2a85af07d5f5

适用范围与成功标准：积分进入PowerCredit与主链Token bridge是两条独立受治理路径，不是自动兑换或同一池；多激励并用仍需独立审查重复奖励。 仅接收[既有专业义务](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-asset-gates)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[积分与独立资产路径保持门禁](#req-wr-economic-points-asset-gates)；验收沿[原专业接受关系](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-asset-gates)。
- 对应验收：[AC-WR-ECONOMIC-POINTS-ASSET-GATES](#ac-wr-economic-points-asset-gates)。
- 系统设计状态：未核实
- 系统设计依据：[积分与独立资产路径保持门禁有限设计](../../p2p/node/node-contribution-points.design.md#des-economic-points-asset-gates)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：生产settlement准备分别对PowerCredit使用reward_asset_config及签名mint记录，对主Token bridge使用独立epoch issuance的node_service预算、分配总额与重复epoch守卫，直接支持资产路径不同账本/门禁子范围；两路径完整治理授权、非自动兑换策略及多激励重复奖励的独立审批仍未核清。 源码/测试定位：[node_points.rs:175–300](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L175-L300)；[node_points_runtime.rs:325–381](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points_runtime.rs#L325-L381)；[reward_runtime_worker.rs:325–349](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L325-L349)；[node_points.rs:557–580](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L557-L580)；[node_points_settlement_publication.rs:27–60](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L27-L60)；[node_points_settlement_publication.rs:137–160](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L137-L160)；[node_points_settlement_publication.rs:227–279](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L227-L279)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不启用资产，不调整兑换比例或发行分桶，不证明兑付readiness。

<a id="ac-wr-economic-points-asset-gates"></a>
### AC-WR-ECONOMIC-POINTS-ASSET-GATES：积分与独立资产路径保持门禁的原验收

Trace refs: O7-REQ-45b12bd9-9d07-4bbf-86bc-2a85af07d5f5；覆盖要求：[REQ-WR-ECONOMIC-POINTS-ASSET-GATES](#req-wr-economic-points-asset-gates)。

在[原专业条件与场景](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-asset-gates)适用的固定候选中逐项核验：积分进入PowerCredit与主链Token bridge是两条独立受治理路径，不是自动兑换或同一池；多激励并用仍需独立审查重复奖励。 原验收成功/拒绝条件及层级不变，实际证据至少保留每条路径预算/授权/拒绝与独立审查记录；没有对应完整证据时保持未核实。不启用资产，不调整兑换比例或发行分桶，不证明兑付readiness。本验收不授权执行经济操作。

<a id="req-wr-economic-power-ledger"></a>
### REQ-WR-ECONOMIC-POWER-LEDGER：贡献形成受限协议资产账本

Trace ID: O7-REQ-c0f136a1-3baf-497e-a0ec-622e745024d0

适用范围与成功标准：PowerCredit保持不可直接转账的受限协议资产；按原积分换算和mint记录形成可追溯余额，不扩展通用交易市场。 仅接收[既有专业义务](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-ledger)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[贡献形成受限协议资产账本](#req-wr-economic-power-ledger)；验收沿[原专业接受关系](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-ledger)。
- 对应验收：[AC-WR-ECONOMIC-POWER-LEDGER](#ac-wr-economic-power-ledger)。
- 系统设计状态：未核实
- 系统设计依据：[贡献形成受限协议资产账本有限设计](../../p2p/node/node-redeemable-power-asset.design.md#des-economic-power-ledger)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：实际mint准备按source_awarded_points/points_per_credit限制credits，为各node生成power_credit_balance/total_minted及可追溯mint records后安装，直接覆盖积分换算到资产账本子范围；不可直接转账的全部入口排除、快照/replay完整账本核对尚未核清，不扩大市场或兑付承诺。 源码/测试定位：[node_points_settlement_publication.rs:27–95](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L27-L95)；[reward_asset.rs:129–253](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/reward_asset.rs#L129-L253)；[reward_asset_core.rs:246–315](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/reward_asset_core.rs#L246-L315)；[node_points_settlement_publication.rs:121–225](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L121-L225)；[node_points_settlement_publication.rs:371–394](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L371-L394)；[reward_runtime_settlement.rs:9–21](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_settlement.rs#L9-L21)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不更改兑换率，不新增转账/市场或当前可兑现承诺。

<a id="ac-wr-economic-power-ledger"></a>
### AC-WR-ECONOMIC-POWER-LEDGER：贡献形成受限协议资产账本的原验收

Trace refs: O7-REQ-c0f136a1-3baf-497e-a0ec-622e745024d0；覆盖要求：[REQ-WR-ECONOMIC-POWER-LEDGER](#req-wr-economic-power-ledger)。

在[原专业条件与场景](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-ledger)适用的固定候选中逐项核验：PowerCredit保持不可直接转账的受限协议资产；按原积分换算和mint记录形成可追溯余额，不扩展通用交易市场。 原验收成功/拒绝条件及层级不变，实际证据至少保留积分/credits换算与mint/balance/record对账；没有对应完整证据时保持未核实。不更改兑换率，不新增转账/市场或当前可兑现承诺。本验收不授权执行经济操作。

<a id="req-wr-economic-power-redemption"></a>
### REQ-WR-ECONOMIC-POWER-REDEMPTION：全部条件成立才燃烧资产并增加目标电力

Trace ID: O7-REQ-a061488d-5b14-489a-af77-ed3f6f75f1bb

适用范围与成功标准：兑换沿原余额、nonce、最小量、epoch储备与上限条件；成功须扣资产、扣储备与增加指定Agent电力一致，失败不得伪装兑付成功。 仅接收[既有专业义务](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-redemption)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[全部条件成立才燃烧资产并增加目标电力](#req-wr-economic-power-redemption)；验收沿[原专业接受关系](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-redemption)。
- 对应验收：[AC-WR-ECONOMIC-POWER-REDEMPTION](#ac-wr-economic-power-redemption)。
- 系统设计状态：未核实
- 系统设计依据：[全部条件成立才燃烧资产并增加目标电力有限设计](../../p2p/node/node-redeemable-power-asset.design.md#des-economic-power-redemption)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：实际worker签名并提交RedeemPowerSigned；PowerRedeemed准备层校验nonce、credit、reserve、epoch cap、目标存在与溢出后才生成扣credit/减reserve/加目标电力投影，直接覆盖兑换守卫和余额变化子义务；完整授权/治理组合、跨节点恢复与重放矩阵未核清。 源码/测试定位：[node_points_settlement_publication.rs:27–95](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L27-L95)；[reward_asset.rs:129–253](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/reward_asset.rs#L129-L253)；[reward_asset_core.rs:246–315](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/reward_asset_core.rs#L246-L315)；[reward_runtime_settlement.rs:23–109](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_settlement.rs#L23-L109)；[power_redemption_publication.rs:78–206](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/power_redemption_publication.rs#L78-L206)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不改额度、目标、权限或承诺当前服务可用。

<a id="ac-wr-economic-power-redemption"></a>
### AC-WR-ECONOMIC-POWER-REDEMPTION：全部条件成立才燃烧资产并增加目标电力的原验收

Trace refs: O7-REQ-a061488d-5b14-489a-af77-ed3f6f75f1bb；覆盖要求：[REQ-WR-ECONOMIC-POWER-REDEMPTION](#req-wr-economic-power-redemption)。

在[原专业条件与场景](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-redemption)适用的固定候选中逐项核验：兑换沿原余额、nonce、最小量、epoch储备与上限条件；成功须扣资产、扣储备与增加指定Agent电力一致，失败不得伪装兑付成功。 原验收成功/拒绝条件及层级不变，实际证据至少保留credits/reserve/electricity与拒绝原因；没有对应完整证据时保持未核实。不改额度、目标、权限或承诺当前服务可用。本验收不授权执行经济操作。

<a id="req-wr-economic-power-signature"></a>
### REQ-WR-ECONOMIC-POWER-SIGNATURE：签名治理条件不足必须拒绝

Trace ID: O7-REQ-85c5baeb-e05a-4924-9024-e05f1903bd2a

适用范围与成功标准：v1仅历史可重算摘要；v2与兑换签名依既有治理策略和绑定执行，缺少密钥、绑定或所需策略条件时fail closed。 仅接收[既有专业义务](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-signature)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[签名治理条件不足必须拒绝](#req-wr-economic-power-signature)；验收沿[原专业接受关系](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-signature)。
- 对应验收：[AC-WR-ECONOMIC-POWER-SIGNATURE](#ac-wr-economic-power-signature)。
- 系统设计状态：未核实
- 系统设计依据：[签名治理条件不足必须拒绝有限设计](../../p2p/node/node-redeemable-power-asset.design.md#des-economic-power-signature)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：mint生产准备层实际调用基于state的签名校验并拒绝invalid；worker绑定身份后签署并提交RedeemPowerSigned，直接覆盖部分签名守卫，不只是签名工具存在；完整governance/emergency/authority策略和恢复矩阵仍未核清。 源码/测试定位：[node_points_settlement_publication.rs:27–95](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L27-L95)；[reward_asset.rs:129–253](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/reward_asset.rs#L129-L253)；[reward_asset_core.rs:246–315](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/reward_asset_core.rs#L246-L315)；[node_points_settlement_publication.rs:143–160](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L143-L160)；[reward_runtime_settlement.rs:23–104](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_settlement.rs#L23-L104)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不接触密钥，不证明HSM/KMS或跨组织custody。

<a id="ac-wr-economic-power-signature"></a>
### AC-WR-ECONOMIC-POWER-SIGNATURE：签名治理条件不足必须拒绝的原验收

Trace refs: O7-REQ-85c5baeb-e05a-4924-9024-e05f1903bd2a；覆盖要求：[REQ-WR-ECONOMIC-POWER-SIGNATURE](#req-wr-economic-power-signature)。

在[原专业条件与场景](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-signature)适用的固定候选中逐项核验：v1仅历史可重算摘要；v2与兑换签名依既有治理策略和绑定执行，缺少密钥、绑定或所需策略条件时fail closed。 原验收成功/拒绝条件及层级不变，实际证据至少保留版本化载荷、绑定/策略与拒绝结果；没有对应完整证据时保持未核实。不接触密钥，不证明HSM/KMS或跨组织custody。本验收不授权执行经济操作。

<a id="req-wr-economic-power-audit"></a>
### REQ-WR-ECONOMIC-POWER-AUDIT：审计与恢复检测异常而不自动修复

Trace ID: O7-REQ-9804d9d7-2c75-475f-b321-5c4ed36276b5

适用范围与成功标准：mint/burn/balance/record与快照/replay保持一致；invariant report用于检测而非自动修复，旧快照可读取不免除签名策略与报告复核。 仅接收[既有专业义务](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-audit)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[审计与恢复检测异常而不自动修复](#req-wr-economic-power-audit)；验收沿[原专业接受关系](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-audit)。
- 对应验收：[AC-WR-ECONOMIC-POWER-AUDIT](#ac-wr-economic-power-audit)。
- 系统设计状态：未核实
- 系统设计依据：[审计与恢复检测异常而不自动修复有限设计](../../p2p/node/node-redeemable-power-asset.design.md#des-economic-power-audit)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：公开只读invariant report检查node/global的mint-burn-balance关系及mint签名，仅累积violations而不改state；真实reward worker调用report.is_ok更新metrics，直接覆盖异常检测不自动修复子义务；旧快照签名策略、跨节点/replay恢复一致性与人工报告复核仍未核清。 源码/测试定位：[node_points_settlement_publication.rs:27–95](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L27-L95)；[reward_asset.rs:129–253](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/reward_asset.rs#L129-L253)；[reward_asset_core.rs:246–315](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/reward_asset_core.rs#L246-L315)；[resources.rs:352–436](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/resources.rs#L352-L436)；[reward_runtime_worker.rs:447–461](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L447-L461)；[reward_asset_core.rs:316–393](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/reward_asset_core.rs#L316-L393)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不证明mainnet或生产恢复，不恢复历史viewer flags入口。

<a id="ac-wr-economic-power-audit"></a>
### AC-WR-ECONOMIC-POWER-AUDIT：审计与恢复检测异常而不自动修复的原验收

Trace refs: O7-REQ-9804d9d7-2c75-475f-b321-5c4ed36276b5；覆盖要求：[REQ-WR-ECONOMIC-POWER-AUDIT](#req-wr-economic-power-audit)。

在[原专业条件与场景](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-audit)适用的固定候选中逐项核验：mint/burn/balance/record与快照/replay保持一致；invariant report用于检测而非自动修复，旧快照可读取不免除签名策略与报告复核。 原验收成功/拒绝条件及层级不变，实际证据至少保留快照前后状态、invariant report与策略复核；没有对应完整证据时保持未核实。不证明mainnet或生产恢复，不恢复历史viewer flags入口。本验收不授权执行经济操作。

<a id="req-wr-economic-genesis-allocation"></a>
### REQ-WR-ECONOMIC-GENESIS-ALLOCATION：冻结分配可核算到各bucket与recipient

Trace ID: O7-REQ-585609e7-ee93-4315-b9c7-fe58857164b4

适用范围与成功标准：按既有冻结总量和七桶参数核算分配及recipient vested_balance；采用原runtime rounding，创世custody account不等于treasury bucket。 仅接收[既有专业义务](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-allocation)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[冻结分配可核算到各bucket与recipient](#req-wr-economic-genesis-allocation)；验收沿[原专业接受关系](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-allocation)。
- 对应验收：[AC-WR-ECONOMIC-GENESIS-ALLOCATION](#ac-wr-economic-genesis-allocation)。
- 系统设计状态：部分覆盖
- 系统设计依据：[冻结分配可核算到各bucket与recipient有限设计](../../p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-allocation)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。 本轮补核：专业设计§3.1/3.2A/3.4明确七桶、绝对额、custody/treasury区别及runtime rounding；真实地址/最终QA仍为外部前提。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 有界源码/记录续核：生产genesis把输入plan按floor+ratio降序/bucket升序余数分配，并聚合recipient vested；circulating/liquid初始为0。 InitializeMainTokenGenesis→build allocations→PreparedMainTokenMonetaryEvent→genesis buckets/recipient balances。 真实recipient/controller绑定仍pending；实现校验任意传入plan总bps=10000，不证明冻结七桶内容被强制或正式mint获准。 已非仅fixture：真实rounding、总额守恒、重复初始化拒绝与custody非treasury路径直接可达。 证据：[main_token.rs:144–289](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/main_token.rs#L144-L289)；[main_token_monetary_publication.rs:105–218](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/main_token_monetary_publication.rs#L105-L218)；[mainchain-token-genesis-freeze-sheet.md:17–60](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#L17-L60)；[mainchain-token-genesis-and-contributor-reward.design.md:22–107](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#L22-L107)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：冻结七桶输入的正式选定/约束、真实账户绑定、同候选QA与实际执行仍缺；不执行交易、不以任意合法plan等同正式分配。

<a id="ac-wr-economic-genesis-allocation"></a>
### AC-WR-ECONOMIC-GENESIS-ALLOCATION：冻结分配可核算到各bucket与recipient的原验收

Trace refs: O7-REQ-585609e7-ee93-4315-b9c7-fe58857164b4；覆盖要求：[REQ-WR-ECONOMIC-GENESIS-ALLOCATION](#req-wr-economic-genesis-allocation)。

在[原专业条件与场景](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-allocation)适用的固定候选中逐项核验：按既有冻结总量和七桶参数核算分配及recipient vested_balance；采用原runtime rounding，创世custody account不等于treasury bucket。 原验收成功/拒绝条件及层级不变，实际证据至少保留固定参数版本、七桶金额与rounding对账；没有对应完整证据时保持未核实。不变更总量/比例/账户，不执行InitializeMainTokenGenesis。本验收不授权执行经济操作。

<a id="req-wr-economic-genesis-control"></a>
### REQ-WR-ECONOMIC-GENESIS-CONTROL：控制主体与个人受益及释放边界分别可审计

Trace ID: O7-REQ-2ee6964f-7c70-453c-958d-616b07c5d9d9

适用范围与成功标准：项目战略控制、协议储备、个人直接受益与外部流通分别核算；原vesting、claim、genesis_liquid与首年释放门槛继续独立约束。 仅接收[既有专业义务](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-control)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[控制主体与个人受益及释放边界分别可审计](#req-wr-economic-genesis-control)；验收沿[原专业接受关系](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-control)。
- 对应验收：[AC-WR-ECONOMIC-GENESIS-CONTROL](#ac-wr-economic-genesis-control)。
- 系统设计状态：部分覆盖
- 系统设计依据：[控制主体与个人受益及释放边界分别可审计有限设计](../../p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-control)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。 本轮补核：专业设计§3.2/3.2A分开战略控制、协议储备、个人受益及首年释放；有限合同明确，真实控制映射与审计执行未闭合。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 有界源码/记录续核：genesis liquid=0；claim实际校验recipient、epoch cliff/linear unlock与releasable，再准备vesting事件。 ClaimMainTokenVesting→main_token_bucket_unlocked_amount→PreparedMainTokenMonetaryEvent。 当前bucket recipient与nonce约束独立于真实个人受益/控制主体审计。 直接支持原vesting/claim/genesis_liquid子范围，不是仅测试fixture。 证据：[main_token.rs:314–330](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/main_token.rs#L314-L330)；[main_token.rs:325–380](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/main_token.rs#L325-L380)；[main_token_monetary_publication.rs:188–218](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/main_token_monetary_publication.rs#L188-L218)；[mainchain-token-genesis-freeze-sheet.md:49–68](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#L49-L68)；[mainchain-token-genesis-and-contributor-reward.design.md:22–107](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#L22-L107)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：真实控制主体、个人受益拆分、非团队首年释放总门槛及正式controller资格未核实；协议储备不作团队库存。

<a id="ac-wr-economic-genesis-control"></a>
### AC-WR-ECONOMIC-GENESIS-CONTROL：控制主体与个人受益及释放边界分别可审计的原验收

Trace refs: O7-REQ-2ee6964f-7c70-453c-958d-616b07c5d9d9；覆盖要求：[REQ-WR-ECONOMIC-GENESIS-CONTROL](#req-wr-economic-genesis-control)。

在[原专业条件与场景](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-control)适用的固定候选中逐项核验：项目战略控制、协议储备、个人直接受益与外部流通分别核算；原vesting、claim、genesis_liquid与首年释放门槛继续独立约束。 原验收成功/拒绝条件及层级不变，实际证据至少保留真实控制映射、受益拆分、释放计划与阻断依据；没有对应完整证据时保持未核实。不改控制人/vesting/释放额度，不把协议储备当团队库存。本验收不授权执行经济操作。

<a id="req-wr-economic-genesis-freeze"></a>
### REQ-WR-ECONOMIC-GENESIS-FREEZE：逻辑冻结不等于真实mint-ready

Trace ID: O7-REQ-6ca62d73-1934-4a80-bb18-1c21e35e2a15

适用范围与成功标准：slot逻辑冻结与public signer规则不代替真实recipient/controller绑定、个人受益表和最终QA pass；pending或conditional状态不授予mint许可。 仅接收[既有专业义务](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-freeze)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[逻辑冻结不等于真实mint-ready](#req-wr-economic-genesis-freeze)；验收沿[原专业接受关系](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-freeze)。
- 对应验收：[AC-WR-ECONOMIC-GENESIS-FREEZE](#ac-wr-economic-genesis-freeze)。
- 系统设计状态：部分覆盖
- 系统设计依据：[逻辑冻结不等于真实mint-ready有限设计](../../p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-freeze)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。 本轮补核：专业设计§3.4和formal sheet gates把逻辑冻结、实际绑定和QA-before-mint分开；不据存在清单宣布执行门禁已验证。
- 实现状态：未核实
- 实现与验证依据：2026-10-17 有界源码/记录续核：固定0ec正式freeze sheet确有slot/public signer/金额规则，仍logic_frozen_supply_set_address_binding_pending。 pre-mint sheet→QA gate合同；未核到真实binding ceremony与本次QA pass。 pending/conditional不是mint许可；不从逻辑冻结或公开签名集合推导执行授权。 真实recipient和governance accounts仍TBD；该记录不是纯模板不存在，也不是已准备好mint。 证据：[mainchain-token-genesis-freeze-sheet.md:1–29](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#L1-L29)；[mainchain-token-genesis-freeze-sheet.md:49–68](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#L49-L68)；[mainchain-token-genesis-and-contributor-reward.design.md:22–107](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#L22-L107)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：具体ceremony/绑定任务及最终QA verdict待核；不改变signer set、绑定或执行经济操作。

<a id="ac-wr-economic-genesis-freeze"></a>
### AC-WR-ECONOMIC-GENESIS-FREEZE：逻辑冻结不等于真实mint-ready的原验收

Trace refs: O7-REQ-6ca62d73-1934-4a80-bb18-1c21e35e2a15；覆盖要求：[REQ-WR-ECONOMIC-GENESIS-FREEZE](#req-wr-economic-genesis-freeze)。

在[原专业条件与场景](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-freeze)适用的固定候选中逐项核验：slot逻辑冻结与public signer规则不代替真实recipient/controller绑定、个人受益表和最终QA pass；pending或conditional状态不授予mint许可。 原验收成功/拒绝条件及层级不变，实际证据至少保留绑定证据、完整pre-mint清单与该次QA verdict；没有对应完整证据时保持未核实。本轮不绑定地址/账户、不运行ceremony、不改变signer set或执行交易。本验收不授权执行经济操作。

<a id="req-wr-economic-genesis-reward"></a>
### REQ-WR-ECONOMIC-GENESIS-REWARD：独立reserve只消费最终复核的批准记录

Trace ID: O7-REQ-e84d3e5c-af7c-4293-8d02-3b466af6dbfa

适用范围与成功标准：独立reward reserve只按canonical README ledger中approved且完成distribution前actual-value review的最终金额执行；资格/评分仍归pack/ledger，不并ecosystem_pool，不按登录或游玩自动发放。 仅接收[既有专业义务](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-reward)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[独立reserve只消费最终复核的批准记录](#req-wr-economic-genesis-reward)；验收沿[原专业接受关系](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-reward)。
- 对应验收：[AC-WR-ECONOMIC-GENESIS-REWARD](#ac-wr-economic-genesis-reward)。
- 系统设计状态：部分覆盖
- 系统设计依据：[独立reserve只消费最终复核的批准记录有限设计](../../p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-reward)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。 本轮补核：专业设计§3.3明确独立reserve与producer approval，正式round记录体现最终复核义务；reserve执行和closure仍未证明。
- 实现状态：未核实
- 实现与验证依据：2026-10-17 有界源码/记录续核：固定0ec真实round ledger已有LTRL-PR-60/59经过actual-value review批准100/50 OC，非只有空模板。 round ledger→distribution closure记录；执行owner/ref/date仍空，pending_distribution。 批准记录是历史业务事实，不授权本轮发放；独立reserve不能被ecosystem或node reward替代。 已核清批准行与最终金额，但尚无独立reserve实际消费/发放结果。 证据：[readme-limited-preview-contributor-reward-ledger-round-2026-04-13.md:12–24](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/doc/readme/governance/readme-limited-preview-contributor-reward-ledger-round-2026-04-13.md#L12-L24)；[readme-limited-preview-contributor-reward-ledger-round-2026-04-13.md:35–53](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/doc/readme/governance/readme-limited-preview-contributor-reward-ledger-round-2026-04-13.md#L35-L53)；[readme-limited-preview-contributor-reward-distribution-closure-round-2026-04-13.md:1–30](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/doc/readme/governance/readme-limited-preview-contributor-reward-distribution-closure-round-2026-04-13.md#L1-L30)；[mainchain-token-genesis-and-contributor-reward.design.md:22–107](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#L22-L107)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：需取得授权执行owner和不可变distribution ref/date并闭合原ledger；不由approved或通用token发行机制升格为已发放。

<a id="ac-wr-economic-genesis-reward"></a>
### AC-WR-ECONOMIC-GENESIS-REWARD：独立reserve只消费最终复核的批准记录的原验收

Trace refs: O7-REQ-e84d3e5c-af7c-4293-8d02-3b466af6dbfa；覆盖要求：[REQ-WR-ECONOMIC-GENESIS-REWARD](#req-wr-economic-genesis-reward)。

在[原专业条件与场景](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-reward)适用的固定候选中逐项核验：独立reward reserve只按canonical README ledger中approved且完成distribution前actual-value review的最终金额执行；资格/评分仍归pack/ledger，不并ecosystem_pool，不按登录或游玩自动发放。 原验收成功/拒绝条件及层级不变，实际证据至少保留canonical批准行、最终金额复核、审批及distribution引用；没有对应完整证据时保持未核实。本轮无发放授权，不修改资格/金额/审批；模板不是已批准批次。本验收不授权执行经济操作。


## 9. 既有补充决策切片的身份与范围

以下只为[既有 source/deposit 合同](#source-deposit-product-contract)的两个独立消费者义务提供身份和精确导航，不新增产品政策或改变原条款；其他 source、allocation、材料 handoff 和经济义务保留原归属。

<a id="source-assessment-replenishment-truth"></a>
### 功能状态卡：补充评估的能力与数量时间事实

Trace ID: O7-REQ-ff33b61a-c81c-46f8-8e34-928af72858ef

- 主责范围：仅原 source/deposit 条款中：当前专业 profile 真正支持的补充选择、未知/过期与已知零的区分、时刻/数量不可证明时显示未知与复查条件，不伪造精确倒计时或保证。
- 适用 profile：仅实际声明相关 source/补充能力的路径；不支持补充时保留真实禁用反馈，能力/来源未知时保留不可用。不能要求 production_only 额外经历采集/精炼。
- 系统设计依据：[`chunked fragment generation`](../../world-simulator/scenario/chunked-fragment-generation.design.md#des-frfr-capability)；该设计仅接收当前 chunked/profile 的有限切片。
- 专业需求接收：[REQ-FRFR-1](../../world-simulator/scenario/chunked-fragment-generation.prd.md#req-frfr-1) 至 [REQ-FRFR-4](../../world-simulator/scenario/chunked-fragment-generation.prd.md#req-frfr-4)；能力与来源、下一机会时间、下一机会目标量。 具体后端、周期、预算与事件算法仍归原专业 owner，不是产品新增算法。
- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 实现与验证依据：2026-10-17 固定候选静态复核：已确认kernel能力/数量/未来tick计算及live disabled/unavailable入口；其他profile与真实网络读面、当前用例执行仍缺。 源码/测试定位：[fragment_refill_quote.rs:11–107](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/fragment_refill_quote.rs#L11-L107)；[fragment_refill_preview.rs:50–119](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/fragment_refill_preview.rs#L50-L119)；[fragment_replenishment_quote.rs:66–132](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/tests/fragment_replenishment_quote.rs#L66-L132)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 剩余范围：其他 source profile、真实网络/玩家端到端及未提供权威投影的 live 路径均未由本文闭合。

<a id="source-replenishment-effect-recheck"></a>
### 功能状态卡：只读评估与权威补充后的复查

Trace ID: O7-REQ-dcbc5b97-bbfb-4345-b411-926bd9dff3d9

- 主责范围：仅原 source/deposit 条款中：预览/扫描/推荐/刷新无世界效果，只有权威 replenishment effect 后 fresh assessment 才见新增可分配量，补充单次生效，等待不续期或承诺 ETA。
- 适用 profile：仅实际声明相关 source/补充能力的路径；不支持补充时保留真实禁用反馈，能力/来源未知时保留不可用。不能要求 production_only 额外经历采集/精炼。
- 系统设计依据：[`chunked fragment generation`](../../world-simulator/scenario/chunked-fragment-generation.design.md#des-frfr-readonly)；该设计仅接收当前 chunked/profile 的有限切片。
- 专业需求接收：[REQ-FRFR-5](../../world-simulator/scenario/chunked-fragment-generation.prd.md#req-frfr-5) 与运行期补种落账；按实际后端、当前状态、事件和快照复查。 具体后端、周期、预算与事件算法仍归原专业 owner，不是产品新增算法。
- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 实现与验证依据：2026-10-17 固定候选静态复核：已确认&self只读评估及kernel正常补种entries路径；异常批次原子性、真实live补种/网络重连与复查全链仍缺。 源码/测试定位：[fragment_replenish.rs:13–179](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/fragment_replenish.rs#L13-L179)；[fragment_refill_quote.rs:11–107](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/fragment_refill_quote.rs#L11-L107)；[fragment_replenishment_quote.rs:1–50](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/tests/fragment_replenishment_quote.rs#L1-L50)；[step.rs:89–99](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/step.rs#L89-L99)；[fragment_replenishment_quote.rs:134–165](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/tests/fragment_replenishment_quote.rs#L134-L165)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 剩余范围：其他 source profile、真实网络/玩家端到端及未提供权威投影的 live 路径均未由本文闭合。

<a id="generation-physical-truth"></a>
### 功能状态卡：生成场景的可测量物理真值

Trace ID: O7-REQ-b4be4414-7def-4831-8302-f9523e82f944

- 主责范围：仅原文“一套可测量的物理真值”及确定、可追溯且不被表现抽象改写的映射；本切片适用于既有 chunk/fragment 场景的物理与资源量事实。 正证为[原产品条款](#measurable-physical-truth-contract)，本卡仅接收既有身份，不新增数值、算法、类型或正式 profile 选择。
- 专业边界：已采用的物理公式、化学映射、中心取整不跨账及 legacy Data 净前后余额分别沿 chunked 原专业条款；这些不是本产品卡新推导的规则，也不映射到 M4 材料类型。
- 系统设计状态：部分覆盖
- 系统设计依据：[既有专业设计切片](../../world-simulator/scenario/chunked-fragment-generation.design.md#chunk-generation-existing-obligations)，仅对应本卡有限义务，不代表全产品或所有 source profile 覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 有界源码/记录续核：生成代码从fragment blocks的几何/密度计算mass，质量/元素映射到fragment预算再累计chunk预算。 generate_chunk_fragments→synthesize/truncate profile→synthesize budget→location写入；ChunkGenerated replay重算并比较。 仅现有Simulator chunk/fragment范围；不选择正式world/chain resource profile。 生产调用链已定位，不能再仅记物理fixture；截断后重算profile并据此生成budget。 证据：[fragment_physics.rs:140–163](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_simulator/src/fragment_physics.rs#L140-L163)；[fragment_physics.rs:239–263](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_simulator/src/fragment_physics.rs#L239-L263)；[init.rs:681–718](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_simulator/src/init.rs#L681-L718)；[replay.rs:429–484](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/replay.rs#L429-L484)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：中心边界归属、表现层全链/全部参数组合与实际运行仍缺；不把本地生成当formal bootstrap。

<a id="source-location-quantity-truth"></a>
### 功能状态卡：来源作用域与数量的同一权威事实

Trace ID: O7-REQ-218aeae3-6a52-423a-9b69-9570975edc3c

- 主责范围：仅原 source 评估中身份、作用域、预计可满足量及权威/估计边界，以及 chunk/fragment 生成与预算由既有专题拥有的切片；消费与表现不得把另一来源/作用域的数量冒充当前事实。 正证为[原产品条款](#source-deposit-product-contract)，本卡仅接收既有身份，不新增数值、算法、类型或正式 profile 选择。
- 专业边界：已采用的物理公式、化学映射、中心取整不跨账及 legacy Data 净前后余额分别沿 chunked 原专业条款；这些不是本产品卡新推导的规则，也不映射到 M4 材料类型。
- 系统设计状态：部分覆盖
- 系统设计依据：[既有专业设计切片](../../world-simulator/scenario/chunked-fragment-generation.design.md#fragment-center-budget-design)，仅对应本卡有限义务，不代表全产品或所有 source profile 覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 有界源码/记录续核：已接受fragment具有coord派生id、实体profile/budget；相同budget进入生成coord的chunk账本。 generate_chunk_fragments location插入与chunk累计→replay对应summary比较。 数量事实来自原生成profile和账本，不凭Viewer重算；只限定该Simulator分支。 直接来源/数量绑定存在；asteroid_fragment先采样后round中心，init仍记原coord，边界跨账防护尚未证实。 证据：[init.rs:648–718](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_simulator/src/init.rs#L648-L718)；[asteroid_fragment.rs:150–163](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_simulator/src/asteroid_fragment.rs#L150-L163)；[replay.rs:429–484](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/replay.rs#L429-L484)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：face/edge/corner及max−0.5cm round至上界的归属/预算一致须核；目前是静态疑点非已运行失败，其他source/handoff/profile不代签。

<a id="source-current-goal-impact"></a>
### 功能状态卡：来源准备的成本与当前目标影响

Trace ID: O7-REQ-1c30e45e-99f4-4353-8f80-380f23243cbe

- 主责范围：仅原 source 评估中时间/资源/物流成本及对当前配方/首个工业目标的影响可读；本切片接收当前专业 profile 的真实目标影响，不以正产出自动冒充目标推进。 正证为[原产品条款](#source-deposit-product-contract)，本卡仅接收既有身份，不新增数值、算法、类型或正式 profile 选择。
- 专业边界：已采用的物理公式、化学映射、中心取整不跨账及 legacy Data 净前后余额分别沿 chunked 原专业条款；这些不是本产品卡新推导的规则，也不映射到 M4 材料类型。
- 系统设计状态：部分覆盖
- 系统设计依据：[既有专业设计切片](../../world-simulator/scenario/chunked-fragment-generation.design.md#legacy-simulator-refining-design)，仅对应本卡有限义务，不代表全产品或所有 source profile 覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 有界源码/记录续核：legacy refine实际扣D中的m、扣电力c、再向D加h；quote可读电力成本与余额，但before目标量使用D−m。 quote_refine_compound→Viewer refine preflight；Action::RefineCompound与CompoundRefined replay→D−m+h。 仅legacy Data账本，不映射M4材料；正产出h不自动表示净目标推进。 成本/余额子范围直接实现；目标影响before字段偏离完整D before，无法由h>0或shortfall_after减少宣称当前目标推进。 证据：[actions_refine_quote.rs:25–94](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/actions_refine_quote.rs#L25-L94)；[actions_core.rs:808–885](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_simulator/src/kernel/actions_core.rs#L808-L885)；[replay.rs:285–309](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/replay.rs#L285-L309)；[player_gameplay.rs:409–435](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/player_gameplay.rs#L409-L435)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：应核完整D before/after与阈值、运行样本和所有消费者；本次只记录已定位静态偏差，不实现修复、不以全未知掩盖差异。 既有目标净变化问题继续回链 [#4504](https://github.com/nil-studio/oasis7/issues/4504)，不重复建题或自动关闭。
