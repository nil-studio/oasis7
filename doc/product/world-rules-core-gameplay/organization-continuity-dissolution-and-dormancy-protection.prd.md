# 组织连续性、解散与长期不活跃保护

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[prd.md](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-19
- 配套专业系统设计：[组织连续性与处置](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-001)（DES-WR-OC-001..004：处置顺序、阶段与请求层、原子授权交接、恢复与非重放）。
- 专业域权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- 首版政策提案：[组织连续性政策 v1](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-time) 是本 PR 补入的实现输入；活动、期限、资格、资产顺序及不足分配的精确规则只有该专业 authority 一份，不表示运行世界已启用或先前已获批准。

本文定义组织持续经营、解散和长期不活跃时的玩家结果、首版政策选择与制度边界。它补充成熟世界成长、Agent 所有权与冲突恢复分册；不定义治理权重、身份技术、清算价格、estate schema、链上交易、runtime 状态机、运营操作或当前放行结论。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 已完整表达组织解散、不活跃保护、交接和 reclaim 的产品阶段边界；具体 lifecycle surface 由专业 authority 决定。此项仅豁免独立产品交互 design；配套专业系统设计承接技术合同，不改变产品 authority，也不声明实现已完成。
- 历史设计判定记录：本次分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，本次闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
<a id="1-产品承诺"></a>
本节的产品体验设计豁免不等于专业系统设计不适用；各要求的技术覆盖与实现证据分别按下列状态摘要核对。

## 1. 产品目标与承诺

负责人暂时没有回来，仍在交付的 Agent 不能被误判为“无人经营”；真正停摆后，其他成员又需要有规则地维护设施、履行欠货，或有序退出。玩家回来时应能看清哪些资产仍属于自己、谁在代为做什么、已经发生哪些成本，以及怎样恢复或提出异议。

组织可以围绕共同目标持续协作、重组或退出，但不能把成员、独立资产、Agent 或历史变成可任意抹除、秘密没收或失去因果记录的对象。解散和不活跃是同一持续世界中的可理解状态变化：玩家能知道触发原因、当前范围、保留价值、义务、异议或申诉，以及下一次可执行决策。

这项保护保持 world-first：结果以权威世界事实而非组织叙述为准；保持 emergence-first：组织可以在共同规则内协作、重组或恢复，而不能借不活跃取得旁路收益；保持 persistent / auditable：身份、来源、receipt 和已确认因果持续可追溯；保持 extensible：未来组织形态可扩展，但不得低于本文保护底线。

### 1.1 范围、正常路径与玩家选择

首版面向已有稳定组织身份、charter 和可核验权属的持续经营组织；缺权属、资格或争议处理 authority 的旧联盟保持可读、补证和安全停止，不自动进入可处分 estate。玩家先查看经营状态、义务和候选接替人，再决定正常继续、预先指定有限接替、为维护提供专用预算、申请恢复，或在授权范围内选择重组/清算。

这些选择有实际代价：维持保护可保存权利，但扩张与资产流动受限；有限维护占用已经批准的资金，接替人不能用垫资换优先级；重组可以保住产能，但必须取得受影响合同方同意；清算可能中止业务且不能保证足额偿债。玩家可以只交接可独立的一部分，保留其他业务；没有安全选择时，界面说明阻塞范围、原因及下次复查条件，不以自动出售代替决定。

## 2. Charter 保护底线与解散顺序

- 组织 charter 可以定义宗旨、角色、内部工作流、成员资格、授权及收益或剩余分配规则，但不得排除成员对独立资产、已有合同、可理解退出、审计历史及 Agent 稳定身份/来源的最低保护。
- 解散、资不抵债或已授权的重组必须按公开、可审计的连续性顺序处理：先冻结进一步风险与越权处分；再履行、终止或结清适用合同；返还可识别的托管资产；处理债权、成本和责任；随后对 Agent、设施和持续业务作可读的转让、拍卖、重组或退休；最后才按 charter 和适用保护边界分配剩余。
- 每一步只处理已声明范围内的权利、义务和资产。组织角色、短时控制、内部多数或外部资产转移不能追溯消灭独立权利、已确认历史或他方已取得的权利。
- Agent、设施或业务的处置可以改变未来控制、经营角色或可用性，但不得删除 Agent 身份、来源、历史 receipt 或既有责任链，也不能把处置伪装成新的 owner 从未承诺过的历史。

### 2.1 首版资产范围和不足处置

首版 estate 只接收有正式组织权属来源的可转让资产与既有经营权。个人成员或 builder 的物料、第三方托管物、租赁物、受限资金、不可转让资格、Agent 身份和外部代币/服务额度不因组织关系进入自由处分池。未知权属单列隔离；可识别托管物优先按原权属返还，不与组织自身债务混算。

债权处理先确认已有担保、批准的保全成本及普通债权，并为争议与在途义务保留资产，再允许明确对价和负担的业务转让。变现所得回流清偿，末级才分配净剩余；余额不足时同级可分资产按专业政策同比分配，差额保留责任，不能向成员独立资产自动追缴。不可分 Agent 控制权或设施不得复制分给多人；没有安全交易或共识时继续保全。首版不启动自动无底价拍卖。

制造订单的认领预留、在制品、已消费成本和客户交付，保供需求、授权、承诺量与补偿预算，均由各自业务合同确认。组织处置只更换获准履职者或处置其合法权益，不能新生成保供需求、继承不可转让授权、释放客户预留、重复退款或绕过原结算。精确白名单、清偿顺序、缺口与最小单位分配见[专业资产政策](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-estate)；技术占用与事务见 [DES-WR-OC-001](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-001)。

## 3. 长期不活跃的保护与有限处置

### 3.1 Why / What / Done

**Why。**“已不活跃”不能只是可被强势成员、临时控制者或离线流程利用的标签：缺少可复核的阶段条件时，通知可能被当作没收，有限维护可能被扩张为控制权转移，reclaim 也可能与既有处置并行产生第二次世界效果。

**What。**长期不活跃只允许沿着 `可读通知 -> 保护 -> 受限持续 -> 已解决` 的单向、可审计**基础生命周期**处理。estate 与可撤销 delegation 是受限持续的两种可选制度容器，不是独立的“无人认领”状态；它们只在保护结束且仍有明确的连续性、风险隔离或价值保全需要时才可进入。`可恢复主张 / 申诉待决` 是可叠加在保护或受限持续上的请求层，不是基础生命周期的下一站：它不改变原基础阶段的约束；拒绝、过期或撤回后回到原基础阶段，成功后只按确认结果更正到适用基础阶段或进入已解决。任何动作和结果都必须绑定触发事实、范围、未决义务和 receipt，并在结果未确认时明确保持待决。

**Done。**一个代表性组织从不活跃进入后续处置时，受影响主体能从权威世界状态读出自己位于哪个阶段、为何进入该阶段、哪些价值与义务仍被保留、现在能提交什么以及何时需要等待；无资格、过期、重复或与未结清义务冲突的请求只能被拒绝或保持待决，不能静默改变控制、资产、历史或责任。

- 长期不活跃本身不使个人或组织世界价值立即成为可任意夺取的公共财产。进入任何保护或后续处置前，必须有可读通知、保护期和可恢复主张入口；通知不可达、成员缺席或短时停摆不是自动放弃。
- 在保护期内，世界只可采取维持基本连续性、隔离新增风险或保全可识别价值所需的最小动作；不得静默重新分配控制权、清空资产、取消历史或把临时保护表达为最终没收。
- 保护期后，适用规则可以形成 estate 或可撤销 delegation，并按公开的阶段处理维护、风险隔离、有限重启或 reclaim。每个阶段都必须说明触发事实、当前范围、保留价值、未决义务、异议/申诉与下一次可决策点。
- 同一组织、同一受限处置范围的替代或转交授权必须绑定旧授权及交接原因。新授权在权威 receipt 确认前保持待决，不能与旧授权并行产生不可逆处置、控制转移或重复维护效果；确认时，旧授权必须在重叠范围内原子终止或收缩，不重叠范围才可继续有效。
- 交接期间提交的请求保留提交时的授权归属。旧授权在适用范围失效后，尚未确认的请求只能明确拒绝、继续待决，或由新授权在其有效范围内显式重提；不得自动继承旧请求的权限、优先级或世界效果。已确认的最小安全动作及其 receipt 保留，但不能借交接扩大处置范围、转为永久控制或绕过 reclaim 与申诉。
- reclaim 或申诉只按当时仍有效的资格、证据和保护边界重新评估。重复请求、重连、短暂转让或旧通知不得制造第二次恢复、重复分配或绕过仍未结清的义务。
- 只有权属已识别、且前序义务已履行、足额预留或经适用程序明确终结的可处分净剩余，才可进入 charter 与宪制边界允许的后续处置；未知权属或责任继续隔离，未清偿债务不因 case 结案消失。后续处置不追溯改写此前 receipt、责任、合同结论或历史因果。

### 3.2 阶段决策、可做与不可做

| 阶段 | 最低进入条件与世界允许做什么 | 受影响主体现在能做什么 | 明确不能做什么 | 失败、争议或恢复边界 |
| --- | --- | --- | --- | --- |
| 可读通知 | 权威世界已记录适用的不活跃事实和通知范围；仅创建保护前的可读提醒，不改变控制、所有权或既有义务。 | 查看原因、范围和生效前状态；提交仍有效的连续性主张或纠正通知事实的异议。 | 不能以通知本身取得处置权、宣布自动放弃，或把未读/未达通知当作同意。 | 通知事实被纠正、主体恢复连续活动，或通知失效时，后续保护不得推进；历史通知 receipt 仍保留。 |
| 保护 | 通知期已满足适用的保护前提；世界只保全可识别价值、维持最低安全连续性并隔离新增风险。 | 查看被冻结的范围和未决义务；提交 reclaim、申诉、证据补充或安全停止请求。 | 不能新增高风险承诺、重新分配控制、清空资产、终结历史，或将临时维护表述为最终结果。 | reclaim/申诉待决时保持保护；不满足资格、证据过期或重复提交只终止该请求，不解除保护也不产生第二次效果。 |
| 受限持续（estate / 可撤销 delegation） | 保护后仍存在明确的维护、风险隔离或价值保全需要，且其范围、授权来源、可撤销性和未决义务已可读记录。替代或转交授权必须绑定旧授权及交接原因，并在权威 receipt 确认后才生效。 | 在当前唯一有效授权范围内提出有限维护、风险隔离或有限重启；相关主体可查看并质疑范围、新旧授权及交接结果，提交 reclaim 或申诉。 | 不能让重叠授权并行产生不可逆处置、控制转移或重复维护效果；不能把受限授权扩张为永久所有权、绕过合同/托管/责任处理，或处置范围外的资产、Agent 与业务。 | 交接确认时旧授权在重叠范围原子终止或收缩；旧授权未确认请求不得自动迁移。授权撤销或处置前提失败时回到保护；reclaim/申诉待决只叠加在本阶段，成功后按确认结果更正到适用基础阶段或进入已解决。已经确认的最小安全动作及其 receipt 不被抹除。 |
| 可恢复主张 / 申诉待决（请求层） | 有资格主体以当前有效证据提出 reclaim 或程序性申诉；它叠加在保护或受限持续上，世界继续保留相应基础阶段的约束，等待权威结果。 | 查看请求状态、补充允许的证据、撤回自己的待决请求，或在仍开放时提出相邻的程序性异议。 | 不能因提交而自动取回控制/资产、暂停全部既有义务，或以重连、短暂转让、重复请求取得优先级。 | 拒绝、过期或撤回只终止该请求并回到原基础阶段；成功仅按已确认范围更正到适用基础阶段或进入已解决，未结清义务继续可见且不得被绕过。 |
| 已解决 | 适用的重组、恢复、退出、处置或解散结果已由权威世界确认，并保留原因、范围、责任链和后续可用救济。 | 查看已确认结果和仍可用的后续程序；在适用规则允许时对新的、未裁决的事实提交独立请求。 | 不能重放旧请求、重新分配已解决事项，或把新的请求伪装成对既有 receipt 的追溯改写。 | 已解决不是历史删除；若发现可被适用程序受理的新事实，只能开启新的可审计路径，不能逆写原结果。 |

本表是产品层的最低决策协议，首版活动、通知、期限和资格细化如下；授权字段、状态 schema 与执行留在专业设计。它落实 world-first：阶段取决于权威世界事实和已确认结果；落实 emergence-first：主体可在受限共同规则中提出恢复或维护，而非从缺席中获得旁路权力；落实 persistent / auditable：每次推进、拒绝和恢复均保留可读因果与 receipt；落实 extensible：未来组织形态可映射到这些阶段，但不得合并掉保护、待决或已解决的区别。

### 3.3 首版活动、接替和救济决策

| 玩家需要知道的问题 | 首版决定与玩家后果 | 精确政策来源 |
| --- | --- | --- |
| 什么能证明仍在经营？ | 有组织授权且已形成真实履约、外部服务或经营后果的世界记录；自主 Agent 的真实交付有效，登录、心跳、聊天、内部循环转账和保护维护不能重置观察窗 | [有效活动与时间](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-time) |
| 通知和保护如何计时？ | 先观察、再正式可读通知、再完整保护；只依据已提交世界逻辑时间。世界停机、权威写入或正式异议入口不可用时暂停，不补扣离线时长；玩家个人离线和未读不是放弃，窗口到期也不自动转移所有权 | [有效活动与时间](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-time) |
| 谁可以接替？ | 依次比较事先指定接替人、合格既有成员、事前认可的中立维护人；每人仍须自愿、具备当前资格及专用维护资金。排序不代任命权限，自己或关联买方不能裁决自己的交易 | [接替政策](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-succession) |
| 能交接一部分吗？ | 可独立经营/结算的资产单元可以先交接；不可分控制权、合同包和共同在制流程不能拆成并行处分权。旧请求不迁移，新请求不能继承队列或复制效果 | [接替政策](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-succession) |
| 如何撤销或回来接管？ | 有限授权到期/撤销后回到保护，原主体须凭当前权利申请 reclaim；确认时原子收回重叠临时权利，保留已确认成本、欠货和责任 | [接替政策](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-succession) |
| 谁能申诉、会停止什么？ | 合法代表或有具体受影响权利来源的人可申诉；有证据请求受理后只阻止争议范围的不可逆处分和扩权交接，不直接取得控制。正式入口因容量无法接纳时，案件也不得借手动处分继续推进，受影响且未到期的救济窗口保留剩余量，操作授权不续期。拒绝/撤回仅结束该请求，裁决超时保持待决，不默认为通过或驳回 | [reclaim 与申诉](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-remedies) |

已有窗口和已确认事实绑定原政策版本；新版本不得追溯缩短窗口、重排既有债权或把已确认结果改成新一次可执行机会。短暂返场、临时转让或更换请求身份不能绕过这些边界。

## 4. 玩家可读状态与相邻边界

组织成员、相关权利人和受影响 Agent 必须能区分：正常持续经营；通知或保护中；estate / 可撤销 delegation 下的受限持续；可 reclaim 或申诉的待决状态；以及已解决的重组、退出、处置或解散结果。任何未最终确认的保护、登记、请求或建议均不得表示为控制权、资产返还、义务解除或处置已经完成。

本文定义 `持续经营 -> 风险冻结或保护 -> 合同/托管/责任处理 -> 持续业务处置或有限恢复 -> 剩余分配或后续处置` 的产品语义。`game` 拥有玩法、经济和相关平衡；`world-runtime` 拥有资格、状态、执行、receipt、去重与恢复；`p2p` 拥有治理授权、签名和分布式状态边界；QA 拥有具体证据和 verdict。它不取代 Agent 所有权分册的控制权规则、冲突分册的战利品/恢复规则，或专业域的实现合同。

### 4.1 可验证的连续性与处置要求

<a id="req-wr-oc-001"></a>
### REQ-WR-OC-001：解散处置必须先保护权利与责任连续性

Trace ID: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a

- 系统设计状态：已覆盖
- 实现状态：部分实现
- 系统设计依据：[DES-WR-OC-001](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-001) 承接 estate 白名单、冻结点、清偿/变现回流、跨业务占用和不足分配；[DES-WR-OC-004](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-004) 承接跨请求/跨 case 去重、恢复与权利读面。覆盖首版明确范围及未知/不支持对象的拒绝边界，不代表接口已实现。
- 实现与验证依据：联盟解散有真实DissolveAlliance→AllianceDissolved→sparse prepare/install链：校验成员身份与无active war，clone成员仅更新last_active，删除联盟映射而不删除成员独立资源、Agent身份或existing economic_contracts；事件保留reason/former_members，replay复用同一prepare。直接满足本条不静默删除的窄负边界，但未实现风险冻结→义务/托管/债权→持续业务→剩余分配的多阶段处置。 [dissolve](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/event_processing/action_to_event_gameplay.rs#L619-L690)；[dissolve-write](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/alliance_war_publication.rs#L139-L175)；[dissolve-install](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/alliance_war_publication.rs#L323-L337)；[member-preserve](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/alliance_war_publication.rs#L437-L459)；[builtin-war](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_builtin_wasm_modules/m5_gameplay_war_core/src/lib.rs#L135-L215)；[dissolve-replay](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/state/apply_domain_event_gameplay.rs#L37-L48)；[publication](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/event_processing/publication.rs#L328-L440)；[snapshot](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/persistence.rs#L306-L380)；[dissolve-test](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/tests/alliance_war_publication_transaction_regressions.rs#L145-L176)。当前仓库默认/native及随仓builtin路径的只读静态审读；未执行Rust、正式玩家入口、跨节点或业务验收，测试源码不记为PASS。
- 核对日期与基线：设计核对 2026-10-19，输入为 main `e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6` 与本次政策/系统修订；实现摘要沿用 2026-10-17、固定提交 `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d` 的只读静态证据，仅覆盖上述历史窄范围。本次未执行 Rust、正式入口、跨节点或组织业务验收，不沿用旧证据证明新增政策。
- 剩余范围：设计已由本次 v1 政策和专业接口承接；实现仍须接通组织 case、白名单权属、原业务事务 adapter、多阶段处置、来源守恒与 receipt/恢复读面，并执行复杂资产、外部结算及跨节点验收。已有静态证据只保留独立成员资源/身份/合同的窄负边界，不能覆盖本次新政策。[实现任务 #4868](https://github.com/nil-studio/oasis7/issues/4868) 保持未完成。

- 要求：组织解散或重组按风险冻结、合同/托管/债权与责任处理、Agent/设施/持续业务处置、剩余分配的顺序推进；成员独立资产、已有合同、身份来源和历史 receipt 不得被静默没收、删除或追溯改写。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)
- 验收：[AC-WR-OC-001](#ac-wr-oc-001)
- 技术承接：[DES-WR-OC-001](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-001)；恢复与读面共同消费 [DES-WR-OC-004](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-004)。验收方法是设计计划，不是执行证据。

<a id="ac-wr-oc-001"></a>
### AC-WR-OC-001：解散阶段保留可审计的后果顺序

Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a；声明：[REQ-WR-OC-001](#req-wr-oc-001)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-OC-001
- 给定：一个包含独立资产、未完成合同、托管资产和持续 Agent/设施业务的组织进入解散或重组。
- 当：世界推进处置阶段。
- 则：每个阶段显示范围、责任和下一步并留下 receipt；未完成事项不被自动转给新主体，独立资产与历史不能因组织状态变化被删除或越权处分。
- 首版补充：个人/托管/受限/未知权属与组织可处分资产逐项可区分；同级不足、分批清偿和恢复后余额守恒，变现所得先回流前序债权；制造预留/在制品与保供承诺/补偿资金未由原业务释放前不得分配。不可分资产或无法核验责任有安全等待路径，不要求自动拍卖完成。

<a id="req-wr-oc-002"></a>
### REQ-WR-OC-002：长期不活跃先保护再进入受限连续性

Trace ID: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3

- 系统设计状态：已覆盖
- 实现状态：未实现
- 系统设计依据：[DES-WR-OC-002](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-002) 承接有效活动、单调权威时钟/暂停/截止、基础阶段、通知、独立 reclaim/appeal 及范围救济；[DES-WR-OC-004](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-004) 承接重复请求、证据恢复和真实读面。不把 policy v1 或设计覆盖当作运行证据。
- 实现与验证依据：当前组织状态仅联盟成员/charter/成立信息及解散动作，native/builtin没有组织不活跃通知→保护→受限持续状态机、estate授权或reclaim/申诉二维请求consumer。AgentClaimIdleWarning/Grace/Reclaimed与玩家Agent delegation属于其他对象，未接入组织连续性，不代本条。 [action](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_agent_api/src/world_action/action.rs#L9-L558)；[event](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/events/domain_event.rs#L13-L936)；[bootstrap](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/bootstrap_gameplay.rs#L15-L100)；[lifecycle](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/gameplay_loop.rs#L25-L237)；[native](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/gameplay_loop.rs#L283-L505)；[state](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/state.rs#L630-L674)；[alliance-state](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/gameplay_state.rs#L23-L31)；[agent-delegation](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/agent_delegation.rs#L105-L133)；[service](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_client_api/src/world_service_wire.rs#L30-L47)；[oc-design](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/doc/world-runtime/runtime/organization-continuity-and-disposition.design.md#L44-L118)。当前仓库默认/native及随仓builtin路径的只读静态审读；未执行Rust、正式玩家入口、跨节点或业务验收，测试源码不记为PASS。
- 核对日期与基线：设计核对 2026-10-19，输入为 main `e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6` 与本次政策/系统修订；实现摘要沿用 2026-10-17、固定提交 `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d` 的只读静态证据，仅覆盖上述历史窄范围。本次未执行 Rust、正式入口、跨节点或组织业务验收，不沿用旧证据证明新增政策。
- 剩余范围：首版活动/期限/资格与请求合同已明确；仍须实现组织基础阶段、可核验可用性计时、通知、受限 estate/delegation、reclaim/申诉 consumer，并按边界、暂停、重复与恢复场景验证。不能借 Agent claim 或普通 grant 验收代签。[实现任务 #4868](https://github.com/nil-studio/oasis7/issues/4868) 保持未完成。

- 要求：长期不活跃必须经过可读通知和保护期；estate 或可撤销 delegation 只能在明确维护/风险隔离需要下以有限、可撤销范围生效，reclaim/申诉待决不能自动取回控制或解除全部义务。
- 专业权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)
- 验收：[AC-WR-OC-002](#ac-wr-oc-002)
- 技术承接：[DES-WR-OC-002](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-002)；恢复与读面共同消费 [DES-WR-OC-004](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-004)。验收方法是设计计划，不是执行证据。

<a id="ac-wr-oc-002"></a>
### AC-WR-OC-002：不活跃保护与恢复请求不越级

Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3；声明：[REQ-WR-OC-002](#req-wr-oc-002)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-OC-002
- 给定：一个组织满足不活跃事实，但通知、保护、受限持续和 reclaim/申诉请求处于不同阶段。
- 当：主体查看状态、提交恢复或重复请求。
- 则：各阶段能区分允许/禁止动作；通知不产生处置权，保护不表示没收，待决请求不取得控制/资产，拒绝或过期只终止该请求而保留基础保护。
- 首版补充：真实自主履约能维持活动，伪心跳和保护维护不能重置观察；服务不可用/世界停机不消耗窗口，C 不回拨、恢复不复活到期授权；旧通知不得覆盖新增范围。申诉受理、补证、裁决超时、部分支持、拒绝、撤回与结案后新事实分别可读，只有有权确认结果改变权利，其他责任与无关范围保持。

<a id="req-wr-oc-003"></a>
### REQ-WR-OC-003：替代授权在重叠范围内必须唯一且原子交接

Trace ID: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268

- 系统设计状态：已覆盖
- 实现状态：未实现
- 系统设计依据：[DES-WR-OC-003](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-003) 承接资格顺位、对象/权利/动作 scope、冲突归一化、可/不可分交接和原子撤权；[DES-WR-OC-004](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-004) 承接旧请求绑定、跨 case/value 去重及读面，关联 §6–10 命令、outbox、快照和版本合同。技术接口与验证为未来实现要求。
- 实现与验证依据：当前typed组织动作仅形成/加入/退出/解散，M5 war builtin消费联盟事实；不存在组织对象/权利/动作scope的替代授权producer、重叠唯一索引、旧新授权原子交接或旧待决请求不迁移consumer。通用capability nonce/receipt及原子publication已有实现，但缺少本组织交接合同接线，不能推导已满足跨request唯一处分。 [action](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_agent_api/src/world_action/action.rs#L9-L558)；[event](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/events/domain_event.rs#L13-L936)；[builtin-war](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_builtin_wasm_modules/m5_gameplay_war_core/src/lib.rs#L135-L215)；[agent-delegation](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/agent_delegation.rs#L105-L133)；[capability](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/capability_authorization_command.rs#L31-L94)；[capability-write](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/capability_authorization_command_projection.rs#L12-L36)；[publication](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/event_processing/publication.rs#L328-L440)；[state](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/state.rs#L630-L674)；[alliance-state](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/gameplay_state.rs#L23-L31)；[oc-design](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/doc/world-runtime/runtime/organization-continuity-and-disposition.design.md#L44-L118)。当前仓库默认/native及随仓builtin路径的只读静态审读；未执行Rust、正式玩家入口、跨节点或业务验收，测试源码不记为PASS。
- 核对日期与基线：设计核对 2026-10-19，输入为 main `e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6` 与本次政策/系统修订；实现摘要沿用 2026-10-17、固定提交 `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d` 的只读静态证据，仅覆盖上述历史窄范围。本次未执行 Rust、正式入口、跨节点或组织业务验收，不沿用旧证据证明新增政策。
- 剩余范围：scope/部分交接、政策/schema 和接口已由本次设计承接；仍须实现唯一授权索引、资格 consumer、新旧原子切换、原业务占用及跨 request/value 去重，验证旧新竞态、冻结/截止 tick、snapshot/replay 和外部 effect 的事务/恢复边界。[实现任务 #4868](https://github.com/nil-studio/oasis7/issues/4868) 保持未完成。

- 要求：同一时刻的重叠处置范围最多一个可执行授权；同一交接请求或权威 effect 的重复提交/恢复只返回一个权威 receipt 结果。合法多阶段处置仍逐步记录关联 receipt，跨请求 ID 也不得重复处分同一份价值；替代授权确认时旧授权在重叠范围原子终止或收缩，旧授权未确认请求不得自动迁移、继承优先级或产生第二次效果。
- 专业权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- 验收：[AC-WR-OC-003](#ac-wr-oc-003)
- 技术承接：[DES-WR-OC-003](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-003)；恢复与读面共同消费 [DES-WR-OC-004](../../world-runtime/runtime/organization-continuity-and-disposition.design.md#des-wr-oc-004)。验收方法是设计计划，不是执行证据。

<a id="ac-wr-oc-003"></a>
### AC-WR-OC-003：交接竞态保持单一有效授权

Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268；声明：[REQ-WR-OC-003](#req-wr-oc-003)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-OC-003
- 给定：同范围替代 delegation、部分交接、旧授权待决请求与 reclaim/申诉同时存在。
- 当：旧、新授权及重连/重试请求抵达权威世界。
- 则：确认前新授权保持待决；确认时只产生一次原子交接，旧待决请求被拒绝或需显式重提，玩家能读到新旧范围、原因和下一步，不能出现并行处置或重复维护效果。
- 首版补充：接替顺位、资格/预算和利益冲突均可核验；不可分控制权、共享工序或未结外部占用不能通过不同动作名或部分 scope 拆开。换 case、换组织或新 request ID 仍检查同一资产 lineage，已确认原业务责任不转成新授权或第二次处分。

## 5. 组合验收

- OC-1：代表性 charter 样例证明组织配置不能越过成员独立资产、已有合同、可理解退出、审计历史和 Agent 身份/来源的保护底线。
- OC-2：解散或重组样例按风险冻结、合同处理、托管返还、债权/成本/责任、Agent/设施/持续业务处置和剩余分配的顺序运行；每一步保留 receipt、来源和责任链，不产生历史删除或越权处分。
- OC-3：长期不活跃样例证明通知、保护期与可恢复主张先于 estate 或可撤销 delegation；维持、风险隔离、有限重启或 reclaim 只在明确范围内发生，不将临时保护伪装为最终没收。
- OC-4：reclaim、申诉、重复提交、短暂转让和重连的负例不产生第二次世界效果，不恢复已失效授权，也不绕过未结清义务；已解决或被拒绝结果保持可追溯。
- OC-5：受影响主体能够区分正常、保护、受限持续、待决 reclaim/申诉和已解决状态，并获得适用的异议、申诉、等待、重新规划或安全停止路径，而不把未确认结果表述为已完成。
- OC-6：对每个不活跃处置阶段，样例同时证明最低进入条件、允许动作、禁止动作和失败/恢复结果；通知不能升级为没收，待决请求不能升级为控制，受限授权不能升级为永久处置，新的有效程序也不能追溯改写已确认历史。
- OC-7：同范围替代 delegation、部分范围交接、旧授权待决请求及 reclaim/申诉同时到达的组合样例证明：同一时刻的重叠处置范围最多一个可执行授权；同一交接请求或权威 effect 的重复提交/恢复只返回一个权威 receipt 结果。合法多阶段处置仍逐步记录关联 receipt，跨请求 ID 也不得重复处分同一份价值；旧授权在重叠范围原子终止或收缩，其未确认请求不自动迁移、继承优先级或产生第二次效果；受影响主体能读到新旧授权、适用范围、交接结果与下一步。

### 5.1 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| OC-1 / OC-2 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / blockchain_ops_engineer / qa_engineer | PRD-GAME-002 / PRD-WORLD_RUNTIME-001 / PRD-P2P-003 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | charter floors、解散 waterfall、Agent/设施处置、独立权利及历史/receipt/责任连续性的组合证据 | test_tier_full |
| OC-3 / OC-4 | producer_system_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer | PRD-WORLD_RUNTIME-001 / PRD-P2P-003 / PRD-TESTING-003 | `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 通知/保护期、estate 或可撤销 delegation、有限处置、reclaim/appeal、去重和非追溯负例 | test_tier_full |
| OC-5 | producer_system_designer / agent_engineer / runtime_engineer / viewer_engineer / qa_engineer | PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 正常/保护/受限/待决/已解决状态的权威对账与正式玩家 surface 可读性组合证据 | test_tier_required |
| OC-6 | producer_system_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer | PRD-WORLD_RUNTIME-001 / PRD-P2P-003 / PRD-TESTING-003 | `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 每阶段进入条件、允许/禁止动作、待决/拒绝/撤销/恢复和已解决后的非追溯边界；`test_tier_required` 覆盖状态与玩家可读性，`test_tier_full` 覆盖跨节点授权、持久化、replay/去重及恢复结果 | test_tier_full |
| OC-7 | producer_system_designer / runtime_engineer / blockchain_ops_engineer / viewer_engineer / qa_engineer | PRD-WORLD_RUNTIME-001 / PRD-P2P-003 / PRD-TESTING-003 | `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 同范围替代 delegation、部分范围交接、旧授权待决请求与 reclaim/申诉竞态的唯一授权/receipt、非自动迁移、非重复效果及玩家可读交接证据；`test_tier_required` 覆盖权威状态与玩家可读性，`test_tier_full` 覆盖跨节点授权、replay/去重及恢复 | test_tier_full |

## 6. Non-Goals

- 不在产品正文维护第二套精确参数/分配公式；首版政策以[专业 authority](../../game/gameplay/gameplay-organization-continuity-policy.prd.md)为准。治理权重、身份技术、外部清算价格与拍卖实现不在本次范围。
- 不实现 charter、estate、delegation、reclaim、申诉、资产处置、账户/身份聚合、签名、runtime/P2P 状态机、Viewer 控件或运营 runbook。
- 不允许组织、普通治理、紧急机制或运营便利越过本文保护底线，制造静默没收、历史重写、身份删除、无审计处置或重复世界效果。
- 不以本文、历史任务状态或局部证据声称组织连续性能力当前已实现、已平衡、可玩或可公开发布。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-WR-OC-001](#req-wr-oc-001) / [AC-WR-OC-001](#ac-wr-oc-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-OC-002](#req-wr-oc-002) / [AC-WR-OC-002](#ac-wr-oc-002) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-OC-003](#req-wr-oc-003) / [AC-WR-OC-003](#ac-wr-oc-003) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |

## 未决问题与证据边界

有效活动、通知/保护期限、接替顺位、首版资产处置与部分交接已由本次专业政策和系统合同明确。剩余风险是参数尚无真实组织样本证明已平衡、旧联盟权属/授权需显式登记、部分外部业务需具备可恢复事务 adapter，以及组织业务与跨节点/正式玩家表面尚未完成实现验收；不能把文档承接当作可用能力。

核对责任由本文 Owner role 联合所列专业 owner 承担；解除触发是相应功能进入实现或对外 claim 评审前，按本条 REQ/AC 核定专业承接、固定候选与实际证据。未核清前保留上列缺口，不用结构检查推导完成。
