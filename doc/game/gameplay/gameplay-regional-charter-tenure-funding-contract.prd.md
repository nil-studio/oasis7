# Gameplay 区域 charter、tenure 与公共融资合同

- `PRD-ID`：`PRD-GAME-017`
- 上层产品映射：本合同承接产品 [`REQ-WR-GR-004`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004) / [`AC-WR-GR-004`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-004)、[`REQ-WR-GR-005`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-005) / [`AC-WR-GR-005`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-005)、[`REQ-WR-GR-006`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-006) / [`AC-WR-GR-006`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-006)、[`REQ-WR-GR-007`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007) / [`AC-WR-GR-007`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-007)、[`REQ-WR-GR-008`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-008) / [`AC-WR-GR-008`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-008) 的产品结果；产品专题拥有区域 charter、tenure 与 levy 的产品承诺，本文件拥有玩家动作、取舍、失败恢复、申诉和可玩性验收。
- 主题 authority：本文件是 `game` 专业域中区域成立/调整、双资格约束、退化/恢复、tenure 长期规划与服务费/公共 levy 玩家语义的详细 authority；不把 `PRD-GAME-015` 的成熟世界成长路线扩展为 charter、tenure 或 levy 的完整合同。
- 专业边界：`world-runtime` 拥有空间、资格、bond、tenure、费用、receipt、状态与确定性恢复；`p2p` 拥有身份、治理授权/签名和分布式状态；`doc/testing/prd.md` 与 QA 拥有组合验证证据。产品专题仍拥有 `GR-004` 至 `GR-008` 的稳定产品要求。
- 设计适用性：`simple-topic-exemption`（`PRD-only-sufficient`）。本文件承载 Why / What / Done、玩家循环、机会成本、失败恢复和验收，tenure 的政策时界关系及适用补偿由 §2.1～2.3 拥有；不新增 API、schema、技术状态机、回滚算法、生产参数取值或 UI 布局；配对例外在 [`doc/game/prd.index.md`](../prd.index.md) 登记。
- 当前执行：可变 task 状态与当前实现证据由 Git、PR、实际 CI 和评审记录 拥有；本文件不宣称当前已实现、已平衡、已发布或已获得 release readiness。
- 产品锚点状态：本合同的 `GR-004` 至 `GR-008` 语义均直达 active 产品专题的 `REQ/AC` 精确锚点；本文件只消费这些产品边界，不把链接存在写成当前实现、可玩性或 release 证据。

## 1. 目标与范围

区域 charter 是一项有边界的共同经营承诺，不是任意圈地、组织标签或全局主权。玩家应能从能力/需求/边界证据出发，理解本地审查、邻区异议与全局保护的分工，再决定是否投入一次有限的提案、服务或恢复行动。

本合同覆盖五个玩家侧问题：

1. 如何提出或调整一个有证据的区域 charter，并理解审查结果和下一步。
2. 如何在受限治理资格与持续本地贡献之间作出可读的经营取舍，而不取得任意排他或全局控制。
3. 区域退化时如何比较恢复、暂停高影响权限和回归未成熟区域的后果。
4. 如何取得、维护、转让或面对收回一个地点 tenure，并为建设/服务做可读的长期规划和申诉。
5. 如何区分可排他服务的自愿费用与不可排他公共品的有界 levy，并在授权失效时保持独立路线。

本合同不复制产品专题的世界制度，也不把下层执行能力写成当前玩家可用能力。`PRD-GAME-015` 只继续拥有 `local operator -> regional specialist -> limited-scope regional influence` 的成熟世界成长轴；本合同提供该成长轴遇到区域制度时的具体玩家动作边界。

## 2. 玩家循环与动作语义

所有预览和提案都是只读的：它们不预留资源、地点、容量、资格、队列顺位或权力。玩家确认后，权威执行必须按当前事实重新校验，最多产生一次有作用域的 receipt；报价或授权漂移时原子拒绝、过期或要求显式重提，不产生部分扣减或隐藏义务。

| 阶段 | 玩家可做什么 | 玩家得到什么 | 主要成本、失败与恢复 |
| --- | --- | --- | --- |
| charter 提案与审查 | `inspect_charter_evidence`、`propose_or_adjust_charter`、`compare_local_neighbor_global_review`、`respond_to_scope_objection` | 看见能力、需求/交付、Agent/资源、边界理由、bond 的证据缺口，以及本地、邻区、全局各自负责什么 | 付出证据准备、机会窗口和可能的 bond/服务投入；缺项、自报边界、邻区通行异议或执行前漂移时补证、缩小范围、改走独立服务路线或重提；提案通过不等于获得权力 |
| 成熟区域日常治理 | `inspect_charter_scope`、`submit_local_service_or_policy`、`compare_governance_and_contribution`、`challenge_overreach` | 看见受限治理资格、持续贡献、实际影响范围、通行与独立恢复路径，及本次事项的下一决策点 | 经营、维护、交付和协调机会成本；资本、短时到访或历史头衔不足时不能独占控制；范围外或触及保护的事项拒绝/转入独立轨道 |
| 区域退化与恢复 | `inspect_degradation_reason`、`repair_region`、`rebuild_service`、`pivot_local_use`、`request_recovery_or_appeal` | 比较观察/恢复、暂停高影响权限、解散/回归未成熟区域的阶段后果；保留资产、身份、合同和历史的下一步 | 短时失败不立即抹除历史；持续退化会减少高影响动作；恢复失败时可回到独立路线、规则化迁移/回收/重建或申诉，不能用技术停机伪装为永久解除 |
| tenure 规划与权利变化 | `inspect_tenure_plan`、`acquire_tenure`、`renew_tenure`、`transfer_tenure`、`request_tenure_recovery_or_appeal` | 看到 tenure 的建设/服务用途、维护与实际使用条件、规划 horizon、续期/转让条件、公共通行影响和失效后的替代路径 | 投入维护、使用和规划机会成本；规划是可读的未来安排，不保证资源、收益或永久主权；到期、长期闲置、明确违约或预定义公共必要性触发规则化收回时，通知、理由、申诉和迁移/回收/补偿路径必须可见 |
| 服务费与公共 levy | `compare_service_fee_and_levy`、`inspect_beneficiary_and_accounts`、`accept_or_decline_service`、`pay_valid_fee`、`challenge_invalid_levy`、`choose_independent_or_alternative_route` | 区分受益对象、用途、成本、服务范围、上限/预算、期限、账目、复核与退出/替代路径 | 可排他服务可由透明使用/维护费或自愿合约支付；levy 只有完整授权和公共品条件时才可结算；无效 levy 不扣费、不累积欠费、不限制资格/通行，玩家回到常态服务、补证、申诉或独立恢复 |

每个阶段都必须让玩家回答：现在改变了什么、承担什么、失败会保留什么、下一次回来做什么。一个提案、投票、授权、排队或本地缓存不能被表达为已生效的世界事实。

<a id="tenure-policy"></a>
### 2.1 地点 tenure 的完整政策

Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186；承接 [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)。本节冻结目标玩法政策；[登记与处分设计](../../world-runtime/runtime/location-tenure-and-disposition.design.md#tenure-identity)负责执行，未据此形成当前土地登记或玩家可用能力。区域 charter 的成立、资格和退化仍沿原 GR-004～006。

**取得与规划。** 地点 tenure 只授予列明空间和用途的建设/服务权。普通通行、未列入的资源、独立设施/库存、Agent 身份、其他人的合同和历史均不随 tenure 转让。提案须给出精确地点、用途、当前权利人、全部受影响权利和已验证的空闲/可交接状态；玩家接受的条款包含开始/结束时界、维护周期、实际使用判据、可续期范围、移交限制、收回程序和费用来源。没有明确期限、维护标准或实际可行的退出方案就不能取得。权利的登记与实际腾空是两件事，不能通过签发新证书覆盖仍有合法资产的地点。

所有时界使用已提交的有效世界时间，不能由客户端钟、离线时长或停机后的批量补时推定过期。政策给定正整数响应单位 `Q`、每次规划长度 `H`、维护周期 `M` 和最大累计规划上界，须满足 `H >= 8Q`、`Q <= M <= H`；取得时明确显示其 tick 与当前世界时间解释。生产取值由当前有权的区域政策发布并由权利人接受，不从手算样例推广；缺合法值拒绝。续期只能在原结束前 `4Q` 窗口内申请，重核真实使用/维护、当前资格、用途和未决处置；每次至多增加原政策允许的一个 `H`，不能靠提前连点形成永久占地。旧期限不因申请、付款待决或复核自动延长。

维护有明确原对象、实际消耗及 receipt；付款或送达意图不等于维护已完成。`used` 来自原声明服务/建设的真实结果，驻留、登录、刷请求、临时搬入物品或重复计同一 receipt 不算使用。一个完整 `M` 内没有合格使用且没有政策承认的停机/保护原因，才可形成闲置证据；维修中已履行的维护与真实计划按原声明判据读取，不由地方管理者临时换标准。

**转让。** 当前持有人、受让人对同一用途、剩余期限、维护责任、费用、资产清单和未决义务明确接受；每项真实第三方义务继续由原权利人/合同主责确认是否允许交接。tenure 本身不含资产出售，不隐含接替旧债、移动物品或变更设施 owner。需要一同交接的资产必须另有精确授权及真实领域结果；否则在原清单标记仍归原人并保护其访问/取回。转让不刷新期限、维护周期、既有通知或申诉窗口；存在未决收回或控制权争议时不接受普通转让。quote 不锁地点，两个受让人只能有一个在同一权利版本上成功。

<a id="tenure-recovery-policy"></a>
### 2.2 限定收回、实际迁移与适用补偿

Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186；产品 [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)。

只有原条款列出的到期、完整周期闲置、可证明确违约或预定义公共必要性可提出收回。普通竞争结果、区域退化或多数意见本身不增加第五种理由。另行明确接受的冲突条件转让属于 §2.1 的规则化转让：原 holder 在暴露前同意精确可转移用途、剩余期限、受让资格、持续占有与到期条件以及第三方义务安排；条件实际满足并由原 tenure authority 单次登记后才改变持有人，不能把比分或临时占领当成收回/没收。尚未转让的临时占有沿[冲突规则](gameplay-chartered-conflict-recovery-and-soft-season-contract.prd.md#conflict-occupation-policy)处理；两类都不夺走未列独立资产或公共通行。

收回通知必须列原 tenure/revision、理由和证据、受影响用途、最早停止新增建设时间、取回/迁移方式、独立复核入口及费用/补偿。通知须在拟执行前至少 `4Q` 发到登记的持有人和有直接义务者的持久受权 inbox；送达可核验不要求玩家在线，入口实际不可用则暂停相关程序时钟。自然到期在取得/续期时已有明确结束值，但仍须发出 `4Q` 提醒；提醒缺失不创造无限新建设权，到期后只停新增长期承诺，不能据此提前移走资产或跳过完整处分通知。

处分先冻结新的重叠建设/长期排他承诺，保留必要安全、已承担义务、取回与申诉。停止 tenure 用途不等于对象消失。对清单中的每项资产，玩家可比较：在原地完成合法安全收尾；以真实运输迁至自己有当前权利/容量的目的地；按原设施回收规则拆解并接收实际回收量；或保留所有权转为受保护待取 custody。每项必须有原 owner、条件/损耗、所需电力/材料/容量/时间、真实 payer 和 receipt；本项选择不可处理别人的货物。搬迁、回收未完成的对象保持 `disposition_pending`，保留原位置/库存/风险和下一步，不宣布已腾空。地点已无 tenure 新使用权也不能凭空释放物理占地，后继建设须避开仍占用的空间或等真实迁移完成。

**补偿适用规则固定在取得前。** 自然到期、闲置或本方违约不自动补偿未来利润、原维护费或已发生的消耗；原规则可回收部分仍按实际结果归 owner。公共必要性收回必须在通知前，由提案的真实公共 payer 为已接受的逐资产补偿包和搬迁/结清成本足额保护资金/材料。补偿包采用当时原领域的修复/重建成本报价，最终只扣除已经实际返还给 owner 的可回收部分，各资源分别记账；不把不同材料或 OC/电力相加、不补承诺外收益。owner 可以同版接受包或请求有限复核；未形成可执行包时只保留提案，不得先赶走再承诺支付。实际接收数量与报价不同时，补偿按通知时冻结的单位/上限和真实差额重新核验，超原包须取得新授权与 owner 接受，不能单方压低或创造无预算债务。

不可逆处分前的资金保护按已接受的最大替代补偿计算，尚未实际返还的回收量一律按零抵扣，另加真实搬迁/安全结清费用；预计回收不是可花的钱。结算采用两种明确选择之一：先封存原资产的实际回收结果并完成所有应返还量，再支付净差；或 owner 同版明确接受把原清单内精确残余回收权益换成足额替代补偿，该权益交接与付款同根后，后续对应残值归原 payer。后一选择不转移清单外资产、身份或其他权利，不由普通 tenure 变更推导。每项只有一个补偿/回收结清键，不能先按零回收足额赔付，再把同一受偿片段的残值第二次交给 owner。原结果未知则保留资金和 pending，不选择有利解释。

到原迁移截止仍未完成，程序不会无限续期：停止本 tenure 新用途，逐项确定已完成、保护 custody、缺资源/目的地或原结果未知。受保护 custody 只防篡改与重复支出，不提供免费运输、无限容量或永久经营；占用/保管成本由原已签条款的 payer 与限额承担，缺资金时保持具体未结责任和独立取回途径，不能静默烧毁、转送、扣取不相关余额或伪称结清。公共必要性必须先满足已签补偿/安全接收前置，欠缺时该物理收回保持阻断；其他不重叠公共工作可继续。历史损失、责任与已结算合同始终保留。

<a id="tenure-review-policy"></a>
### 2.3 有限复核与终态

Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186；承接 [AC-GAME-017-04](#ac-game-017-04)。复核不是地方管理者自行批准收回，也不使用战争、服务或 RD panel 的授权。

原登记绑定独立的 `location-tenure.review` 授权，列 world、区域、事项/处分范围、名册、可验证且不可预选的随机来源、费用及期限。申请人、收回提案者、受让人、其实际控制人和直接经济关联者不得担任成员。通知 `N` 后 `2Q` 内登记争点及证据，冻结后用已承诺的随机排列选取三人初审；至 `N+3Q` 至少两人对同一合法处分签署才产生决定。初审后到 `N+4Q` 可提出一次仅关于权限、遗漏原范围证据或程序错误的复核，由不同三人审至 `N+5Q`；普通补证、换申请人或重连不重置原时界。

证据和申诉窗口均为半开区间：登记须在相应截止之前取得 canonical 受理；截止时刻的新请求已超窗，客户端发送时间不回填。初审不是可执行 final：只有申诉窗口已闭、完整原案证明没有已受理待决复核，或有效复核已经完成，才可执行争议项不可逆处分。N+4Q 的执行必须先核全此前已受理的申诉/原未知请求；有待决或来源 unknown 就等原复核/恢复。普通安全收尾和无争议义务继续，不因等待 final 扩大保全。

决定只可维持、撤销或缩小尚未执行的收回/迁移范围，确认原条款内有预算的补偿，或要求补齐原缺证；不能改写已发生的物理结果、创造新用途、无预算赔偿或处置第三方财物。程序保全仅阻止争议项的不可逆处分，已验证不受争议的安全收尾、普通通行和其他义务继续。`N+6Q` 为程序尾部：查全原决定和已执行 receipt 后，已有有效 final 继续；没有有效结论时，不准以争议触发提前收回，保留原有效 tenure 至自然结束，已有自然到期的新使用限制保持，物理处分仍须完整前置。结果 unknown 先恢复原案，不能当作“无人反对”。尾部不把案件清零或免费完成搬迁。

费用由提出收回的真实 payer 预留，日常取得/续期/转让由签署的申请 payer 承担。初审、复核、必要安全收尾和查询的资源份额分别保留；缺资金/授权/合格名册/随机时不能先准入再省略复核。一般玩家不必通过付费制造新案才能取回原通知、receipt 和恢复下一步。实际响应时长、独立性与费用合理性仍须同候选和真人验证，文档时界不是已平衡结论。

<a id="tenure-policy-acceptance"></a>
### 2.4 政策的验收与组合

Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186；技术验证沿[地点 tenure 验证](../../world-runtime/runtime/location-tenure-and-disposition.design.md#tenure-verification)和[独立方法](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#tenure-cases)。必须分别核正常取得/维护续期、双受让并发、未通知收回、闲置证据不足、公共必要性资金不足、复核与部分实际迁移、占领到期与软赛季边界；三入口都解释保留价值、原责任、下一步，不能把一次登记或终态标签当真实处置完成。

## 3. 五项专业验收

### <a id="ac-game-017-01"></a>AC-GAME-017-01：charter 提案与分层审查形成可读选择

承接产品 [`REQ-WR-GR-004`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004) / [`AC-WR-GR-004`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-004)。

给定成立、合并或调整区域的提案，玩家能查看空间锚定边界、持续设施/物流/服务能力、需求/交付、Agent/资源、边界理由和 bond 证据，并区分本地审查、邻区异议与全局宪制/反圈地复核的作用范围。提交前预览只说明当前取舍，不预留地点、资源、资格或权力；执行前条件漂移、缺项或越界时，玩家收到原子拒绝/待补证据及下一步。有效结果至多形成一次有作用域的 charter receipt，不把批准写成永久控制或全局权力。

### <a id="ac-game-017-02"></a>AC-GAME-017-02：双资格约束本地治理且保留通行与独立路线

承接产品 [`REQ-WR-GR-005`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-005) / [`AC-WR-GR-005`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-005)。

玩家能比较受限 OC/可撤回委托/实际控制人封顶形成的治理资格与持续运营、维护或交付形成的本地贡献资格；任何一侧单独不足以把资本、短时到访或历史头衔变成无限控制。区域事项只影响声明的公共项目、服务、许可和受限竞争；触及受保护权利的事项拒绝或转入独立宪制轨道。容量稀缺时玩家看到透明结算、替代路线或重建路径，不能被区域任意驱逐、封路或阻断独立成长。

### <a id="ac-game-017-03"></a>AC-GAME-017-03：退化、恢复和解散保持可归因且可恢复

承接产品 [`REQ-WR-GR-006`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-006) / [`AC-WR-GR-006`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-006)。

玩家能区分短时服务失败、持续退化、观察/恢复、暂停高影响权限、恢复成功和解散/回归未成熟区域。每种状态说明触发事实、当前限制、恢复条件、异议/申诉和下一决策点；`repair_region`、`rebuild_service`、`pivot_local_use` 的时间/阶段成本、资源成本、保留/失去价值和主要风险可比较。短时失败、技术停机或未审计处罚不能静默删除 charter、bond、设施、Agent、tenure、身份、合同或历史 receipt；规则化迁移、回收、重建和申诉只产生一次可追溯结果。

### <a id="ac-game-017-04"></a>AC-GAME-017-04：tenure 支持可读长期规划但不产生永久主权

承接产品 [`REQ-WR-GR-007`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007) / [`AC-WR-GR-007`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-007)。

玩家在取得、续期或转让 tenure 前能读到建设/服务用途、维护与实际使用条件、可规划的时间 horizon、公共通行影响、续期/转让限制以及不能保证的未来价值。玩家可以选择继续维护、转让、延期、改走其他地点或请求申诉；预览不锁定地点或容量，也不保证未来资源、收益或排他权。到期、长期闲置、明确违约或预定义公共必要性触发收回时，玩家能看到通知、理由、申诉以及适用的迁移、可回收拆除或规则化补偿路径；区域竞争、退化或公共必要性不能自动静默没收资产、身份、合同或封锁公共通行。

### <a id="ac-game-017-05"></a>AC-GAME-017-05：服务费与公共 levy 分离且失效时 fail closed

承接产品 [`REQ-WR-GR-008`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-008) / [`AC-WR-GR-008`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-008)。

玩家能比较可排他服务的收益对象、用途、成本、范围、退出/替代路径与自愿费用，以及不可排他公共品 levy 的 charter 授权、目的、受益范围、预算/费率上限、期限、公开账目和定期复核。有效项目在当前授权下最多结算一次；未授权、过期、超范围、无账目或未审计 levy 原子拒绝，不扣费、不产生欠费、资格变化、通行限制或独立成长门槛。玩家得到补证、申诉、常态服务、独立恢复或重新规划的下一步；无效提案、重连和重复执行不产生第二次世界效果。

## 4. 权威边界与证据切线

| 语义 | 本合同拥有 | 其他 authority |
| --- | --- | --- |
| 玩家动作与取舍 | 提案、审查比较、双资格取舍、恢复选择、tenure 长期规划、费用/levy 比较、申诉与下一步 | 产品专题拥有世界规则与承诺 |
| 产品结果 | 仅消费 `GR-004` 至 `GR-008` 的作用域、不可越权和 fail-closed 边界 | `governed-regional-capabilities-and-extensions.prd.md` 拥有产品 authority |
| 执行事实 | 不定义技术字段、签名、状态机、receipt schema、空间算法或回滚；tenure 的玩家政策时界关系与适用补偿沿 §2.1～2.3，实际生产取值仍须合法发布/接受 | `world-runtime` 拥有确定性执行、状态、费用、tenure、receipt、恢复；`p2p` 拥有身份、授权、签名和分布式状态 |
| 设施玩法 | 不取代 micro_depot 的 install/service/depletion/upkeep 合同 | `gameplay-regional-infrastructure-micro-depot-contract.prd.md` 拥有 `PRD-GAME-016` |
| 验证与当前可用性 | 给出玩法 smoke 与平衡风险，不给 release verdict | `doc/testing/prd.md`、QA、同一候选 fresh evidence 与根 README claim envelope 拥有验证/发布判断 |

本合同与 `PRD-GAME-015` 的关系是“相邻消费”而非 authority 合并：成熟世界成长路线提供独立 lane、区域服务和有限影响的 progression；本合同具体说明玩家在区域 charter、tenure 和融资制度中如何行动，但不据此宣称这些制度已实现或已发布。

## 5. 玩法风险与验证期待

- 若 charter 审查成为首局或独立成长的强制入口，会破坏 mature-world 的小玩家路线；验证样例必须发生在首个持续能力之后，并保留服务、恢复和独立路线。
- 若 tenure 只展示取得/失去而不展示建设、服务和长期规划，玩家无法判断持续维护的价值；required smoke 必须包含至少一次长期规划、续期或转让取舍及一次申诉/替代路径。
- 若 levy 被写成普遍税收、加入组织门槛或通行费，必须判为玩法越界；无效 levy 负例要证明无扣减、无欠费、无资格/通行副作用。
- 若重复提交、重连、Agent retry 或旧 receipt 复制 charter、tenure、收费或恢复效果，必须判为 exactly-once 回归。
- 若阶段成果只有更多库存、吞吐或重复次数，而没有新选择、恢复弹性、局部协调位置或区域用途，必须判为 `grind_only`。

`test_tier_required` 应覆盖五项 AC 的玩家可读预览、当前条件重验、原子拒绝、下一步、申诉/替代和跨 Viewer/pure API/Agent 的语义一致性；`test_tier_full` 再覆盖跨节点授权、持久化、replay、并发与恢复。上述验证期待不是当前实现或 release 证据。

## 6. Non-goals

- 不规定空间算法、资格阈值、OC/委托权重、实际控制人公式、bond 数额、费率/预算上限、levy 计算或市场价格；tenure 的时界关系、适用补偿和程序按 §2.1～2.3，生产参数须合法发布并明确接受，不由示例取值代签平衡。
- 不实现投票、治理签名、账户聚合、土地登记、费用结算、设施迁移、罚没、申诉、状态机、receipt schema、API、Viewer 控件或 Agent 工具。
- 不改变 `PRD-GAME-015` 的成熟世界成长路线、`PRD-GAME-016` 的 micro_depot 设施合同、战争/市场/配方/区域竞争或紧急保供专业规则。
- 不将文档接收、局部实现、历史样本或单项 green 写成当前可玩、已平衡、已发布或 release claim。
