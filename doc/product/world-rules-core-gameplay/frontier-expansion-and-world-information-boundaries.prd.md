# Frontier 扩展与世界信息边界

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-09-13
- 专业域权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)

本文是长期产品分册，定义有限已知世界的 frontier 扩展、pioneer priority 和世界信息可见性的产品边界。它不定义地图生成、相邻判定、物流算法、charter 字段、priority 时长/价格、情报访问控制、数据格式、侦察 UI、runtime 状态机或当前 readiness。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 已直接说明 frontier 选择、期限机会和信息分层的产品边界；空间与情报呈现由专业 authority 决定。
- 历史设计判定记录：本次分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，本次闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
本节的产品体验设计豁免不等于专业系统设计不适用；各要求的技术覆盖与实现证据分别按下列状态摘要核对。

## 1. 产品目标

世界的已知范围是有限但可持续扩展的：玩家、Agent 与组织只能借由受治理的相邻 frontier 探索、物流建立和 charter 扩展进入新区域。扩展不是生成新的独立服务器、经济或历史，而是将新的地点、资源、义务与结果接入同一条权威时间线、同一经济和同一可审计世界历史。

玩家需要既能审计公共规则和已结算结果，又不能免费获得所有实时战略情报。探索与投资可以产生有限的先行开发机会，但不产生领土主权或永久信息垄断。

<a id="2-范围与玩家边界"></a>
## 2. 范围与玩家体验边界

### 2.1 受治理的相邻 frontier 扩展

- 只有已知世界相邻的 frontier 才能被探索、经由物流建立并进入 charter 扩展路径；任意跳跃、脱离相邻关系的宣称、私有副本或另起时间线不能形成世界扩展。
- 探索、物流和 charter 分别证明发现、可达/维持以及公共组织边界；任一环节不足都不能单独把未知地点变为定居区域、完整市场或自治政治体。
- 新区域的资源、合同、损失、公共义务和历史结果持续归属同一世界经济与因果链。frontier 扩展不重置旧区域、绕过既有权利/安全边界，或把发现者的局部行动写成全局治理权。

### 2.2 Pioneer priority 不是主权

- 对满足探索和开发里程碑的主体可以授予 pioneer priority：它是可转让的、会到期、受范围限制的先行开发或协作优先机会。
- priority 必须绑定明确的开发里程碑、地点/用途与失效条件；未推进、到期、转让或违反适用规则后，机会按规则释放、转交或重新竞争。
- priority 不等于土地所有权、区域主权、永久排他、军政控制、免除 charter、免除通行义务或阻止其他合格主体探索/提供服务。任何已形成的地点 tenure、公共通行或区域权利仍按各自的受治理边界处理。

### 2.3 Priority 与 tenure 的独立切割

- pioneer priority 只是对下一个合格开发步骤的限时、限范围优先机会；它本身不创建 tenure、所有权、charter 权力或排他访问。tenure 只能在其独立资格、用途和确认程序完成后成立。
- tenure 成立前转让 priority，只转让剩余的地点/用途范围、里程碑义务、原到期时点和相关 receipt 链；受让人不继承发现者身份、声誉、私有数据权限或预先批准的 tenure，也不得借转让延长原期限。
- priority 到期、撤销或里程碑失败时，只终止尚未消费的优先机会并释放对应开发步骤；不得静默取消已独立确认的 tenure、已完成交付、已结算合同或其 receipt。已确认 tenure 仅按自身的续期、使用、违约、公共必要性、通知、申诉与恢复规则处理；过往 priority 也不为 tenure 保留新的排他授予或续期优势。
- priority 资格、里程碑或到期事实处于争议且证据不足时，拒绝新的 priority 排他授予与转让，但不冻结普通安全披露、非排他探索、已确认 tenure 或已结算义务。争议结果的 receipt 必须说明范围、原因、下一路径以及 tenure 是否曾被独立确认；已撤销 priority 不得通过重放历史 receipt 重新取得。

#### 受让人资格与转让再校验

- 转让不能成为 frontier 资格的旁路。提交或接受转让时，权威必须按当前事实重新核验受让人是否满足该 frontier、地点和用途的适用探索、物流、charter、维护与反滥用条件；原持有人也必须仍有可转让的有效 priority。产品层不冻结资格字段或阈值，但不能把“已登记受让人”表达为“已具备开发资格”。
- 只有受让人、原 priority 状态、地点/用途范围、未完成里程碑与原到期边界同时通过再校验，转让 receipt 才能确认。确认只转移剩余机会及其 provenance，不转移发现者身份、声誉、私有观察数据、数据访问权、已完成里程碑或任何预先批准的 tenure；不得借转让延长原期限或扩大范围。
- 受让人不合格、范围不再适用、原 priority 已到期/撤销，或资格在待决期间发生变化时，转让必须原子拒绝、过期或按专业合同保持待决，不消费 priority、不产生新的开发资格、不延长期限，也不形成隐性排队优势。原持有人仅在 priority 仍有效且未被其他已确认结果消费时保留原机会；否则维持其已确认的到期/撤销结果，不能因为失败转让而回滚。
- 转让 receipt 或拒绝/过期 trace 必须让受影响主体读到原 priority、受让人、地点/用途范围、剩余期限、资格校验的语义结果（例如通过、待复核或不合格）及原因类别和下一步；不得泄露不必要的私有资格证据。重连、重复提交、关联主体循环转让或旧 trace 重放不得绕过这次再校验。

#### 一次性消费、并发转让与可恢复失败

pioneer priority 在其未到期、未撤销且里程碑仍适用时，只有两类会改变机会归属的结果：**已接受的转让**，或**已接受的下一个合格开发步骤**。前者将剩余机会及其原有期限转给一个受让人；后者只消费该步骤所需的优先机会，不能顺带创建 tenure、扩展到其他地点/用途，或把后续步骤预先排他。尚未得到权威结果的转让或开发请求只是待决，不能被玩家、Agent、界面缓存或链下约定当作已取得/已消费 priority。

- 对同一 priority 的并发转让、受让后立即提交、原持有人与受让人竞争提交，或重连后的重复请求，世界只能接受其中一个符合当时条件的归属/消费结果；其余请求必须原子拒绝、过期或按专业合同保持待决，且不得产生第二次 priority 消费、额外开发资格、隐性排队优势或资源 sink。玩家能读到当前持有人或已消费范围、冲突原因，以及重新评估、等待、以非 priority 路径探索，或在条件允许时重新竞争的下一步。
- 网络中断、审计尚未完成或争议开始时，未能证明已被接受的请求不得延长到期时点、保留私下排他，或阻止无关的安全披露、普通探索和既有已结算义务。恢复后只能查询 receipt 并按当时仍有效的资格重新评估；历史 transfer/priority receipt 不能重放为新的转让或开发提交。
- 为防止把有限先行机会变成可套利的永久排他，拆分、循环倒手、向关联主体回转或用未结算请求反复刷新，均不得重置期限、里程碑、地点/用途范围或已消费范围；任何允许的转让仍保留完整 provenance，并受同一争议、撤销和反滥用边界约束。

#### Tenure 与 charter 状态转换的待决边界

tenure 的续期、转让、收回、迁移、补偿或申诉请求必须绑定提交时的 charter 身份、适用范围和 tenure 状态。登记、排队、链下约定或界面提示不等于控制、处置、补偿或其他世界结果已经成立。

- charter 暂停、解散、合并、边界收缩或由新授权替代后，尚未得到权威 receipt 的旧请求不得自动转入新 charter、继承优先级或产生控制、资产、补偿或其他处置效果。它们只能在旧状态仍允许时按专业合同确认，或被明确拒绝/保持待决；新 charter 确实覆盖相同需要时，主体必须显式重提，并保留与旧请求的可读关联。
- 同一 tenure 范围内并发的续期、转让、收回、迁移或补偿请求，至多一个权威 receipt 可以改变控制或处置归属。其余请求必须原子拒绝、过期或保持待决；重连、自动重试或历史 receipt 重放不得制造第二次世界效果、隐性排队优势或静默没收。
- 受影响主体必须能读到原请求绑定的 charter、当前状态、范围变化或拒绝原因，以及重提、申诉、迁移、独立替代或等待等适用的下一步。已确认历史保持可审计，不能因后续 charter 状态变化被追溯改写。

这些规则保持 **world-first**：归属与消费只以同一权威时间线中的接受结果为准；保持 **emergence-first**：探索和协作可竞争、可转让但不产生主权或免费旁路；保持 **persistent/auditable**：已结算结果与未结算失败可由 receipt 和范围追溯；保持 **extensible**：未来可增加开发步骤或转让机制，但必须保留一次性消费、原期限和无重复权益的不变量。

### 2.4 探索数据与公共安全基线

- 探索获得的数据可在与地点、用途和时间相称的范围内提供私有商业使用或先行研究优势；这不构成永久秘密、对他人基本恢复能力的剥夺或安全风险的隐瞒许可。
- 至少与公共安全、可通行性、重大已知危险和受影响主体基本恢复有关的最低事实必须按适用授权公开；私有期、保护期或争议处理结束后，可公开的探索知识应形成可复核的公共 baseline。个人数据、敏感漏洞细节、实时防御部署或他人私有合同不因公共 baseline 而自动公开。
- 已披露或被消费的数据都必须说明来源、范围、取得/更新时间和不确定性；过期、冲突或局部观察不得被包装成整个 frontier 的实时完整真值。

### 2.5 可审计公共事实与需要取得的实时信息

- 公共规则、已结算世界结果、已成立 charter、适用公共安全信息和可公开 baseline 必须可审计，不能被拥有更强信息能力的主体秘密改写。
- 实时库存、路线容量/状态、部署、计划、未结算订单和其他 live operational information 默认需要侦察、已有关系、购买、合同或明确授权才能获得；玩家不能因访问产品、持有 OC、加入组织或历史名望自动读取。
- 任何向玩家或 Agent 提供的实时信息必须标明 freshness、范围和不确定性，并在授权失效、侦察不足、数据冲突或状态变化时给出刷新、替代、等待或停止的可读路径。

## 3. 权威与冲突处理

| 产品层拥有 | 专业与执行权威 |
| --- | --- |
| frontier 进入的产品结果、priority 非主权、探索数据的私有/安全/公共分层，以及公共事实与实时信息的可见性边界 | `doc/game/prd.md` 拥有探索、物流、工业和玩家规则；`doc/world-runtime/prd.md` 拥有相邻/资格、世界状态、授权、receipt 与确定性执行；`doc/p2p/prd.md` 拥有分布式状态与安全技术边界；`doc/world-simulator/prd.md` 仅拥有玩家可见状态、receipt 与下一动作的表面表达；`doc/testing/prd.md` 拥有证据与当前 verdict |

本分册不定义地理算法、范围/时长/价格、情报保密技术、侦察实现、物流数值、charter/tenure 状态或公开 API。它不扩大现有区域 charter、市场发现、情报或玩家 surface 的当前专业真值；缺少同一候选专业证据时，只能保持为长期目标或较窄未承诺边界。

## 4. 路线图

1. 连续扩展：建立相邻探索、物流和 charter 缺一不可的 frontier 进入结果。
2. 有界先行：让 pioneer priority 支持早期开发而不成为永久主权或圈地。
3. 信息公平：把公共规则/已结算事实与实时运营情报分层，并让探索数据最终可形成安全的公共 baseline。
4. 可读不确定性：所有影响行动的信息都可追溯范围与 freshness，不以缓存或局部观察伪装全知。

## 5. Done：成功标准与验收

- FI-1：frontier 样例证明未知区域只能由相邻探索、物流和 charter 的受治理组合扩展，并持续接入同一时间线、经济和历史；任一跳跃或旁路被拒绝或清楚说明。
- FI-2：pioneer priority 样例证明其可转让、会到期且绑定开发里程碑/范围；每次转让都按当前 frontier、地点/用途和反滥用条件重新核验受让人资格，不合格或资格在待决期间失效时原子无效且不延长、不消费、不回滚既有到期/撤销结果；它不自动产生 tenure、所有权、主权、永久排他、区域控制或豁免。验收至少覆盖：到期于 tenure 成立前时释放/重新竞争；成立前转让不转移身份/声誉/数据权限且不延期；已独立成立 tenure 后 priority 到期不追溯没收；争议期不重复授予、不冻结无关安全/通行/已结算义务；已撤销 priority 不能凭历史 receipt 重放恢复；以及同一 priority 的转让、开发提交、重连重试或关联主体倒手竞争时，至多一个结果改变归属或消费范围，其他请求不产生第二次权益/sink，也不能重置期限或里程碑。每个样例都必须让受影响主体读到资格校验的语义结果、剩余持有人/范围/期限、原因类别与适用下一步。charter 暂停、边界变更、解散、合并或授权替代时，待决 tenure 续期、转让、收回、迁移、补偿或申诉请求不得跨越旧授权自动确认、迁移或继承优先级；同范围竞态至多一个 receipt 改变处置归属，受影响主体可读到原绑定、当前结果与适用下一步。每个样例都必须区分待定/争议中、有效、已转让、已消费、已独立成立 tenure 和到期/撤销状态，并提供 receipt 支持的下一动作。
- FI-3：探索数据样例区分有界私有商业使用、最低公共安全/可通行性披露与最终可公开 baseline，并保留来源、范围、freshness 和不确定性。
- FI-4：信息样例证明公共规则、已结算结果、charter 和安全事实可审计，而 live inventory、routes、deployments、plans 等需要侦察、关系、购买、合同或授权；每项实时信息均有 freshness 与恢复路径。
- FI-5：专题说明不将本长期模型或局部探索/市场证据表述为当前地图、信息系统、frontier、preview 或发行 readiness。

### 5.1 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| FI-1 / FI-2 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / viewer_engineer / qa_engineer | PRD-GAME-002 / PRD-WORLD_RUNTIME-001 / PRD-P2P-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 相邻 frontier、物流/charter、同世界连续性；priority 里程碑/期限/转让；受让人及原 priority 对探索、物流、charter、维护、反滥用条件的当前再校验；待决期间失效时不消费、不延期、不回滚；receipt/trace 可读资格语义结果、原因类别、剩余持有人/范围/期限与下一步且不泄露不必要私有证据；priority/tenure 独立状态、转让与开发提交并发、重复转让/重试、关联倒手、receipt 重放和静默 tenure 撤销负例；charter 状态转换下 tenure 待决请求的非跨越、单一处置 receipt 与玩家可读下一步 | test_tier_full |
| FI-3 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / qa_engineer | PRD-GAME-002 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 探索数据私有范围、最低安全/通行披露、公共 baseline、来源与 freshness 证据 | test_tier_required |
| FI-4 / FI-5 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-002 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | `README.md`; `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 公共/实时信息分层、侦察/关系/购买/授权、freshness/恢复和当前 claim 分离审计 | test_tier_required |

## 5.2 产品要求、叶子验收与未决问题

<a id="req-wr-fi-001"></a>
### REQ-WR-FI-001：frontier 扩展必须保持相邻、同世界和可恢复

Trace ID: O7-REQ-3013b8cf-e3db-4506-a30a-db34e25ed636

- 系统设计状态：未核实
- 实现状态：未核实
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已排除通用Move/物流/claim/社交数据作为frontier专属交付证明。 [生产dispatcher把战争/危机、经济合同、工业及物流分别路由；没有把这些普通动作自动解释为专属frontier/认可/赛季合同。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing.rs#L379-L478) [fragment refill是补充资源报价，不是frontier探索/charter准入或pioneer priority。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/fragment_refill_quote.rs#L1-L60) 本轮只读静态审读，未执行验收；实际实现交付PR映射未逐条核清，固定生产源码为交付子范围依据，不以未取得运行证据抹去已证交付。
- 核对日期与基线：2026-10-17；需求/专业设计/静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；本轮对比 main 仅文档变化，本条源码继续适用。无新产品运行验收。
- 剩余范围：探索、物流、charter的相邻frontier组合准入、跳跃/私有副本拒绝与同一世界接入的专属producer与Viewer/Agent消费者仍未核清。产品§5.3明确地图、邻接、物流与侦察能力何时进入可验证实现尚未决定；此policy/candidate边界不证明代码必然不存在。

- 性质：`目标要求`
- 适用条件：玩家、Agent 或组织尝试进入未知相邻区域。
- 要求：产品必须把探索、物流和 charter 作为可读的组合前置，并将新区域接入同一世界时间线、经济与历史；任一跳跃、私有副本或旁路不得制造独立世界结果。
- 理由：扩展应带来有代价的探索与协作机会，而不是重置风险或绕过既有权利。
- 上位承诺：frontier 扩展与世界连续性。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)。
- 验收：[AC-WR-FI-001](#ac-wr-fi-001)。

<a id="req-wr-fi-002"></a>
### REQ-WR-FI-002：pioneer priority 必须是有期限、有范围的机会

Trace ID: O7-REQ-066843dd-9e40-4148-8933-d369d16b55a2

- 系统设计状态：未核实
- 实现状态：未核实
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已排除通用Move/物流/claim/社交数据作为frontier专属交付证明。 [生产dispatcher把战争/危机、经济合同、工业及物流分别路由；没有把这些普通动作自动解释为专属frontier/认可/赛季合同。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing.rs#L379-L478) [fragment refill是补充资源报价，不是frontier探索/charter准入或pioneer priority。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/fragment_refill_quote.rs#L1-L60) 本轮只读静态审读，未执行验收；实际实现交付PR映射未逐条核清，固定生产源码为交付子范围依据，不以未取得运行证据抹去已证交付。
- 核对日期与基线：2026-10-17；需求/专业设计/静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；本轮对比 main 仅文档变化，本条源码继续适用。无新产品运行验收。
- 剩余范围：pioneer priority的地点/用途/里程碑/期限、受让资格、撤销到期、关联倒手去重及单次归属消费的专属producer与Viewer/Agent消费者仍未核清。产品§5.3明确地图、邻接、物流与侦察能力何时进入可验证实现尚未决定；此policy/candidate边界不证明代码必然不存在。

- 性质：`目标要求`
- 适用条件：探索或开发里程碑产生先行机会、转让或到期。
- 要求：产品必须区分 priority 与 tenure/主权，说明地点/用途、未完成里程碑、剩余期限、受让人资格结果和到期/撤销后的下一步；重连、重复、关联倒手或历史 receipt 重放不得产生第二次机会。
- 理由：玩家可以承担先行开发风险，但不能把限时机会套利成永久排他或隐性治理权。
- 上位承诺：pioneer priority 与 tenure 的独立边界。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)。
- 验收：[AC-WR-FI-002](#ac-wr-fi-002)。

<a id="req-wr-fi-003"></a>
### REQ-WR-FI-003：信息分层必须同时保护公共安全与不确定性

Trace ID: O7-REQ-23640893-de22-4940-9b25-ed08c9ab0f7e

- 系统设计状态：未核实
- 实现状态：未核实
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已排除通用Move/物流/claim/社交数据作为frontier专属交付证明。 [生产dispatcher把战争/危机、经济合同、工业及物流分别路由；没有把这些普通动作自动解释为专属frontier/认可/赛季合同。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing.rs#L379-L478) [fragment refill是补充资源报价，不是frontier探索/charter准入或pioneer priority。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/fragment_refill_quote.rs#L1-L60) 本轮只读静态审读，未执行验收；实际实现交付PR映射未逐条核清，固定生产源码为交付子范围依据，不以未取得运行证据抹去已证交付。
- 核对日期与基线：2026-10-17；需求/专业设计/静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；本轮对比 main 仅文档变化，本条源码继续适用。无新产品运行验收。
- 剩余范围：frontier公共事实/最低安全披露/授权live inventory和route的分类、来源/freshness/不确定性及失权恢复的专属producer与Viewer/Agent消费者仍未核清。产品§5.3明确地图、邻接、物流与侦察能力何时进入可验证实现尚未决定；此policy/candidate边界不证明代码必然不存在。

- 性质：`目标要求`
- 适用条件：玩家或 Agent 读取探索数据、公共事实或实时运营信息。
- 要求：产品必须让玩家区分可审计公共事实、最低公共安全披露和需要侦察/关系/购买/授权取得的实时信息，并显示来源、范围、freshness 与不确定性；证据不足时不得把局部观察包装成完整实时真值。
- 理由：信息优势应产生有界选择，而不是让安全事实被隐藏或让缓存伪装为可靠情报。
- 上位承诺：探索数据与公共安全基线。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)。
- 验收：[AC-WR-FI-003](#ac-wr-fi-003)。

<a id="ac-wr-fi-001"></a>
### AC-WR-FI-001：frontier 进入保持同一世界边界

Trace refs: O7-REQ-3013b8cf-e3db-4506-a30a-db34e25ed636；声明：[REQ-WR-FI-001](#req-wr-fi-001)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-FI-001。
- 给定：一个待探索的相邻 frontier。
- 当：玩家依次尝试探索、建立物流并进入 charter 扩展路径，同时提交跳跃或私有副本请求。
- 则：只有满足受治理组合前置的路径接入同一时间线、经济和历史；跳跃或旁路被拒绝或清楚说明原因。

<a id="ac-wr-fi-002"></a>
### AC-WR-FI-002：priority 转让与到期不制造主权

Trace refs: O7-REQ-066843dd-9e40-4148-8933-d369d16b55a2；声明：[REQ-WR-FI-002](#req-wr-fi-002)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-FI-002。
- 给定：一个尚未到期、带有地点/用途和开发里程碑的 pioneer priority。
- 当：发生转让、受让人资格变化、到期、撤销、并发提交或重连重试。
- 则：受影响主体能读到资格结果、剩余范围/期限和下一步；最多一个权威结果改变归属或消费，priority 不创建 tenure、主权、永久排他或延长期限。

<a id="ac-wr-fi-003"></a>
### AC-WR-FI-003：实时情报不会冒充公共真值

Trace refs: O7-REQ-23640893-de22-4940-9b25-ed08c9ab0f7e；声明：[REQ-WR-FI-003](#req-wr-fi-003)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-FI-003。
- 给定：同一 frontier 同时存在已结算公共事实、最低安全披露和 live inventory/route 状态。
- 当：玩家或 Agent 查看这些信息，或授权、freshness、观察范围发生变化。
- 则：公共事实可审计；实时信息要求真实授权并显示来源、范围、freshness 与不确定性；授权失效、证据冲突或过期时提供刷新、替代、等待或停止路径。

### 5.3 未决问题与证据边界

- 尚未决定：frontier 的具体地图、邻接、物流和侦察能力何时进入可验证实现；影响 FI-1 至 FI-5 的当前 verdict，决策负责角色为 `producer_system_designer` 联合 gameplay、runtime、Agent、Viewer 与 QA，触发条件是进入相应实现或公开 claim 审查前，解决前临时不承诺当前存在可进入 frontier 或实时情报能力。
- 本分册证据只能证明指定候选、入口、版本/窗口和环境中的扩展与信息边界；局部探索样本、缓存或文档建档不能证明当前地图、公共 baseline、公开 API 或发行 readiness。

## 6. Non-Goals

- 不定义地图、相邻、路线、侦察、库存、部署、计划、charter、priority、数据权限或 freshness 的算法、数值、字段、UI 或 runtime 实现。
- 不把 pioneer priority、探索数据或 frontier 开发扩写为主权、永久排他、全局治理权、免费实时情报或经济旁路。
- 不承诺当前存在任何可进入 frontier、地图扩展、信息商业权、公共 baseline 或公开 API。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-WR-FI-001](#req-wr-fi-001) / [AC-WR-FI-001](#ac-wr-fi-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-FI-002](#req-wr-fi-002) / [AC-WR-FI-002](#ac-wr-fi-002) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-FI-003](#req-wr-fi-003) / [AC-WR-FI-003](#ac-wr-fi-003) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
