# 普通共同决策与宪制边界

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[prd.md](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-09-20
- 专业域权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)

本文定义普通共同治理可处理的有限事项，以及它与玩家基本保护、系统安全权威和宪制修订之间不可绕过的产品边界。本文直接接收 GG-2 的治理主体/控制权公平与 GG-3 的外部 OC、游戏内治理权分离语义，建立稳定的产品 REQ/AC；§6.3 保留历史 GG-1..5 至当前接收锚点与组合验收的 crosswalk；它不定义资格参数、权重、阈值、锁定时长、身份技术、隐私机制、链上结构、runtime/P2P 状态机或当前可用性结论。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 只定义普通治理、宪制轨道、拒绝和可读结果的制度边界；投票与流程交互仍由专业 authority 决定。
- 历史设计判定记录：本次分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，本次闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
本节的产品体验设计豁免不等于专业系统设计不适用；各要求的技术覆盖与实现证据分别按下列状态摘要核对。

## 1. 产品目标

玩家和组织可以在同一个持续、可审计的世界中共同决定有限的运行事项，而不能把持有、付费、组织章程、普通投票、局部多数或历史声望扩展为无界世界主权。

普通治理服务于已明确授权的政策、公共财库和既有 charter 日常运行。玩家基本保护、安全/finality、signer/custody 与普通治理自身权限边界不属于普通运行事项；需要改变其他宪制规则时，必须进入独立、可理解且可申诉的宪制轨道。

## 2. 普通治理事项白名单

普通治理只能处理规则明确列举且已具备授权来源的以下事项类别：

- **政策运行**：在既有世界规则和权利边界内，选择已声明公共政策的方向、优先级或适用方案。
- **公共财库**：决定已授权公共项目的预算用途、拨付方向或持续/终止，但不能借财库决定取得对玩家独立资产、系统安全或世界最终性的控制权。
- **既有 charter 日常运行**：在 charter 已授予且不越过个人保护底线的范围内，调整角色、服务安排、内部工作流或日常运行规则。

事项必须在提交时声明类别、目标、作用范围和授权来源。没有匹配白名单的事项不得以“其他”“紧急”“技术升级”或复合提案静默进入普通治理；系统只能原子拒绝，或明确路由到具备独立授权与验收的非普通轨道。拒绝与路由均不得产生部分世界效果、资金占用、资格变更或事实上的先行实施。

普通治理不能自行扩展白名单或改变本节的分类规则。未来新增事项类别必须先通过适用的宪制流程，并保持本页的保护底线。

### 2.1 治理主体资格与控制权公平

普通治理必须以一个经验证的游戏账户作为治理主体，聚合该账户已批准的玩家身份、managed signer 与 external wallet bindings；这些绑定只用于确定同一主体的资格、委托与计票范围，不把 signer、custody、密钥或钱包控制权变成普通治理权限。普通玩家可以保持假名；只有在高影响资格需要防止重复控制或规避上限时，才允许消费私下最小化的控制关系断言，公开面不得泄露非必要个人数据。

资格的产品边界由线性锁定 OC、明确范围内可撤销的委托与按实际控制人聚合后的影响上限共同构成。账户拆分、关联组织、短期循环转移或重复绑定不得制造额外普通治理影响；区域 charter 的本地双合法性仍需锁定 OC/委托/控制人上限与连续本地贡献共同成立，不由全局治理替代区域或受影响主体的有界权利。

重复治理主体的 challenge 必须提供可理解的 review，以及按事实适用的 merge 或 revocation、计票 correction 与 appeal。纠正不得抹去原授权、投票、处理或 receipt 历史；公开结果应同时保留纠错理由、最终有效结果和可复核的因果链。本文冻结的是玩家可理解的资格与公平边界，不冻结阈值、身份技术、隐私证明、投票字段或控制人计算实现。

### 2.2 外部 OC 与游戏内治理权分离

OC 作为外部链上资产保持可自由转让；持有、转入或转出本身不自动取得、出售或保留游戏内普通治理、高影响资格、区域权利、世界资产或行动结果。游戏内治理权只来自特定事项的可审计锁定、snapshot 与明确退出/解除绑定过程；进入有效窗口后，外部转让、反向转移或解除绑定不得追溯改票、重复投票或让同一权利同时属于两个控制关系。

既有 operator-managed 单向 `OC -> LetAI Run quota` 服务额度桥继续保持为非治理、非赎回路径；额度、`token_key` 或服务消费不得影响任何治理资格、权重、投票、委托或世界权利。本文只规定两侧不可互推的产品边界，不宣称链上交易、智能合约、桥接或游戏权利绑定已实现。

## 3. 不可由普通治理改写的保护

以下保护不属于普通治理权限，普通提案、公共财库、组织 charter、技术升级、局部多数、历史声望或紧急状态均不能直接或间接改写：

- 玩家对独立资产、已有合同、可理解退出和适用救济的最低保护；
- 玩家与 Agent 的身份来源、历史连续性，以及已确认世界结果和 receipt 的因果连续性；
- 审计、程序性申诉和对授权范围、理由与结果的复核能力；
- validator/finality、signer、custody、密钥材料、网络最终性及其他系统级安全权威；
- 普通治理事项白名单、权限边界和本节保护本身。

涉及这些保护的普通提案必须原子拒绝。表现层、Agent 或自动化不能通过拆分提案、重命名事项、先执行后追认或把安全操作描述为日常运行来规避分类。

## 4. 其他宪制变更的独立轨道

不触及上述不可修改保护、但会改变其他宪制规则的事项，必须进入独立于普通运行治理的宪制轨道。生效前至少同时满足：

- 公开审议与玩家可理解的影响说明，包括受影响主体、权利、资源和恢复边界；
- 充分延迟与复核窗口，使受影响主体能检查、反对、退出适用安排或准备恢复；
- 适用的超多数，以及跨区域或受影响主体的独立确认；
- 独立审计与程序性申诉入口；
- 对授权、理由、范围、确认、异议、生效点和结果的连续 receipt。

任一条件缺失时，变更不得生效，也不得产生部分世界效果。具体阈值、身份、时钟、投票结构与执行状态由专业域定义；产品层只冻结“条件必须全部满足、且不能由同一普通治理决定自行代签”的边界。

## 5. 紧急 containment 与防绕过

紧急授权只可在其专业合同声明的范围和期限内限制风险扩散、维持基本连续性或保护待决事实。它不能替代宪制修订，不能扩大普通治理白名单，也不能暂停不可修改保护、审计或申诉。

财库拨付、charter 变更、技术升级、复合提案、局部多数、历史声望和先行实现均不是宪制轨道的替代品。任何路径只要实质改变受保护边界或其他宪制规则，就必须按第 3、4 节重新分类；分类、拒绝、路由和最终结果均保留可复核因果。

这使规则保持 world-first：世界效果只来自当前有效的权威授权；保持 emergence-first：共同决策可以演化，但不能产生无界主权或权力套利；保持 persistent / auditable：历史、授权、异议与结果不因治理变化被重写；保持 extensible：未来可增加事项与宪制机制，但不能削弱保护底线或绕过分类。

## 6. 玩家可读结果

提交共同决策事项时，玩家至少能够区分：

- **普通事项可受理**：显示事项类别、目标、范围、授权来源和后续决策点；受理不等于结果已经生效。
- **不属于普通治理**：显示被排除的边界，以及原子拒绝或适用的非普通轨道路由；不得伪装为权限、资源或实现错误。
- **宪制事项待满足条件**：显示尚缺的审议、延迟、确认、审计或申诉条件；待决不产生部分宪制效果。
- **已生效或未通过**：以权威 receipt 说明授权、理由、范围、生效结果与可用复核/申诉；不能把提案、投票、排队或 UI 缓存表示成已生效世界事实。

本文不冻结文案、布局或字段；正式玩家 surface 的表达由相应专业 authority 拥有，但不得隐藏分类、保护边界或下一步。

### 6.1 已通过普通事项的执行闸门、失效与恢复

普通事项的**通过**只证明它在当时的白名单、授权来源和决策程序下取得了有限的执行资格；它不是已经生效的世界结果，也不是对未来资源、角色、charter 范围或权限的预留。只有一个在执行时仍满足当前授权、作用范围、前置条件、反滥用检查和专业合同的权威动作，才可以把该事项的允许部分生效并产生 receipt。执行闸门必须按以下优先级处理：

1. **保护与授权优先：** 若事项实质触及第 3 节保护、已被重新分类为宪制事项，或其原有授权/charter 已撤销、到期、收缩或被替代，则不得执行；不得以既有通过结果、排队位置、自动化计划或“先执行后复核”绕过当前边界。
2. **当前范围与前置条件优先：** 仍可执行的事项只能作用于其通过时明确的目标、范围和允许效果，且必须重新校验当前资源、资格、对象状态与适用专业条件。范围已变化或前置条件不足时，系统只能原子拒绝、明确过期，或要求在当前有效轨道重新提交；不得静默扩大范围、沿用旧条件、制造隐藏债务或部分拨付/角色变更。
3. **单次世界效果优先：** 同一已通过事项的并发执行、重连后的自动重试、Agent 代办或历史 receipt 重放，至多产生一个权威世界效果。第一个有效 receipt 确认后，其余请求必须被去重、拒绝、过期或按专业合同保持待决，不能取得第二次资金、权限、排队优先或其他世界效果。

当执行被阻断或失效时，玩家可以查看原事项、导致失效的当前边界、是否已有已确认结果，以及适用的重新提交、等待、申诉或常态替代路径；不能把“已通过”“正在执行”或本地缓存表示成已拨付、已改权或已生效。已确认的结果及其理由、范围和 receipt 保留为历史，但不因后续重试而再次执行，也不授权超出原事项的补充效果。

这项执行闸门保持 **world-first**：世界效果只来自执行时仍有效的权威条件；保持 **emergence-first**：共同决策可产生行动机会，但不能冻结世界或制造治理套利；保持 **persistent / auditable**：通过、失效、拒绝与唯一生效结果之间的因果连续可复核；保持 **extensible**：未来可增加普通事项类别或执行机制，但仍须保留保护优先、当前再校验和单次世界效果。

### 6.2 可验证的治理边界要求

<a id="req-wr-gcb-001"></a>
### REQ-WR-GCB-001：普通治理只处理白名单事项

Trace ID: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b

- 系统设计状态：部分覆盖
- 实现状态：未实现
- 系统设计依据：[gameplay-war-politics-mvp-baseline.design / des-war-politics-002](../../game/gameplay/gameplay-war-politics-mvp-baseline.design.md#des-war-politics-002)。已承接：普通治理白名单内的参数与提案/投票预览。
- 实现与验证依据：当前普通治理入口未实现事项分类白名单合同：自由文本提案仍直接进入普通票局。既有字段检查和只读预览不是本条类别/目标/范围/授权及保护边界分类的交付。 [完整OpenGovernanceProposal arm仅核Agent、唯一key、非空文本、窗口/阈值范围、至少两个选项](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_gameplay.rs#L913-L1035)；[普通治理输入是proposer、proposal key、自由title/description、options、窗口与票权。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_agent_api/src/world_action/action.rs#L387-L402)；[只读投票quote计算当前/投后票数与建议](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/governance_quote.rs#L33-L148)；[WorldEventBody穷尽分派Domain/Governance/CapabilityAuthorization；模块状态写bytes](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing.rs#L801-L900)；[DomainEvent穷尽dispatcher将货币、工业、治理、Alliance/contract各自路由具型reducer。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L1008-L1140)；[完整治理程序消费三种票局事件](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_builtin_wasm_modules/m5_gameplay_governance_council/src/lib.rs#L13-L227)；[active Gameplay Tick emit仅反序列化五类具型directive](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/gameplay_loop.rs#L25-L254)。闭合范围同时核对typed dispatcher、当前M5 bootstrap、builtin council及具型emit consumer；不涵盖任意未提供的外部部署/WASM。测试源码不是运行记录；本轮未取得本条完整组合验收通过证据。
- 核对日期与基线：2026-10-17；需求卡为PR #4844 head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；源码固定当前main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`，所引源已逐Git blob复算匹配完整tree；仅静态审读，本轮未运行业务测试、模型、世界或正式端到端验收。
- 剩余范围：实现普通事项声明、白名单/保护边界分类与普通/财库/charter/紧急/升级/复合路由；执行前重验并原子拒绝越界且不占用资金/权限，提供缺项和下一步。 实施待安排： [Refs #4855](https://github.com/nil-studio/oasis7/issues/4855)；本状态审查不关闭该任务。

- 要求：普通提案必须在提交时声明类别、目标、范围和授权来源；不匹配白名单或触及保护边界的事项必须原子拒绝或明确路由，不得以紧急、技术升级、复合提案或先行实现产生部分效果。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)
- 验收：[AC-WR-GCB-001](#ac-wr-gcb-001)

<a id="ac-wr-gcb-001"></a>
### AC-WR-GCB-001：越界事项拒绝且不占用世界资源

Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b；声明：[REQ-WR-GCB-001](#req-wr-gcb-001)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-GCB-001
- 给定：一个普通治理提交，分别覆盖白名单内政策/财库/既有 charter 日常事项和触及独立资产、安全 finality 或治理自身边界的事项。
- 当：系统分类并处理提交。
- 则：白名单事项显示范围与授权并进入普通流程；越界事项原子拒绝或路由到独立轨道，不产生资金占用、权限变更、部分执行或事实上的先行效果。

<a id="req-wr-gcb-002"></a>
### REQ-WR-GCB-002：宪制变更必须满足独立条件才生效

Trace ID: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06

- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：当前普通票局及技术manifest治理均未实现本条独立宪制合取程序。票局只记结果，技术timelock/finality不具受影响主体独立确认、宪制申诉及连续receipt语义。 [open逐Agent生成snapshot](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/governance_meta_publication.rs#L238-L401)；[native票局按期限、quorum、最高选项阈值结算。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/gameplay_loop.rs#L376-L427)；[技术manifest提案有执行条件、timelock与finality处理。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/governance_publication.rs#L50-L132)；[只读投票quote计算当前/投后票数与建议](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/governance_quote.rs#L33-L148)；[WorldEventBody穷尽分派Domain/Governance/CapabilityAuthorization；模块状态写bytes](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing.rs#L801-L900)；[完整治理程序消费三种票局事件](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_builtin_wasm_modules/m5_gameplay_governance_council/src/lib.rs#L13-L227)；[active Gameplay Tick emit仅反序列化五类具型directive](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/gameplay_loop.rs#L25-L254)。闭合范围同时核对typed dispatcher、当前M5 bootstrap、builtin council及具型emit consumer；不涵盖任意未提供的外部部署/WASM。测试源码不是运行记录；本轮未取得本条完整组合验收通过证据。
- 核对日期与基线：2026-10-17；需求卡为PR #4844 head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；源码固定当前main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`，所引源已逐Git blob复算匹配完整tree；仅静态审读，本轮未运行业务测试、模型、世界或正式端到端验收。
- 剩余范围：实现宪制影响说明、延迟/复核、独立确认、审计、申诉/receipt及当前授权合取闸门；缺任一待决/过期/原子拒绝并显示缺项，不产生部分规则或资产效果。 实施待安排： [Refs #4855](https://github.com/nil-studio/oasis7/issues/4855)；本状态审查不关闭该任务。

- 要求：改变非保护性宪制规则的事项必须同时具备影响说明、延迟/复核窗口、适用的独立确认、审计、申诉和连续 receipt；缺少任一条件时保持待决或拒绝。
- 专业权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)
- 验收：[AC-WR-GCB-002](#ac-wr-gcb-002)

<a id="ac-wr-gcb-002"></a>
### AC-WR-GCB-002：条件缺失不产生部分宪制效果

Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06；声明：[REQ-WR-GCB-002](#req-wr-gcb-002)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-GCB-002
- 给定：一项会改变宪制规则但不直接改写保护底线的提案。
- 当：审议、延迟、独立确认、审计或申诉条件中任一项尚未满足，或授权在执行前失效。
- 则：玩家能读到缺失条件和下一步，提案保持待决、过期或原子拒绝；不会改变规则、资产、权限或已确认历史。

<a id="req-wr-gcb-003"></a>
### REQ-WR-GCB-003：已通过事项执行时必须重新校验并只生效一次

Trace ID: O7-REQ-4f6597ff-8e34-4d12-bb60-060df0e09595

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：政策执行切片确实在执行时重新检查操作者现存且具有任一Passed提案、当前税率/合同配额边界及被列Agent存在，然后生成GameplayPolicyUpdated；另核专用OC财库切片重验批准状态、distribution_id唯一性及当前桶余额，作为边界旁证，不扩大普通事项覆盖。 [政策执行当前参数/对象重验](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_policy_contract.rs#L15-L128)；[任一Passed提案授权边界](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing.rs#L89-L96)；[政策状态实际写入准备](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/core_policy_transition.rs#L119-L145)；[财库批准、当前余额与distribution去重](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_core_main_token.rs#L111-L276)。读面边界：政策state实际更新且普通提案Passed只为票局结果；投票quote不能证明政策已生效。未核到原事项→执行拒绝/成功receipt的完整玩家读面。 本轮只读静态源码，未运行业务测试或端到端验收；不将单项机制、字段或普通Agent投票代签完整义务。
- 核对日期与基线：2026-10-17；固定源码/产品审读基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；新增下载24/24文件Git blob匹配该tree；全部26个引用源另逐一核验26/26匹配；与 `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5` 的仅文档差异沿用前轮核对。本轮无业务执行证据。
- 剩余范围：政策授权只匹配操作者任一Passed提案，不绑定本次proposal_id或批准payload，也没有按该事项消耗；财库distribution_id去重不等于proposal一次消费。具体批准事项的当前scope、撤销/到期、宪制重分类、跨重试单次世界效果及失效读面仍未闭合。 产品语义已冻结，专业技术承接未核到不等于待定产品政策；未检视路径不判未实现。

- 要求：普通事项通过只取得有限执行资格；执行时必须重新校验当前授权、范围、前置条件和反滥用边界，同一事项最多产生一个 receipt 支持的世界效果。
- 专业权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- 验收：[AC-WR-GCB-003](#ac-wr-gcb-003)

<a id="ac-wr-gcb-003"></a>
### AC-WR-GCB-003：授权漂移与重试不能扩大或重复执行

Trace refs: O7-REQ-4f6597ff-8e34-4d12-bb60-060df0e09595；声明：[REQ-WR-GCB-003](#req-wr-gcb-003)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-GCB-003
- 给定：一项已通过但尚未执行的普通事项，随后发生授权撤销/收缩、范围变化、并发提交或重连重试。
- 当：执行闸门处理原请求及其重复请求。
- 则：只有当前条件仍有效且范围匹配的请求可产生一次 receipt；其他请求原子拒绝、过期或保持待决，不能部分拨付、继承旧条件、取得第二次资金/权限/优先级。

<a id="req-wr-gcb-004"></a>
### REQ-WR-GCB-004：治理主体聚合与控制权公平必须可审计

Trace ID: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd

- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：当前计票主体明确为Agent；snapshot与单Agent改票是相邻机制。未实现经验证账户聚合bindings、按实际控制人限权的治理主体及duplicate challenge纠错合同。 [snapshot取Agent reputation与profile/stake/status；missing profile给默认cap](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L903-L1006)；[open逐Agent生成snapshot](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/governance_meta_publication.rs#L238-L401)；[Alliance charter是String；governance profile与snapshot以Agent为键；proposal状态Open/Passed/Rejected。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/gameplay_state.rs#L23-L175)；[WorldEventBody穷尽分派Domain/Governance/CapabilityAuthorization；模块状态写bytes](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing.rs#L801-L900)；[DomainEvent穷尽dispatcher将货币、工业、治理、Alliance/contract各自路由具型reducer。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L1008-L1140)；[完整治理程序消费三种票局事件](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_builtin_wasm_modules/m5_gameplay_governance_council/src/lib.rs#L13-L227)；[active Gameplay Tick emit仅反序列化五类具型directive](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/gameplay_loop.rs#L25-L254)。闭合范围同时核对typed dispatcher、当前M5 bootstrap、builtin council及具型emit consumer；不涵盖任意未提供的外部部署/WASM。测试源码不是运行记录；本轮未取得本条完整组合验收通过证据。
- 核对日期与基线：2026-10-17；需求卡为PR #4844 head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；源码固定当前main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`，所引源已逐Git blob复算匹配完整tree；仅静态审读，本轮未运行业务测试、模型、世界或正式端到端验收。
- 剩余范围：实现账户/approved bindings主体聚合、事项锁定OC、可撤销scope委托、实际控制人上限、隐私最小化审计和challenge→review→merge/revocation→计票correction→appeal；保留历史且不双计。

- 要求：经验证游戏账户聚合其已批准的玩家身份、managed signer 与 external wallet bindings 作为一个治理主体；线性锁定 OC、明确范围内可撤销委托与按实际控制人聚合后的影响上限共同防止账户/组织拆分、重复绑定或循环转移放大普通治理影响。普通假名得到保留，公开审计只暴露必要的委托关系、最终计票/结果与纠错因果；重复主体 challenge 必须有 review、merge 或 revocation、计票 correction 与 appeal，并保留原历史 receipt。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- 验收：[AC-WR-GCB-004](#ac-wr-gcb-004)

<a id="ac-wr-gcb-004"></a>
### AC-WR-GCB-004：重复主体不得扩大影响且纠错保留历史

Trace refs: O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd；声明：[REQ-WR-GCB-004](#req-wr-gcb-004)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-GCB-004
- 给定：一个账户同时声明多个已批准绑定，或多个账户/组织通过拆分、委托、循环转移形成同一实际控制关系，并出现重复主体 challenge。
- 当：系统评估普通治理资格、影响上限、公开审计与 challenge 处置。
- 则：绑定聚合为一个治理主体；锁定 OC、范围内可撤销委托与控制人上限不因拆分或循环转移增加影响；公开结果可审计但不泄露非必要个人数据；challenge 有 review 及适用的 merge/revocation、计票 correction 与 appeal，纠错保留原授权、投票、receipt、理由和最终有效结果，不产生双计或第二次治理效果。

<a id="req-wr-gcb-005"></a>
### REQ-WR-GCB-005：外部 OC 转让不得旁路游戏内治理权边界

Trace ID: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已实现的直接子范围仅限当前chain transfer入口OC→本World既有票局的隔离：生产main-token符号为OC，HTTP只接main_token并构造鉴权TransferMainToken到MainTokenTransferred；货币写点只改余额与nonce，治理依提案冻结的Agent snapshot计票，单Agent改票替换旧票；默认builtin治理程序也只消费票局事件。 [真实chain transfer HTTP入口只支持main_token asset](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/transfer_submit_api.rs#L838-L884)；[外部转账请求明确构造TransferMainToken并附MainTokenAction auth envelope。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/transfer_submit_api_support.rs#L152-L180)；[TransferMainToken处理构造MainTokenTransferred并以monetary prepare预验。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/action_to_event_core_main_token.rs#L16-L96)；[原生MainTokenTransferred重验账号/余额/nonce](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/main_token_monetary_publication.rs#L310-L387)；[snapshot取Agent reputation与profile/stake/status；missing profile给默认cap](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/state.rs#L903-L1006)；[production main-token配置明确symbol=OC。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/main_token.rs#L3-L29)；[完整治理程序消费三种票局事件](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_builtin_wasm_modules/m5_gameplay_governance_council/src/lib.rs#L13-L227)；[active Gameplay Tick emit仅反序列化五类具型directive](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/gameplay_loop.rs#L25-L254)。闭合范围同时核对typed dispatcher、当前M5 bootstrap、builtin council及具型emit consumer；不涵盖任意未提供的外部部署/WASM。测试源码不是运行记录；本轮未取得本条完整组合验收通过证据。
- 核对日期与基线：2026-10-17；需求卡为PR #4844 head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；源码固定当前main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`，所引源已逐Git blob复算匹配完整tree；仅静态审读，本轮未运行业务测试、模型、世界或正式端到端验收。
- 剩余范围：外部OC锁定/退出/解绑→具体治理主体及事项snapshot绑定、跨账户/实际控制人并发转移隔离仍未实现于上述入口；operator-managed quota bridge、token_key及服务消费的全部正式消费者和三种状态并列读面仍未核清，不把局部隔离扩成整条已验证。

- 要求：外部 OC 可转让，但持有、转入或转出不自动取得或保留游戏内普通治理权、高影响资格、区域权利、世界资产或行动结果；游戏内治理权只来自事项范围内可审计的锁定、snapshot 与退出/解除绑定。有效窗口后的转让或解除绑定不得追溯改票、双投或同时行使同一权利；既有 operator-managed 单向 `OC -> LetAI Run quota` 桥保持非治理、非赎回，额度、`token_key` 与服务消费不得影响治理资格。
- 专业权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- 验收：[AC-WR-GCB-005](#ac-wr-gcb-005)

<a id="ac-wr-gcb-005"></a>
### AC-WR-GCB-005：OC 转让、治理快照与额度桥保持隔离

Trace refs: O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5；声明：[REQ-WR-GCB-005](#req-wr-gcb-005)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-GCB-005
- 给定：一次 OC 外部转让、一个已进入有效窗口的事项，以及一条额度桥绑定或服务消费记录。
- 当：发生转让、反向转移、退出/解除绑定、重复投票或额度消费尝试。
- 则：玩家能区分外部 OC receipt、事项范围内锁定/snapshot/退出状态和服务额度状态；转让或解除绑定不追溯改票、不产生双投或双重控制，不自动给予世界权利；额度桥仍只能单向提供服务额度，不能兑换 OC、影响治理资格/权重或产生游戏内治理效果。缺少专业 authority 时，结果保持待决、拒绝或 `incomplete/unknown/blocked`，不得以默认值补齐。

<a id="req-wr-gcb-006"></a>
### REQ-WR-GCB-006：高影响保护动作必须有界、可审计且可复核

Trace ID: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已实现内部World处罚/申诉/恢复的直接子范围：target+evidence去重、slash上限、冻结、申诉期限、接受后恢复和连续证据链；技术紧急brake有期限与释放。节点intent明确pending/enforced=false，reader只回读已有记录，故不声称正式治理授权producer已接通。 [内部API校验目标/非空证据理由、申诉窗口和guardian IDs](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/governance_identity_penalty.rs#L10-L144)；[target+evidence incident去重、slash不超过锁仓、冻结；申诉期限/状态检查；接受申诉返还slash并恢复原身份](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/event_processing/prepared_governance_events.rs#L551-L820)；[guardian检查已登记node公钥、去重、计数阈值。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/governance_internal.rs#L390-L418)；[紧急brake具期限上限和释放；veto要求提案已批准且已排程。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/governance.rs#L344-L441)；[节点证据明确产pending_governance_submission且enforced=false的intent。](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_node/src/node_engine_slashing.rs#L3-L38)；[状态读面加载World](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/status_server_support.rs#L690-L744)；[完整治理程序消费三种票局事件](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7_builtin_wasm_modules/m5_gameplay_governance_council/src/lib.rs#L13-L227)；[active Gameplay Tick emit仅反序列化五类具型directive](https://github.com/nil-studio/oasis7/blob/8bbce1b1252a98366ba3dfa8abb3f46c881e2375/crates/oasis7/src/runtime/world/gameplay_loop.rs#L25-L254)。闭合范围同时核对typed dispatcher、当前M5 bootstrap、builtin council及具型emit consumer；不涵盖任意未提供的外部部署/WASM。测试源码不是运行记录；本轮未取得本条完整组合验收通过证据。 [PR #3628](https://github.com/nil-studio/oasis7/pull/3628)为原状态卡所记原子publication交付定位，本轮未重新独立核其合并/运行事实，不用作最新正式入口证明。
- 核对日期与基线：2026-10-17；需求卡为PR #4844 head `278eab5ead2a5fceb67a0e27a38303bbb3a1459d`；源码固定当前main `8bbce1b1252a98366ba3dfa8abb3f46c881e2375`，所引源已逐Git blob复算匹配完整tree；仅静态审读，本轮未运行业务测试、模型、世界或正式端到端验收。
- 剩余范围：正式提交者和绑定World事件的authority、initiator/guardian本次签名、证据充分性/target/金额/期限、申诉/裁决角色仍需核清或实现；逐类补齐否决/降权等高影响动作、自动到期恢复、紧急防绕过及正式跨入口验证。不得把内部API缺少角色校验写成远程可绕过。

- 要求：冻结、否决、降权、惩罚或其他高影响保护动作只能在专业合同声明的授权、理由、证据、作用范围和有效期限内发生，并且必须保留可审计 receipt、恢复/复核/申诉路径与明确的失败结果。紧急动作不得扩大普通治理白名单、改写已确认历史或取消申诉；同一证据或请求至多产生一个世界效果。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- 验收：[AC-WR-GCB-006](#ac-wr-gcb-006)

<a id="ac-wr-gcb-006"></a>
### AC-WR-GCB-006：紧急高影响动作不绕过治理且保留唯一复核路径

Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8；声明：[REQ-WR-GCB-006](#req-wr-gcb-006)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-GCB-006
- 给定：一项有范围和期限的高影响保护动作，以及一项证据不足、越权、重复请求或需要复核/申诉的对照样例。
- 当：专业 authority 评估授权、理由、证据、作用范围、期限和既有历史。
- 则：授权完整的动作只在声明边界内生成一个可审计结果，并暴露恢复、复核或申诉路径；缺项原子拒绝、保持待决或收窄范围，不产生部分惩罚、隐性资格变化或第二次世界效果；紧急路径不能改写历史、扩大白名单或取消申诉。

## 6.3 迁移闭合与边界

- 历史 GG-1..5 的产品承诺按下表由稳定 REQ/AC 与组合验收接收。

| 历史承诺 | 当前 active REQ / AC 接收方 | 组合验收覆盖 |
| --- | --- | --- |
| GG-1：普通治理白名单及越界拒绝/路由 | [REQ-WR-GCB-001](#req-wr-gcb-001) / [AC-WR-GCB-001](#ac-wr-gcb-001) | GCB-1（与 GCB-2 同属 `test_tier_full`） |
| GG-2：治理主体聚合与控制权公平 | [REQ-WR-GCB-004](#req-wr-gcb-004) / [AC-WR-GCB-004](#ac-wr-gcb-004) | GCB-7（`test_tier_full`） |
| GG-3：外部 OC 与游戏内治理权隔离 | [REQ-WR-GCB-005](#req-wr-gcb-005) / [AC-WR-GCB-005](#ac-wr-gcb-005) | GCB-8（`test_tier_full`） |
| GG-4：组织保护底线、解散及不活跃连续性 | [REQ-WR-OC-001](organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001) / [AC-WR-OC-001](organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-001)；[REQ-WR-OC-002](organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002) / [AC-WR-OC-002](organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-002) | OC-1..4（OC-1/2 与 OC-3/4 均属 `test_tier_full`） |
| GG-5：受保护底线及宪制变更程序 | [REQ-WR-GCB-001](#req-wr-gcb-001) / [AC-WR-GCB-001](#ac-wr-gcb-001) 与 [REQ-WR-GCB-002](#req-wr-gcb-002) / [AC-WR-GCB-002](#ac-wr-gcb-002) | GCB-1/2（`test_tier_full`）及 GCB-3/4（均为 `test_tier_full`，包括紧急 containment 与宪制修订分离） |

本 crosswalk 指向目标规则与组合验收，不改变各 REQ/AC 追踪行的测试层级，也不构成实现、当前可用性或发行 readiness 证据。

## 7. 组合验收

- GCB-1：白名单内的代表性政策、公共财库和既有 charter 日常事项能够进入普通治理；宪制、基本权利、安全、validator/finality、signer 与 custody 事项被原子拒绝或明确路由到非普通轨道，且不产生部分世界效果。
- GCB-2：普通提案、财库、charter、技术升级、局部多数、历史声望、复合提案和先行实现均无法修改不可由普通治理改写的保护或普通治理自身权限边界。
- GCB-3：代表性其他宪制变更只有在公开影响说明、延迟与复核、适用超多数、跨区域或受影响主体确认、独立审计和程序性申诉全部成立时才生效；缺失任一条件时不产生部分效果。
- GCB-4：紧急 containment 只能限制扩散或维持其授权范围内的基本连续性，不能代替宪制修订、扩大白名单或暂停保护、审计与申诉。
- GCB-5：受理、拒绝、路由、宪制待决与最终结果均保留授权、理由、范围、下一步和 receipt 因果；玩家不会把提案、投票、队列或界面状态误认为已生效世界事实。
- GCB-6：一个已通过的代表性普通政策、财库或 charter 日常事项，在执行前遇到授权撤销/收缩、范围或前置条件变化、并发提交及重连/Agent 重试时，只有在执行时仍有效的事项可产生一次 receipt 支持的世界效果；其余请求不会部分执行、继承旧条件或产生第二次资源/权限/优先级。玩家能区分已通过、执行受阻/失效与已确认结果，并获得重新提交、等待、申诉或常态替代中的适用下一步。
- GCB-7：代表性多绑定账户、拆分账户/组织、可撤销委托与重复主体 challenge 只能形成一个可审计治理主体；控制人聚合后的影响上限不因拆分或循环转移扩大，纠错保留原 receipt 与最终有效结果，公开审计不泄露非必要个人数据。
- GCB-8：代表性 OC 外部转让、治理事项 snapshot/退出/解除绑定和额度桥消费保持分离；转让或解除绑定不能追溯改票或双投，单向额度桥不能兑换 OC、影响治理资格/权重或产生世界治理效果。
- GCB-9：代表性冻结、否决、降权、惩罚或其他高影响保护动作只在已声明授权、理由、证据、范围和期限内产生一次可审计结果；证据不足、越权、重复请求和复核/申诉样例不产生部分或第二次效果，紧急路径不扩大白名单、不改写历史且保留恢复/复核/申诉。

## 8. 验收追踪

| 产品承诺 | 专业 owner | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| GCB-1 / GCB-2 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 普通事项白名单、排除事项原子拒绝/路由、保护边界与多种绕过负例的组合证据 | test_tier_full |
| GCB-3 / GCB-4 | producer_system_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer | `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 宪制条件全满足/缺失样例、紧急 containment 与宪制修订分离、无部分效果的确定性证据 | test_tier_full |
| GCB-5 | producer_system_designer / gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 事项分类、受理/拒绝/路由/待决/结果的 receipt 因果及正式玩家 surface 可读性证据 | test_tier_full |
| GCB-6 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / viewer_engineer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | `test_tier_required` 覆盖执行前再校验、授权/范围失效、玩家状态区分与恢复下一步；`test_tier_full` 覆盖并发执行、Agent/重连重试、去重、replay 与恢复后的唯一 receipt/无第二次世界效果 | test_tier_full |
| GCB-7 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 治理主体绑定聚合、实际控制人影响上限、最小数据公开审计、重复主体纠错/申诉与历史 receipt 证据 | test_tier_full |
| GCB-8 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | OC 转让与治理快照/解除绑定的非追溯/非双投负例，以及单向非治理 quota bridge 隔离证据 | test_tier_full |
| GCB-9 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 高影响保护动作的授权/理由/证据/范围/期限、原子拒绝、唯一 receipt、恢复/复核/申诉与紧急防绕过负例；具体阈值、状态机和测试由专业 authority 拥有 | test_tier_full |

## 9. Non-Goals

- 不定义治理资格的参数或实现机制，不定义资产锁定比例/期限、委托与控制人计算、身份聚合技术、隐私机制、权重、阈值、时钟或经济参数；本页仍冻结治理资格的产品边界、控制权公平、审计/纠错/申诉要求与验收，GCB-004/006 不宣称资格计算、惩罚状态机或相关专业实现已经存在。
- 不定义 OC 外部转让、游戏内权利绑定或既有 `OC -> LetAI Run quota` 桥的链上/服务实现；GCB-005 只冻结两侧不可互推的产品边界与负例。
- 不实现投票、提案、宪制修订、申诉、runtime/P2P 状态机、validator/finality、signer 或 custody 操作。
- 不把本文、历史证据或局部实现写成当前功能、preview readiness、主网、发行或公开 claim。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-WR-GCB-001](#req-wr-gcb-001) / [AC-WR-GCB-001](#ac-wr-gcb-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-GCB-002](#req-wr-gcb-002) / [AC-WR-GCB-002](#ac-wr-gcb-002) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-GCB-003](#req-wr-gcb-003) / [AC-WR-GCB-003](#ac-wr-gcb-003) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-GCB-004](#req-wr-gcb-004) / [AC-WR-GCB-004](#ac-wr-gcb-004) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 治理主体聚合、锁定/委托/控制人影响上限、最小数据公开审计、重复主体 review/merge/revocation/correction/appeal 与历史 receipt 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-GCB-005](#req-wr-gcb-005) / [AC-WR-GCB-005](#ac-wr-gcb-005) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | OC 外部转让与游戏内治理权的锁定/snapshot/退出隔离、不可追溯/不可双投与单向非治理 quota bridge 负例的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-GCB-006](#req-wr-gcb-006) / [AC-WR-GCB-006](#ac-wr-gcb-006) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 高影响保护动作的授权、理由、证据、范围、期限、唯一 receipt、恢复/复核/申诉与紧急防绕过产品边界；参数、状态机和执行证据仍由专业 authority 拥有 | `test_tier_full` |

## 未决问题与证据边界

普通事项执行、宪制独立轨道与紧急保护的适用专业合同及同候选组合证据须分别核对；迁移 crosswalk 仅保留语义去向，不能替代治理权限或发行验收。

核对责任由本文 Owner role 联合所列专业 owner 承担；解除触发是相应功能进入实现或对外 claim 评审前，按本条 REQ/AC 核定专业承接、固定候选与实际证据。未核清前保留上列缺口，不用结构检查推导完成。
