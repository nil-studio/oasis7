# 区域 charter 状态权威、双资格与恢复系统设计

- 设计 ID：`DES-WR-CHARTER`
- 状态：`active`，长期目标合同；专用运行时和正式玩家验收尚未由本文证明。
- Owner role：`runtime_engineer`；制度与区域政策由 `producer_system_designer` / `gameplay_designer` 拥有，身份和授权由 `blockchain_ops_engineer` 拥有，组合验收由 `qa_engineer` 拥有。
- 固定审读基线：`nil-studio/oasis7`，`e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`，2026-10-19。
- 上游：[区域能力 GR-004 至 GR-006](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004)；[区域玩法合同](../../game/gameplay/gameplay-regional-charter-tenure-funding-contract.prd.md#ac-game-017-01)。

## 1. 问题、目标与非目标

区域 charter 不是联盟的一段章程文字。成立、合并、调整、日常授权及持续退化需要一个共同的权威状态，才能避免证据或某一层审查通过后产生部分权力，也避免在恢复时把旧权限重新启用。本设计冻结空间证据、分层审查、双资格、阶段收缩、资产处置和恢复之间的逻辑协议。

成功结果是：完整证据和各层独立权限审查在当前条件下至多产生一次 scoped charter receipt；日常操作同时满足治理与本地贡献资格；退化和恢复有审计事实、明确阶段及逐项处置，身份、合同和历史保持连续。

本设计不实现地点 tenure、公共 levy、创作者许可、治理主体聚合或 OC 锁定规则；只消费其准确授权或提供本 charter 的状态引用。资本/贡献阈值、bond 数额、退化持续窗口和处置补偿由版本化专业政策决定，不填新数值。charter 不取得跨区/全局主权、任意驱逐或独立资产控制权。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-GR-004](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004)；Trace refs: O7-REQ-7de05591-35c1-4d8a-94b2-f64fce61976e | 成立/合并/调整必须有空间、持续能力、需求、Agent/资源、边界理由和 bond 证据，本地/邻区/全局各守作用域；当前漂移不产生部分权力。 | [证据与审查](#charter-formation) | 空间/M4 服务与账本 producer；治理/P2P 当前权限 | 不把 FormAlliance 或一般 grant 视为 charter，不定义地权。 |
| [REQ-WR-GR-004](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004)；Trace refs: O7-REQ-7de05591-35c1-4d8a-94b2-f64fce61976e | 成立/合并/调整必须有空间、持续能力、需求、Agent/资源、边界理由和 bond 证据，本地/邻区/全局各守作用域；当前漂移不产生部分权力。 | [接口](#charter-authority-interfaces) | 空间/M4 服务与账本 producer；治理/P2P 当前权限 | 不把 FormAlliance 或一般 grant 视为 charter，不定义地权。 |
| [REQ-WR-GR-004](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004)；Trace refs: O7-REQ-7de05591-35c1-4d8a-94b2-f64fce61976e | 成立/合并/调整必须有空间、持续能力、需求、Agent/资源、边界理由和 bond 证据，本地/邻区/全局各守作用域；当前漂移不产生部分权力。 | [原子提交](#charter-persistence) | 空间/M4 服务与账本 producer；治理/P2P 当前权限 | 不把 FormAlliance 或一般 grant 视为 charter，不定义地权。 |
| [REQ-WR-GR-005](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-005)；Trace refs: O7-REQ-ee810b69-8c6a-4aca-86e8-ac387c8f8fcb | OC/委托/控制人资格与持续本地贡献共同成立，区域事项不得外溢；通行/容量及独立恢复不任意排他。 | [双资格](#charter-dual-qualification) | GCB-004/005 资格断言；贡献 producer；物流/容量 authority | 不实现上游身份聚合公式、通行价格或路径算法。 |
| [REQ-WR-GR-005](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-005)；Trace refs: O7-REQ-ee810b69-8c6a-4aca-86e8-ac387c8f8fcb | OC/委托/控制人资格与持续本地贡献共同成立，区域事项不得外溢；通行/容量及独立恢复不任意排他。 | [接口](#charter-authority-interfaces) | GCB-004/005 资格断言；贡献 producer；物流/容量 authority | 不实现上游身份聚合公式、通行价格或路径算法。 |
| [REQ-WR-GR-006](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-006)；Trace refs: O7-REQ-46939a8f-c2a9-4ab6-818c-6bfa4bc83c5c | 短时失败、观察恢复、暂停高影响权限、恢复成功和解散可区分；bond、设施、Agent、tenure、身份/合同历史逐项有处置和申诉路径。 | [阶段与处置](#charter-degradation) | 服务事实与权威时间；各资产/合同 owner；独立申诉 authority | 不从停机推断违约，不由区域解散自动转让或没收资产。 |
| [REQ-WR-GR-006](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-006)；Trace refs: O7-REQ-46939a8f-c2a9-4ab6-818c-6bfa4bc83c5c | 短时失败、观察恢复、暂停高影响权限、恢复成功和解散可区分；bond、设施、Agent、tenure、身份/合同历史逐项有处置和申诉路径。 | [原子提交与恢复](#charter-persistence) | 服务事实与权威时间；各资产/合同 owner；独立申诉 authority | 不从停机推断违约，不由区域解散自动转让或没收资产。 |

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线 | 目标 | 差距 / owner |
| --- | --- | --- | --- |
| 区域身份 | [AllianceState](../../../crates/oasis7_runtime/src/gameplay_state.rs) 具有成员、`charter: String` 与形成时间。 | 独立 charter identity、空间/权限修订及审查链。 | Alliance 关系不是区域资格；runtime/institution module。 |
| 权重与物流 | [现有票权状态](../../../crates/oasis7_runtime/src/gameplay_state.rs) 与 [路线状态](../../../crates/oasis7_runtime/src/state/industry_transition/logistics_topology.rs) 各自存在。 | 同一 charter 上下文中的双资格及不封路校验。 | 现有 Agent 权重、路线 owner/capacity 不证明该合取。 |
| 持久化 | [统一事务与恢复](../design.md#622-disposition-and-idempotency-contract) 有基础合同。 | 证据、审查、bond、权限、阶段与处置账本共同恢复。 | 下列专用 schema/producer/消费者和验收仍须交付。 |

## 4. 边界与结构

charter 状态由一个经治理批准的区域制度模块实例拥有；Kernel 校验能力、保护、资源守恒及提交，不把联盟兼容字段扩成土地或治理 authority。模块发布、状态 schema 和激活遵守 [SC-32](../../product/world-rules-core-gameplay/prd.md#sc-32) 及 [制度迁移](../design.md#61-institution-migration-test)；未完成适用前置不能借本文直接新增 native 制度 action/state。

空间/服务/M4 提供实际世界证据，身份与一般治理提供有界资格断言，区域模块裁决作用范围与阶段，资源和资产子系统执行各自 typed delta。Viewer、Agent 和提案者只提出范围与行动意图，不能自报空间证据、持续贡献、bond 支付、邻区无异议或已恢复。

证据、政策、授权撤销和时间遵守[共同权威输入约定](../design.md#runtime-gameplay-authority-designs)：只有进入同一 canonical 输入并绑定摘要/来源的事实可影响生效。执行节点不能通过实时 HTTP、外部监控当前页面或模型意见替代确定性证据，恢复不能用现在的观测覆盖旧窗口。

charter 主记录与 levy/tenure 等子域授权分离。`active` 只表明该 charter 处于正常阶段；消费者还必须匹配具体 capability、对象范围、有效期、授权修订和其自身合同。不存在“区域管理员因此可执行一切”的回退路径。

## 5. 关键运行流程

<a id="charter-formation"></a>
### 5.1 证据、三层审查与成立/调整

1. 只读预览绑定 proposal/revision、create/merge/adjust 类型、所有前身 charter 修订、拟定 scope、请求 capability、政策版本和完整证据集合。提案存储是审查元数据，不预留土地、预算、bond、资源、资格、容量或权力。
2. 证据集合必须分别包含下列权威输入；缺一个不是默认满足。提案人可以提交引用，producer 必须能在提交时重验实际来源。

| 证据 | producer 与最低内容 | 禁止替代 |
| --- | --- | --- |
| 空间锚定 | 世界空间 authority：稳定地点/区域对象、边界版本、交叠/邻接与公共通行影响、可重算的范围承诺。 | 手绘多边形、自报坐标、无 owner 的边界文本不能独立建权。 |
| 持续能力 | 设施/物流/服务状态及已确认运行、维护、交付 receipt；绑定对象、能力范围、观察窗口与当前可用性。 | 有设施名称、短时访问、一次库存快照或模块自报不代表持续服务。 |
| 需求与交付 | 需求/合约和实际交付 authority 提供未重复的来源、服务对象、承诺/履行量及其窗口。 | 提案人自报需求、重复 receipt 或仅注册用户数不代替。 |
| Agent 与资源 | 身份/控制授权、可用资源与实际投入路径；涉及的 owner/payer 明确。 | 成员名单不授予 Agent 控制权，余额不授予地域权力。 |
| 边界理由与 bond | 边界理由关联上述证据；bond policy、payer、资产种类/金额、受益/退还/处置条件及可负担性来自合法账本和政策。 | bond 大小不替代审查；提案或报价不提前锁定资金。 |

3. 三层分别出具绑定同一 proposal/revision/payload 的审查记录。本地审查能力、需求与日常经营可行性；邻区审查跨边界影响、公共通行与相邻权利；全局审查宪制、保护和反圈地。全局不能选择本地日常经营方案，邻区不能要求取得本地区内无关权力，本地也不能跳过另两层。
4. 必需邻区集合由权威空间与影响集计算；没有受影响邻区也必须留下可验证的空集结论，不能由提案者选空。异议绑定具体受影响边界与请求修正，结果为 `approved/needs_evidence/objected/rejected`；不存在将未回复自动当批准的缺省规则。政策如允许有界无异议程序，须有完整通知、期限和有效证明，不能跳过该层验证。
5. 效果、边界、前身 charter 或所需审查集合改变时形成新修订；旧审查只作 provenance，不搬到新内容。所有审查通过只形成 `ready`。激活时对相关空间、能力、需求、身份/资源、bond 与审查 authority 重验；条件漂移整体拒绝或待补证据。
6. create 原子建立 charter、限定能力、所需 bond 占用与唯一 receipt；merge/adjust 同时对全部前身修订做 compare-and-set，在同一提交中撤出被替代的重叠权限、建立新范围、保持历史和适用 bond 义务。不能逐个前身部分合并。相同排他 capability 在重叠对象范围只能有一个当前执行 authority；共享公共通行本身不能被声明为排他 capability。

<a id="charter-dual-qualification"></a>
### 5.2 成熟区域双资格与作用范围

每次本地治理提交与实际执行都消费两个独立证明：`GovernanceQualificationRef` 证明本事项内的受限 OC、可撤回委托和实际控制人聚合上限；`LocalContributionRef` 证明同一治理主体在该区域、适用窗口和活动类型下的持续运营/维护/交付。贡献由真实已确认服务事件生产，按 receipt/贡献来源去重；参与名册、瞬时到访、历史头衔、未结算承诺和本人声明不计为已证贡献。

逻辑准入为 `current_charter_capability ∧ issue_governance_qualification ∧ continuing_local_contribution ∧ current_scope ∧ protected_boundary_ok`。任一侧 `unknown/expired/revoked` 为不准入，不用另一侧高分补偿；控制人封顶在上游聚合后消费，不能对子账户分别封顶再相加。委托撤回、贡献中断和授权收缩按权威事件序影响后续执行，已投票/通过不冻结永久权力。

本地事项先经过[治理分类](governance-classification-and-constitutional-execution.design.md#governance-classification)。只有当前 scope 内的公共项目、服务、许可和规则已允许的受限竞争可继续；禁止保护项直接拒绝，其他可修改宪制事项进入独立程序。日常资格不转成跨区/全局权限，也不改变他人独立资产或 Agent 控制权。

公共通行、容量结算和独立恢复不以本地治理投票资格作为附加门槛。物流/服务消费者须提供适用的透明容量/费用规则及替代路线或重建路径；稀缺可以产生可解释的容量不足，不能把非成员、贡献不足或无 OC 解释为任意封路/驱逐。不存在真实可用替代时如实返回阻塞及复查条件，不能虚构路线、免费容量或擅自扣费。

<a id="charter-degradation"></a>
### 5.3 退化、权限收缩、恢复与解散

每个退化事实绑定实际服务/能力指标、证据窗口、来源 receipt 和政策版本。持续时长使用权威世界时间与已验证观察覆盖；客户端离线或服务遥测缺失记录为 `unknown`，不能当作连续违约。事实覆盖有缺口时暂停依赖该判断的升级处罚；若另有已声明安全限制，只能按该独立 authority 有界限制，不借缺证据永久解散。

| 原状态 | 触发与必需条件 | 新状态 / 权限 | 拒绝或恢复边界 |
| --- | --- | --- | --- |
| 尚无 charter，区域为未成熟 | 完整成立审查、当前条件及 bond 提交成功 | `active`，仅已授予能力 | 缺项保持提案待补，不产生临时权力。 |
| `active` | 经过政策窗口验证的退化事实，或应进入观察的有界服务失败 | `observing`，限制与恢复要求按 policy 显式列出 | 短时失败不直接解散，也不抹除历史。 |
| `observing` | 当前修复/重建/调整用途证据满足恢复政策，必要复核有效 | `active`，新授权修订及恢复 receipt | 仅“提交修复”或进程重启不足；改变边界/能力须另走调整审查。 |
| `observing` | 持续退化达到声明条件，观察/恢复机会与通知已满足 | `high_impact_suspended`，原子收缩指定高影响能力 | 安全/价值保全与既有合法义务使用明确范围，不发通用管理员权。 |
| `high_impact_suspended` | 新鲜恢复事实、政策要求的独立复核/申诉处置及当前条件满足 | `active` 或 `observing`，按政策给出范围和复查点 | 不沿用暂停前的授权 revision，不自动恢复已撤销/替代能力。 |
| `high_impact_suspended` | 无法恢复的已审计结论、通知/申诉条件与完整处置计划成立 | `dissolved`；区域回归未成熟，旧 charter 留终态历史 | 不因技术停机触发；缺处置 authority 时保持待决且不删除资产。 |
| `dissolved` | 新的成立提案及全部新审查 | 新 charter identity，引用前身作为历史来源 | 旧 charter 不能复活；旧 receipt 不授予新权力。 |

恢复操作必须指定 `repair_region/rebuild_service/pivot_local_use` 中当前受支持的路径，给出来源事实、资源/时间义务、保留/损失价值、验收条件及下一复查点。恢复动作被接受不等于服务恢复；只有实际服务证据和阶段转换 receipt 才能重开声明权限。

解散计划以准确世界快照枚举全部受影响对象及其当前 owner、原义务、允许动作、目标和处置 authority，禁止使用未限定的“区域内资产全部回收”：

| 对象 | 必须保留 | 允许的处置与结果证明 |
| --- | --- | --- |
| bond | payer、原政策/用途、已占用/已结算量及原 receipt | 按原条件释放、返还或已合法裁定的扣减；独立账本事务与 receipt，不把解散本身当罚没授权。 |
| 设施与材料 | owner、数量、状态、来源和在途/容量义务 | 已支持的迁移、回收、重建或 hold；物理过程逐步结算，不复制设备或材料。 |
| Agent | 身份、所有者/控制权、既有授权与历史 | 仅交还或终止适用的 charter 角色；转移控制另走其授权协议。 |
| tenure | 原登记、期限、维护与申诉权利 | 只向 tenure authority 提出处置请求；无合法收回条件不得默认为已收回。 |
| 合同及历史 | 未完成服务/交付/资金义务、身份与原 receipt | 继续、终止、补偿或争议由原合同裁决；追加 linked 结果，不擦除债务或确认历史。 |

权限终止与物理处置可以在不同时间完成：阶段转换事务撤去声明权限并建立处置计划；子处置按对象 authority 单次完成，未结项目可读且不能标作已结。计划执行 capability 只覆盖所列对象与安全/义务动作，不能重建日常治理权。申诉引用原因、阶段及对象处置；已发生效果保持原记录，纠正需要独立裁决和新的 linked receipt。

## 6. 接口与数据合同

<a id="charter-authority-interfaces"></a>
以下冻结逻辑 schema `regional-charter/v1`；编码可以采用现有模块 codec，不能改变必填含义。所有对外 charter/proposal/前身/处置引用都是 `(world_id, module_instance_id, local_id)` 的全限定身份；不得仅传裸 `charter_id` 或按同名模块回退。revision、tick 为无符号整数且单调，修订明确属于被引用对象，并携带相应 immutable digest/commit。集合按全限定 canonical ID 排序、不得重复或静默截断。`effective_interval` 为开始含、到期不含；无到期只能由政策显式允许，仍服从当前状态/撤销与复核条件。

| 记录 / 接口 | 必填内容 | producer → consumer 与规则 |
| --- | --- | --- |
| `CharterProposalV1` | `world_id, module_instance_id, proposal_id, proposal_revision, operation(create/merge/adjust), previous_charters[(id,revision)], scope_ref, capability_scope, policy_ref, evidence_set_hash, required_review_set, payload_hash` | 提案→审查器；create 的前身集合显式为空，merge/adjust 必须齐全；新修订不搬旧审查。 |
| `CharterEvidenceV1` | proposal/revision、evidence kind、producer/authority、对象/范围、observation interval、事实/receipt refs、相关状态版本、有效性与缺项 | 空间/服务/需求/身份/资源→区域模块；元数据不伪装成事实 proof。 |
| `CharterReviewV1` | proposal/revision/payload、layer(local/neighbor/global)、reviewer authority、适用影响范围、verdict、理由/异议、原/后继 receipt | 独立审查→激活器；required set 的每项须有当前有效结果。 |
| `CharterStateV1` | charter id/revision、source decision、policy、scope/capabilities、effective interval、status、authorization revision、bond commitment、degradation/recovery refs、可空的 disposition plan ref | 区域模块→事务与授权读面；可空字段仅表示当前无该流程，不表示已完成。 |
| `CharterAuthorityRef` | `world_id, module_instance_id, charter_id, charter_revision, source_decision_ref, scope, capability_scope, effective_interval, status, authorization_revision, source_commit_ref` | 只读权威查询→治理/levy/tenure 等消费者；所有匹配项在消费者提交点重验。charter active 不自动授权 levy 或收回 tenure。 |
| `LocalEligibilityV1` | charter/授权 revision、matter/scope、subject ref、治理资格 ref、本地贡献 ref、policy/窗口、结果(eligible/ineligible/unknown)、缺项/复查点 | 资格及贡献 producer→本地 gate；结果保留两个来源，不能压成未经解释的单一分数。 |
| `CharterTransitionV1` | charter、expected revision、operation identity、from/to status、cause/evidence、authority、生效 tick、capability delta、bond/处置 refs、receipt | 阶段裁决→根事务；同 revision 的并发转换只有一个可提交。 |
| `CharterDispositionV1` | plan id/revision、charter/cause、对象完整集合承诺、每对象 owner/原义务/允许动作/authority、operation id、状态和 child receipt | 处置协调→各资产 owner；`planned/pending/applied/rejected/disputed` 分项持久化，不能以 parent 状态替代子结果。 |

写请求包含 `request_id`、语义 `operation_id` 与 payload digest；幂等键是 `(world_id, module_instance_id, charter_or_proposal_id, operation_id)`。同键同内容返回原结果，同键异内容 `identity_conflict`；换 request ID 不改变语义操作。另以 `(全限定 charter identity, expected_revision)`、world 域内重叠排他 capability 键和每个资产的全限定 disposition operation 约束并发，防止跨实例冲突及客户端换 operation ID 绕开单次处置。

错误种类冻结为 `unsupported_schema / missing_evidence / scope_unverified / review_pending / scope_objection / authority_invalid / qualification_incomplete / protected_boundary / stale_revision / bond_unavailable / disposition_unavailable / identity_conflict / outcome_unknown`。错误携带对象、可公开依据、当前是否已有 receipt、可执行下一步；不得将 `outcome_unknown` 转成新提案。数值/字节容量由版本化运行配置提供，超限在审查/写入前拒绝。

## 7. 状态、事务与持久化

<a id="charter-persistence"></a>
提案状态为 `needs_evidence/reviewing/ready/applied/rejected/expired/cancelled`；与 charter 的 `active/observing/high_impact_suspended/dissolved` 分开。reviewing、ready 和提案 applied 之外的审查记录都不授予权力；`applied` 必须引用创建/调整事务的唯一 charter receipt。

成立/合并/调整事务共同检查证据 read-set、所有前身修订、审查 authority、当前资格、bond 账本与重叠权限，原子写 charter、能力变化、bond 变化、幂等结果、事件和 receipt。任何检查或准备失败都不能留下 bond 占用、临时角色或部分边界。阶段转换同样原子提交状态与所声明的能力收缩/恢复，禁止先发布新状态再异步撤权。

物理迁移、回收、合同结算是独立子事务，各自同时写对象变化、原义务释放/履行、子处置终态与 receipt。更新汇总进度只读取子结果，不能再执行经济效果；取消和完成争用同一处置身份，只有一个合法终态。证据/对象漂移要求重评未执行部分，已结结果和数量不回滚重算。

恢复共同读取空间/政策版本、审查链、charter/授权修订、权威时间/观察覆盖、bond、处置计划/子结果及语义去重索引。不能从 Alliance 文本、缺省 active 或当前余额反造 charter。已提交但响应丢失返回原 receipt；提交是否发生不明时按[事务恢复](../design.md#622-disposition-and-idempotency-contract)阻断确切操作，未确认前不再锁 bond 或恢复权限。canonical replay 重建事实，不按重启 wall clock 补罚、延长窗口或重新调度外部业务。

charter 终态、来源与处置 tombstone 在所有相关历史仍可被引用时必须可查询；物理压缩/归档不能丢失“已释放/已迁移/已结算”的判断。`persisted`、同世界恢复和跨节点 finality 分别由原持久化/P2P 合同证明，本文的状态名不代替它们。

## 8. 部署、安全与运行约束

所有 charter 写入、timer/服务触发及恢复经过同一模块实例和 Kernel 事务，没有绕开它的“管理员修复”写路径。独立审查能力按 layer 和影响范围限定；同一普通批准不能伪装成本地、邻区和全局三个结果。服务 evidence producer 无治理权，审查者无直接资产写权，处置 executor 无一般日常经营权。

证据缺失、签名/身份不可核验、模块版本不可取回和持久化疑义只阻断相关授权，保留历史、诊断及仍合法的独立服务。公开读面披露本人的资格结果、范围、阶段、理由及下一步，不泄漏贡献者私密数据和控制关系原件。

## 9. 质量与容量

| 场景 | 环境 / 刺激 | 指标与来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 三层完整性 | 同一 proposal，遗漏各 evidence/review、邻区集合或漂移一个范围版本 | 缺项时临时权力/bond 占用为零；完整案一份 receipt，依据 GR-004 | [CHARTER-VERIFY-001](#charter-verify-001) | 目标，未运行。 |
| 双资格与通行 | 资本集中、撤销委托、贡献中断、外来主体与容量争用 | 无单侧资格越权、重复控制放大或按成员身份封路，依据 GR-005 | [CHARTER-VERIFY-002](#charter-verify-002) | 目标，未运行。 |
| 连续性 | 短时失败、持续退化、证据丢失、恢复及无法恢复 | 阶段可解释，历史/身份丢失为零，处置每项至多一次 | [CHARTER-VERIFY-003](#charter-verify-003) | 目标，未运行。 |
| 资源/并发 | 多前身合并、重叠 scope、最大 evidence/对象集合、超限请求 | 原子拒绝，无部分合并、截断审查或无界遍历 | [CHARTER-VERIFY-004](#charter-verify-004) | 规模上限由 runtime 配置冻结，不宣称 SLA。 |

## 10. 兼容、迁移与回滚

既有 Alliance 保持原关系与历史重放，不自动生成 charter、空间权利或 bond。启用新 schema 时先部署只读解析和诊断，再经合法模块/制度激活开放新写入；新 charter 的前置证据必须真实生成，不能迁移时将旧成员和文字补成 approved。

旧消费者读不到新 scope/阶段/资格时只能显示不支持或有界历史，不给出可执行权限。新快照保留新旧对象并明确 authority 类型；不能用同名字段把二者混同。回滚仅能回到可读新状态、冻结新准入且继续保全未结处置的版本；不能退到不认识 bond/撤权/处置结果的旧 binary 后继续写。已发生物理结果须通过合法补偿/恢复处理，不通过回退日志撤销。

## 11. 验证设计与可追溯性

场景 ID 是本设计冻结的实施验收方法，专用测试仍待实现。required/full 使用同一 source/build、module/policy、world 和权限快照；正式玩家/API/Agent 结果与其世界 receipt 同案核对，不能用 FormAlliance 成功代签。

<a id="charter-verify-001"></a>
- **CHARTER-VERIFY-001 / required**：成立、合并、调整各一正例；逐项删去持续能力/需求/资源/bond/空间证据，伪造无邻区影响，给邻区通行异议、缺全局审查及执行前漂移。比较三层理由，检查所有前身、bond、角色和空间能力原子不变；重复有效案只有一次 scoped receipt。
<a id="charter-verify-002"></a>
- **CHARTER-VERIFY-002 / required**：在同一成熟 charter 内比较资本足但无贡献、有贡献但治理资格不足、可撤回委托、同控制人多个身份、贡献中断和双资格有效；范围外、保护项、容量稀缺另测。核验公开通行/透明结算/替代与独立恢复不被任意阻断，scope/authority revision 更新立即约束后续执行。
<a id="charter-verify-003"></a>
- **CHARTER-VERIFY-003 / required + full**：分别执行短时失败、观察修复成功、持续退化暂停、无法恢复解散及申诉纠正；插入服务证据缺口和纯技术停机。逐类核验 bond、设施、Agent、tenure、合同与 receipt 保留；子处置取消/完成、响应丢失、重复恢复只产生一个结果。full 加跨节点、快照与后缀 replay。
<a id="charter-verify-004"></a>
- **CHARTER-VERIFY-004 / required + full**：在合并及每一子处置写点注入提交前故障与提交后丢响应；并发调整同一修订及重叠排他 capability、超限 evidence/对象集合；对同案 Viewer/pure API/Agent 比较当前阶段、缺项、范围、保留/损失及下一步。恢复不重新扣 bond、不复活旧授权，也不补造缺失处置。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-GR-004](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004)；Trace refs: O7-REQ-7de05591-35c1-4d8a-94b2-f64fce61976e | [成立/审查](#charter-formation) | 空间与能力证据、三层审查及当前条件共同产生一次 scoped 结果。 | [系统测试手册](../../../testing-manual.md)；按本文场景 CHARTER-VERIFY-001、004；固定三层 authority 的 required/full 正负样例；专用合同回归由实现提供，本文仅规定验收。 | evidence/review 集、前后 charter/bond/capability、唯一 receipt；实际 PR 保存结果 | 不证明空间算法性能、政策平衡或当前上线。 |
| [REQ-WR-GR-004](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004)；Trace refs: O7-REQ-7de05591-35c1-4d8a-94b2-f64fce61976e | [接口](#charter-authority-interfaces) | 全限定 proposal/charter/前身引用、证据与审查版本绑定；当前权限查询不派生 levy 或资产权。 | [系统测试手册](../../../testing-manual.md)；按本文场景 CHARTER-VERIFY-001、004；固定三层 authority 的 required/full 正负样例；专用合同回归由实现提供，本文仅规定验收。 | evidence/review 集、前后 charter/bond/capability、唯一 receipt；实际 PR 保存结果 | 不证明空间算法性能、政策平衡或当前上线。 |
| [REQ-WR-GR-004](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004)；Trace refs: O7-REQ-7de05591-35c1-4d8a-94b2-f64fce61976e | [事务](#charter-persistence) | 空间与能力证据、三层审查及当前条件共同产生一次 scoped 结果。 | [系统测试手册](../../../testing-manual.md)；按本文场景 CHARTER-VERIFY-001、004；固定三层 authority 的 required/full 正负样例；专用合同回归由实现提供，本文仅规定验收。 | evidence/review 集、前后 charter/bond/capability、唯一 receipt；实际 PR 保存结果 | 不证明空间算法性能、政策平衡或当前上线。 |
| [REQ-WR-GR-005](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-005)；Trace refs: O7-REQ-ee810b69-8c6a-4aca-86e8-ac387c8f8fcb | [双资格](#charter-dual-qualification) | 两类资格与控制人上限、事项范围共同成立；通行/独立恢复保留。 | [系统测试手册](../../../testing-manual.md)；按本文场景 CHARTER-VERIFY-002、004；当前资格/贡献事实与容量案例；专用合同回归由实现提供，本文仅规定验收。 | 两个来源证明、拒绝原因、路线/服务结果及三端同义读面 | 不代替 GCB-004/005 身份/OC 实现验收。 |
| [REQ-WR-GR-005](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-005)；Trace refs: O7-REQ-ee810b69-8c6a-4aca-86e8-ac387c8f8fcb | [接口](#charter-authority-interfaces) | 两类资格与控制人上限、事项范围共同成立；通行/独立恢复保留。 | [系统测试手册](../../../testing-manual.md)；按本文场景 CHARTER-VERIFY-002、004；当前资格/贡献事实与容量案例；专用合同回归由实现提供，本文仅规定验收。 | 两个来源证明、拒绝原因、路线/服务结果及三端同义读面 | 不代替 GCB-004/005 身份/OC 实现验收。 |
| [REQ-WR-GR-006](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-006)；Trace refs: O7-REQ-46939a8f-c2a9-4ab6-818c-6bfa4bc83c5c | [退化/处置](#charter-degradation) | 短时失败不解散，持续退化分阶段收权；各资产历史和单次恢复/申诉可追溯。 | [系统测试手册](../../../testing-manual.md)；按本文场景 CHARTER-VERIFY-003、004；四类退化样例、故障/并发/恢复层；专用合同回归由实现提供，本文仅规定验收。 | 阶段原因/时间、权限 diff、逐对象处置及保留历史；实际 PR 证据 | 不证明未经定义的补偿金额、tenure 收回或生产部署。 |
| [REQ-WR-GR-006](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-006)；Trace refs: O7-REQ-46939a8f-c2a9-4ab6-818c-6bfa4bc83c5c | [恢复](#charter-persistence) | 短时失败不解散，持续退化分阶段收权；各资产历史和单次恢复/申诉可追溯。 | [系统测试手册](../../../testing-manual.md)；按本文场景 CHARTER-VERIFY-003、004；四类退化样例、故障/并发/恢复层；专用合同回归由实现提供，本文仅规定验收。 | 阶段原因/时间、权限 diff、逐对象处置及保留历史；实际 PR 证据 | 不证明未经定义的补偿金额、tenure 收回或生产部署。 |

## 12. 决策、长期风险与未决问题

采用独立 charter 状态并消费外部专业证据，避免把组织文本、余额或单一投票扩大成地域权力。阶段转换与物理处置分开，使撤权可以原子发生，而运输、回收和合同结算仍保持各自真实时间及单次结果；未结义务必须持续可见。

政策 owner 在启用前提供版本化资格窗口、退化/恢复/解散条件、bond 和各资产允许的处置政策；无有效政策或 producer 的具体能力返回明确未支持。改变持续窗口、作用范围、身份输入或处置权时须重审相应正负样例和旧 charter 的继承边界，不得靠技术停机、缺字段默认值或新模块版本静默收回权利。
