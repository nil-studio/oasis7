# 贡献信号接收、审核与奖励台账系统设计

- Owner role：liveops_community；评分与批准：producer_system_designer；执行：原 reward execution owner；验证：qa_engineer。
- 适用范围：有限预览的 issue、PR、DM、公开内容和 session summary 贡献信号，以及已采用入口对这些原记录的反馈。输入基线为 d53e6e745d68c88f59d9aca009cd94451fb6be44 的产品、操作包、导入脚本与 reserve 合同。
- 本文给出完整目标合同；当前脚本的 ready、文档合入和模板可填写均不证明跨渠道台账、批准或发放已经执行。新增评分选择是本次提出、供原 producer 评审采纳的首版政策，实际采用绑定其明确版本与原 round，不追改历史轮次。
- Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a。

## 1. 问题、目标与非目标

现有 PR importer 能区分是否申请及 Reward Account 是否齐全，但它逐条输出候选，不识别同一贡献跨渠道、跨轮次的主记录。现有 ledger 分列建议、批准与发放，却没有不可变修订、失效证据和 unknown 发放的完整消费合同。目标是让贡献者读到真实范围、证据状态、处理结果与下一步，并使所有当前消费者只使用同一主贡献及其有效决定。

本合同沿用原贡献类型、审核角色、独立 reward reserve 和真实多签执行，不建设新的奖励服务、World 身份、通用资格或支付协议。参与、分数、建议档位、收到反馈与文档归档都不自动产生代币、World 资产、治理权、后续访问权或合作。成熟世界声誉、服务信用和世界资格继续由各原专业来源产生，本台账不能充当这些来源。

## 2. 上游约束与相关角色

评分唯一规则为 [reward pack PRD](readme-limited-preview-contributor-reward-pack.prd.md#recognition-score-v1)；逐行记录与批准的专业来源为 [ledger PRD](readme-limited-preview-contributor-reward-ledger.prd.md#recognition-ledger-contract)。原 [reserve consumer](../../p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-reward) 只消费 canonical ledger 中已批准、完成 actual-value review 的最终金额。操作包、发布说明和客户端不能代替这三者的决定。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | 收到、核验与后续批准各有真实来源，未申请不进入奖励审核 | [接收来源](#recognition-intake-source) | LiveOps、原入口 | 不自动创建权益或新渠道 |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | 同贡献跨渠道、身份和轮次保持主记录、来源及去重 | [贡献身份](#recognition-contribution-identity) | LiveOps、原作者与原证据 | 不将 first-submit 当归属证明 |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | 成员与汇总建议可算且不代批准或金额 | [评分消费](#recognition-scoring-policy) | pack producer | 不创建分数兑换率 |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | 重复、未核项、失效项不贡献第二次分数或资格 | [评分消费](#recognition-scoring-policy) | pack producer、主记录版本 | 不从负修正后的正数推出资格 |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | 批准、拒绝、延期与真实执行分别留痕 | [决定生命周期](#recognition-decision-lifecycle) | producer、执行 owner | 不以被审核代签权益 |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | 失证、撤回、合并和纠错只追加，不抹原批准/发放或造债 | [决定生命周期](#recognition-decision-lifecycle) | 原审核与决定来源 | 不自动追回、补发或惩罚 |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | 批准与原发放标识关联，unknown 先查原件，已执行不再发 | [发放消费者](#recognition-distribution-consumer) | 原 reserve/multisig | 不修改原链的签名、nonce 或费用 |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | 独立批准、对外签发与实际执行分层，无批准不得发起新支付 | [发放消费者](#recognition-distribution-consumer) | 原 producer、reserve/multisig | 不把受理或账本保留当成支付，也不声称可撤销已外发原件 |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | 全部已采用入口返回真实状态、证据、blocker 与下一步 | [反馈与恢复](#recognition-feedback-recovery) | 各入口、当前读权限 | 不把未接线表面说成已支持 |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | 关闭、跨入口重试和恢复不重置原窗口及资格 | [反馈与恢复](#recognition-feedback-recovery) | 原计划与当前读权限 | 不用新账号或新 round 重开旧机会 |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | 有限材料、完整被审集合与真实容量决定可完成的审核 | [容量与成本](#recognition-bounds) | LiveOps / producer / QA | 不承诺无限扫描或固定人工 SLA |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | 去重与旧分配恢复保留完整主项、alias 和历史，超限不得截断为未记录 | [容量与成本](#recognition-bounds) | 原主记录、分配与历史来源 | 不用局部集合重开奖励资格或抹除旧分配 |

## 3. 当前状态、目标状态与差距

| 对象 | 固定基线事实 | 本文目标 | 尚需实际接线或运行验证 |
| --- | --- | --- | --- |
| PR import | [build_result](../../../scripts/readme_reward_pr_intake_lib.py) 仅以显式 yes 与 Reward Account 判定 ready/deferred；未申请或无效不生成 row | 保留原状态，候选导入后经过主记录/证据/当前版本接收 | ready 不证明证据、作者、账户或奖励资格已核验 |
| round scan | [build_report](../../../scripts/readme-reward-pr-intake-round-scan.py) 逐 entry 生成行，Ledger ID 取前缀及 PR 号；输出不填 producer/distribution | 原导出只是 draft 输入，跨渠道合并后才形成 canonical 投影 | 跨库同号、跨轮次、不同渠道的语义去重尚非当前脚本能力 |
| 操作包与 ledger | 有 base、quality、band、approval/distribution 分列，§7.3 曾明确 ≥90 路径未决 | 本次首版完整评分选择及不可变事件、当前投影和原件恢复 | 不把模板新增字段当数据库、反馈接口或一次发放已实现 |
| 历史轮次 | 原真实 round、批准及执行引用各按当时规则保存 | 只用准确 legacy 引用接收旧事实并防新重复 | 不重算历史分数、金额、日期或发放结果 |

## 4. 边界与结构

<a id="recognition-intake-source"></a>
### 4.1 原入口与权威生产者

渠道只产生投稿事实；LiveOps 在原项目授予的职责内登记来源、核验材料和形成建议；producer 依原真实审核权限批准/拒绝/修订；执行 owner 依原 reserve/multisig 权限实施已批准金额。保存每次行动的真实操作者、当时角色依据和原决定引用，填写角色名称或拥有仓库写权限本身不授予奖励批准/支付权。

| 当前来源 | 接收对象与原身份 | 入账和返回合同 |
| --- | --- | --- |
| PR | 原仓库身份、PR 号、作者身份、body 修订/获取时点与显式申请 | 沿原 importer 四态；ready 只进 draft；未申请/无效保留 import 结果及更正步骤，不伪造奖励 row |
| issue / 反馈帖 | 原 issue 身份、作者、版本和所申报成果 | 先记录 received；证明事实/归属/范围后才进入该计划审核；反馈 bug 的受理不自动等于申请奖励 |
| DM | 原获准接收通路、消息身份、经核的发送者与最小摘要 | 私有原文留原受控来源；公开 ledger 只放不泄密的引用/理由；不得推定允许公开全文或支付 |
| content | 原公开作品身份、版本、作者与可核验位置 | 重发/转载链接作为别名，不能单凭上传者或 URL 判断作者及独立贡献 |
| session summary | 原样本/会话、实际作者、范围、build/environment 与总结版本 | session 时长不是分数；结构化贡献按原 C-04 规则核实，不把实际游玩自动视为申请 |
| 已采用 Viewer / API / 其他玩家表面 | 当前表面所指原计划/投稿/查询通路 | 只消费同一记录的可读投影；没有该计划入口时明确未提交并指向真实受支持通路，不伪造 endpoint、后台受理或读取权限 |

每个当前采用计划必须能定位其原 scope、purpose、起止窗口、负责人、可补证条件、实际来源与查询/返回通路。缺任一必要来源时，相关项返回未进入/待补/延期及原因；无关且已核实的项继续。登记与受理不收费、不发币的意思只限本台账业务效果；原外部访问、运营人时及链上真实执行成本仍按各自来源记录。

<a id="recognition-contribution-identity"></a>
### 4.2 稳定贡献与参与者身份

目标 ledger 维护项目内稳定 contribution_id；它标识一次可独立核验的成果/贡献，不含 round、渠道、账号或 Reward Account。LiveOps 在首次 canonical 登记时分配不可复用的 opaque ID，记录当时原来源及不可变 event_id。既有 Ledger ID 仍是 round 的行定位，不能代替跨轮主贡献 ID。发放账户只是收款字段，不代表贡献归属、同一人或新资格。

contributor_ref 引用原 Contributor/Public Handle 的可核身份与必要别名证据，不增加实名、raw public key 或 World 账户要求。相同 handle、相同收款账户或 first-submit 不足以判定同一人/真实作者；跨 handle 合并须原渠道身份或作者确认等已有合法材料。共同成果保留参与者与争议说明，只建一个成果主项，不给每个署名复制整份 base。首次高质量提交的原优先记录只能在归属和独立成果已经核清后使用，不能抢占他人的作品。无法核定时只 hold 争议项或有争议的汇总关系，其他已核贡献按各自原身份继续。

判定同贡献依次核对原件身份（例如稳定仓库加 PR/issue 标识、公开作品版本、原 session）、同一成果及新增实际价值，再形成 reviewer 理由。纯转载、重复报告、相同成果换类型、换账号/付款地址、重连或改投另一 round/purpose 只追加 source_alias，不新计资格、优先权或分数。issue 与 fix、文档与翻译若确有可单独验证的新增成果，可建新项，但必须写清相对旧项的真实增量与证据；“另有一个链接/标签”不足以拆项。

主记录索引由原 canonical ledger owner 持有，记录 contribution_id、原件别名、当前主项、适用计划/窗口、已批准分配和历史执行引用。当前 round 只引用该索引的固定版本及命中主项，不在每轮重新从零判断；索引缺页、旧执行归属不明或相关 legacy 记录未核清时，受影响项先延期，不能以“本轮没找到”为从未奖励。它是原运营台账的业务索引，不是全网扫描器或新的身份权威。

先分成 A/B 后确认重复时，追加 merge 决定，选一个稳定主项，其他 ID 永久作为可查询 alias；选择主 ID 不改变真实作者、首次来源或原批准的事实。别名在当前投影中直接指主项，禁止循环和 ID 重用。若原 merge 后证实存在独立成果，追加 correction 并指出原记录与独立增量，不删除 merge，也不因此重开窗口或复制已经消费的部分。

## 5. 关键运行流程

<a id="recognition-scoring-policy"></a>
### 5.1 评分、成员与汇总

评分按 [首版政策](readme-limited-preview-contributor-reward-pack.prd.md#recognition-score-v1) 的一个 base、一个 quality 修正和原阈值计算。每条成员行记录 qualification/evidence disposition、规则版本、base、选中的单一 quality、s 和自身 band；进入奖励建议还须原Reward Account、证据及reviewer字段齐全可核，普通反馈已收到不能绕过奖励deferred。C-07 恒为 0；不可核、虚假或失证项不能靠 base−20 仍为正数取得 eligible。duplicate 的别名不是第二个成员。

bundle_id 是同一原 round、purpose 与已核 contributor 的建议集合身份，成员固定为互异主贡献及其明确版本。先记录全部候选及排除原因，再仅对当前有资格、可核且尚未被另一个认可决定消费的成员计算 S。未决项单独保留，不能默默从分母消失，也不阻塞可分离的已核成员；补证后加入同一 bundle 的后继版本，仍受原窗口约束。跨 round/purpose 的同一贡献沿原主项及既有决定，不加入第二份建议资格。历史包保留当时总分；当前更正/新增包明确列旧已消费项及本次真正可处理的增量。

此处“已消费”指原主贡献已经有生效批准/分配或实际执行；未执行授权的合法修订仍是原主项/决定族，不能换包再建一项奖励。rejected/deferred 不伪装成已付款，其真实补证或纠错只能沿原案件、权限和允许窗口追加，不能重新取得投稿优先权或重开已关闭计划。求和采用精确整数，溢出或成员版本/总数不一致则不生成汇总建议，不截断或饱和计分。

成员行 band 与汇总 band 分开。例：独立 blocking bug s=40+20=60（medium）与独立文档 s=20+10=30（small），S=90 才给该集合 large；两个成员仍为 medium/small。该集合不是付款行，不能以 large 为每个成员或普通 PR 复制高额批准。每个普通 merged PR 的分配总和、拆分收款、补充批准及重试仍共享原 150 OC 默认上限；原 exceptional note 与 1500 OC 轮次例外不变。

实际价值复核可拒绝或下调建议/金额，但需追加真实 producer 决定并保留原计算分、计算 band 与原建议，不能伪改公式凑分。并发改证、合并、身份更正、旧发放确认或规则版本变化使旧建议 stale；当前批准必须重读其全部成员和适用输入。汇总以固定成员清单逐项相加，不采用未声明的累加质量分、金额例外、在线时长或自动“重要贡献加分”。

<a id="recognition-decision-lifecycle"></a>
### 5.2 受理、审核、决定与纠错

1. 接收时形成 intake event，绑定原请求/来源修订及 scope/window。成功持久回读后才返回“已收到”；导出 stdout、抓取成功或写入超时都不能代签 canonical 接收。
2. 原角色核查当前身份、证据可用性、贡献归属、原窗口及主记录。满足才推进 captured→scored→reviewed；缺项保留 draft/deferred 与补充/等待/关闭路径。ready 是导入条件，不是 reviewed。
3. producer 明确记录 approved/rejected/deferred，绑定 decision_id、前版、exact input revisions、范围、理由、实际权限来源和批准明细。无明确批准保持未批准；感谢、reviewed、汇总 band 或同事口头转述不替代批准。
4. 进入执行前完成原 actual-value review，固定最终逐项金额/受益账户及执行授权。没有发放事实时保留 approved/pending，不显示 distributed。
5. 新证据、撤回、失效、归属冲突或金额复核只追加事件/决定。更正未执行决定须核原 current revision；旧决定和原事实保持可查，当前投影单独说明哪些未执行授权已经撤回或失效。

| 发生变化时 | 当前处理与允许后续 | 必须保留 |
| --- | --- | --- |
| 批准前撤回、deleted/expired/inaccessible、归属或 scope 不符 | 标当前不可依赖；按原计划待补、延期或关闭，不进入批准 | 最初 intake/来源引用、可用性变化、理由，不要求保存已删除源内容 |
| 已批准、尚无对外签发登记且可核未产生可投递原件 | 暂停依赖该项的新准入；producer 以新 revision 确认、下调、撤销或延期其未执行部分 | 原批准及当时证据/金额；不把它改写成从未批准 |
| 已对外签发/可能投递，无论是否已知提交 | 记录当前证据问题、阻止新的分配/许可；依 §6.2 查原操作，不能声称本地hold已经阻止原件生效 | 原批准、签发/提交原件和unknown；占用持续，不假设已撤销或可改换账户 |
| 已确认 distributed | 保留真实执行，另标当前证据/决定不再可依赖；交原 review 核查 | 原金额/日期/Distribution Ref；不自动追回、惩罚债务、补发或再授资格 |
| 先分后并且至少一项已批准/发放 | canonical 主项关联全部原决定/支付，停止重复未执行分配，当前增量扣除已处理贡献 | 每次真实决定和执行都保留；若历史真的重复发放，明确异常而不删一条美化 |
| 原窗口/渠道已关闭、reviewer 不可用、身份未明 | 原有权 owner 延期/关闭或明确交接到既有合法路径，未获允许不得补证或重新申请旧贡献 | 原范围、时点、未决责任和可查询引用，不借渠道迁移延长期限 |

## 6. 接口与数据合同

### 6.1 原台账记录与消费者

这是原运营记录的目标数据合同，可先由获权人员按原 ledger 的版本化存储执行，不新增网络 API、签名格式或自动付款组件。实际实现可保持旧表为投影，同时以同一 canonical 更新保存以下记录；不能只在临时聊天中声称这些字段已经存在。

| 记录 | 必须携带的身份、版本与内容 | producer → consumer |
| --- | --- | --- |
| IntakeEvent | event_id、原 channel/object/message/revision、scope/purpose/window、actor/contributor_ref、收到时点、证据引用/可用性、request_ref | 原入口/LiveOps → 主记录与玩家反馈 |
| ContributionRevision | contribution_id、revision、previous_ref、来源别名、归属证据、独立成果说明、当前 evidence/qualification disposition、merge/correction refs | LiveOps → 评分/producer/历史查询 |
| RecommendationRevision | bundle_id/revision、规则版本、固定候选/合格/排除成员及版本、各自 s/band、S/汇总 band、复核者 | LiveOps → producer；无付款副作用 |
| DecisionRevision | decision_id/revision、previous_ref、确切主项/建议版本、真实批准者/依据、approved/rejected/deferred、理由、有效分配明细与 actual-value review | producer → 原执行 owner/反馈 |
| Allocation / ExecutionLink | 不可复用 allocation_id、canonical主贡献及原分配占用、current approval revision、完整受益人/Reward Account/原资产网络/金额、原proposal/transaction/request ref、对外签发登记、attempt与结局 | producer/原执行 owner → ledger closure |
| ReadProjection | 当前 canonical revision、原件身份、当前状态与历史结果、scope/window、可公开证据状态、blocker、真实 next action、可读旧版本引用 | ledger → 各获权受众 |

所有引用绑定实际固定版本/内容摘要和原来源，字段字符串本身不证明权限或事实。原同 ID/同内容的重复写返回已存在结果；同 ID 不同内容或 expected revision 落后返回冲突及当前可读版本，不能 last-write-wins。query 的当前权限与历史结果是否存在分开核，不把无读权或无法查询说成未提交。

<a id="recognition-distribution-consumer"></a>
### 6.2 原 reserve 执行与一次效果

原 token contract 继续决定可用 reserve、signer、多签批准、nonce、资产/网络、手续费与链的确认条件。本文只要求其原执行 owner 接收一个可核的有效分配：同一 contribution 的当前合法批准、actual-value review、固定账户及金额、原 scope、allocation_id，以及已经消费该项的全部旧执行引用。账户修改、计划迁移或重新登记不是新的分配；在执行前修改须经原 producer 的明确修订和接收，旧请求一旦提交或 unknown 不能简单替换。

ledger 与外部多签/链不构成跨系统原子事务。准入采用以下两个原运营写入切点，所有实际签发/执行 writer 都必须消费；它们不是新链上 capability，也不能阻止违反原权限的外部行为：

1. **原分配占用**：先按 canonical 主贡献、全部 alias 与其原批准的唯一分配族，CAS 占用整个当前金额/收款计划，并登记 allocation_id、approval revision、固定 payload、原 proposal/transaction/request 身份或其尚未创建的明确准备状态。不同 allocation ID、账号、round 或批准修订都先查同一主贡献的占用/旧已付金额，不得复用旧批准。拆分收款是该一次计划的明细，各明细实际已付+在途+拟发金额不超过当前合法批准总额；同PR总额继续受原150 OC护栏。不同writer对同一分配只能取得一个有效占用，失败者读取原记录。
2. **对外签发登记**：在创建/发送多签 proposal、交付签名材料或任何可投递原件之前，再以同一 canonical revision 核当前证据hold、批准、actual-value review、账户/金额、占用及原执行权限，CAS持久记录这一个准确计划即将对外的事实并读回。若新hold先提交，本次登记拒绝；若该登记先提交，后发hold不能证明外部未接收。原通路分配proposal ID前即可先绑定准确请求内容与本地关联键；取得真实ID后只追加关联，不谎称本地键已获外部受理。没有真实技术通路/signer权限时保留pending，不铸造技术授权。

对外登记以前，只有可核尚未签发/没有任何可能投递的原件，才可由原有权方撤销该占用并接收新的批准版本。登记以后，即使本地尚未看到签名或提交ACK，也按可能在途处理；hold、撤销批准或换账户只阻新的分配/新的对外许可，并沿原多签协议请求其实际支持的停止/取消，不把本地删文件、改行或停止worker当外部取消证明。若登记后实际尚未发出但无法证明此事实，同样保留unknown占用。释放或改变尝试须取得原权威“未生效且旧请求不能再生效”的结果；否则持续查询原件，不把占用交给第二个allocation。证明取消也不恢复已失效的证据、批准或权限，下一步仍须当前合法重验。

原提交 ACK、后台任务启动或局部余额变化不算 distributed。只有可核的原正式执行结果确实包含正确网络/资产/金额/收款人及原请求身份，才能追加 Distribution Ref 和真实日期。确认成功但台账写回失败时先查询该原件，恢复同一 closure；不能再次发放来“补齐台账”。

失联或确认未知时状态为 pending/unknown，执行者查询原多签 proposal、原交易/请求及原链确认结果；未查到、权限不足、节点历史不足或超时不是未执行证明。仅在当前批准/权限仍允许且原执行协议支持安全重发同一原件时，才绑定同 allocation/attempt显式重传；后发hold不允许主动重新投递来抢在撤销前生效。创建新交易只允许原权威证明确未生效且旧请求不能再生效、重新验证当前批准与费用后进行，仍消费同一分配，不增加金额。没有这种证明或安全查询能力时等待/转原授权执行 owner 核查，禁止换账户、换链、换 nonce 猜测重试。

当前证据/资格检查与原事务已经提交的事实不一致时，保留两轴，不伪造原链 rollback。原查询不产生新的认可、分配或发放效果；外部读取配额、实际已经发生的交易费用及独立运营成本仍按原规则。

## 7. 状态、事务与持久化

### 7.1 不可变事件与当前投影

一个 canonical 更新包含新事件/修订、涉及的主项及别名索引、成员/决定引用、当前投影和反馈定位；在同一存储版本中全部生效。初始实现可以使用原经授权接受的仓库版本或原有版本化运营存储，采用 expected canonical revision 防并发覆盖；并非在本 PR 运行或授予任何仓库/平台写权。reviewer/producer 的真实决定引用仍单独必需，Git commit 不是奖励批准。

写入前重读全部受影响主项、alias、bundle、current decision 与主贡献的全部分配占用/已付结果。两个操作者同时把已识别的同贡献登记成新项时只允许一个基于原版本的更新成功，失败者读取并关联已有主项；未知的语义关联在核实后按 merge 纠正，不能宣称自动识别全网重复。先分后并若两条均有对外原件，合并索引必须保留两份在途占用并分别查询，不能删除一个占用或伪称仅一笔已存在支付。已提交的旧轮事实通过不可变 refs 引入当前索引；缺失不能以 empty 代替。

读投影分开：导入四态；专业 captured/scored/reviewed 与原 review status；当前证据/资格；决定及 actual-value review；原执行 pending/failed/retried/distributed。approved 后失证可以同时显示“历史已批准、当前暂停未执行”；distributed 后失证显示“已真实发放、当前证据不可核”，不能让单一 deferred 覆盖已付事实。

### 7.2 失败与恢复顺序

| 故障 | 权威结果与恢复 |
| --- | --- |
| 本地准备/导出后未持久提交 | 仍是候选；重读原 scope 与 canonical revision，保留同 request_ref 后再接收，不返回已入账 |
| canonical 写入结果未知 | 按 event_id/decision_id/allocation_id 查原存储版本；读不到不能新分配身份或宣布失败；先恢复原更新 |
| 已持久、反馈丢失 | 查询返回同原件与当前可读投影；不重做审核/批准/支付 |
| 证据获取超时或来源失效 | 只更新当前可用性与该项 blocker；原决定历史不删除；原窗口允许才补充 |
| 计划关闭或无人接任 | 记录真实 owner/关闭理由/未决项及原查询位置；保留有责任的旧执行对账，不自动延长申请窗口 |
| 恢复旧备份/回放 | 原事件按 revision/previous_ref 还原已存在的历史和投影；不得重发外部消息、审批或支付；恢复到陈旧版本时停止新批准/执行，先与原最新决定及原链结果对齐 |

无 World root transaction 参与这一离线运营状态机；如将来实际通路涉及 World 动作，必须另用原 World authority 的独立合同，不能拿本 ledger 的版本或 token 发放结果代替世界提交。

<a id="recognition-feedback-recovery"></a>
## 8. 玩家反馈、安全与运行约束

每次实际接收、进入审核、状态变化或原件查询返回同一组语义：原计划/范围/窗口、intake/主贡献定位及别名关系、当前证据摘要或“无可用证据”、当前专业/决定/执行状态、主要 blocker、是否需补充、真实下一步及其权限/窗口。文案映射如下，不制造第二套批准状态：

| 真实状态 | 玩家可读表达与下一步 |
| --- | --- |
| no_reward_review_requested / invalid_intake / 无入口 | 未进入奖励审核；沿当前受支持入口明确申请或更正字段；未申请的普通反馈仍按原反馈合同处理 |
| ready→draft / captured | 已收到、待核验；说明具体证据与当前缺口，不写“已认可/必发放” |
| 正在由原角色核验 | 核验中；给出当前关联和等待/补充规则，不虚构截止承诺或排队优先权 |
| scored / reviewed | 结果已记录；展示成员分/成员档与汇总档的区别，尚未批准的明确未批准 |
| deferred / rejected / closed | 真实待补/延期/拒绝/关闭理由；原窗口内补证、等待既有 owner、转真实适用通路或结束 |
| approved / pending / unknown | 明确已批准的具体范围与未确认发放；查询原执行，不创建第二付款申请 |
| distributed + 证据可用性变化 | 保留已发放原结果，另示当前证据状态及原复核路径，不说已追回或新增欠款 |
| duplicate / merged / correction | 提供允许公开的主记录关系和当前决定；保留首次 trace，不重新排队、重置窗口或再计分 |

PR/issue 用原受支持提交与回复/查询路径；DM 只在原获权私有通路返回必要信息；content/session summary 沿原投稿登记/结果入口反馈。每条回执记录实际发送渠道和原消息定位，发布失败只标反馈未送达，重试返回同一结果；没有真实渠道许可不能自动发帖或声称已通知。用户当前可访问的公开 ledger 信息沿原公开范围；私有证据、账户绑定证明及他人决策材料要核当前受众，公开链可见不授予私有材料读取权。

撤回原文后保留最小来源定位、摘要是否仍获准、可用性与处置原因，不复制已删内容或保存不必要私人数据。账号变更先核原归属与新收款确认，不能只凭当前 handle 覆盖旧记录；未知身份只读其原可公开事实。受保护的引用不能通过 raw URL、错误详情或跨渠道主记录提示泄露。

<a id="recognition-bounds"></a>
## 9. 质量、容量与完整成本

首版是有限运营审核 profile：单次 canonical 更新最多 64 个受影响主项，最多 256 个事件/修订及 1 MiB UTF-8 元数据；引用正文与外部附件不内嵌。该上限是本设计的工程初值，不是贡献者名额、资格阈值或免费服务承诺。超限更新在产生决定前拆成可独立接收的批次；同一个 merge/correction 的不可分写集超限则阻断该更新并由原 owner 采用经过同等验证的扩大容量版本，不允许截断关联记录后批准。

一个汇总集合可以分批准备：先固定本轮候选 manifest 的完整成员数 N、稳定 IDs 与版本；每批至多 64 个，记录已核/延期/排除集合，最终按固定清单核总数和摘要后计算唯一 bundle revision。未准备完时仅展示部分进度，不给全体总分；能独立审核的成员仍可按本行决定推进。已决定/执行成员在最终集合中列为已消费，不成为新资格。N 取该次明确有限原窗口清单，不能运行无界“扫所有未来贡献”的循环。

| 场景与输入 | 有限工作与预期 | 判定/方法 |
| --- | --- | --- |
| 3 个已核主项、另 1 个同项跨渠道副本 | 4 个 intake 对应 3 个主项；单项一个 base/quality；别名不增加分数/付款 | 主项与原件可逐一对账；RI02/03 |
| N 个候选、E 个显式证据引用 | 主项计数给出 ceil(N/64) 的批数下界；实际批数由完整分片计划同时满足64主项、256事件/修订及1 MiB元数据三界，按原角色逐项核证。不可分单项/merge超限则阻断；最终manifest的候选/合格/排除计数守恒 | 不是只验前64项；2项各600 KiB须2批，不能按计数只用1批；RI03/06 |
| 证据读取/人员预算不足 | 记录已经发生的调用、人时和不可核项；明确延期及真实 owner，保留无关已核项 | 不以 partial 当完整pass；RI01/06 |
| 批准后一次提交、两次 ACK 丢失 | 一条 allocation、同一原执行；额外只是原件查询/必要原安全重传 | 零新增分配；RI05 |
| 1 MiB 边界或合并超 64 主项 | 完整对象满足才提交；否则保持原版本与原结果，不半写/删历史 | RI06；目标未作负载实测 |

费用由原通路承担：LiveOps 核证、producer 审批、QA、外部取件/存储与原执行费用分别记录实际已知量及未知项；本文不把这些合并成 World Data/E 或凭 band 自动扣币。若原计划有预算/名额/窗口限制，在处理前明确该原版本；未知成本不能写 0，预算不足不抹旧责任。该目标可通过人工版本化 ledger 先按有限集合执行；自动 importer、统一查询与消费者 CAS 尚须实现和实际验收，设计样例不是已经发奖的业务证据。

## 10. 兼容、迁移与回滚

旧 importer 四态、20 列 ledger 导出和 round scan 继续作为候选输入；扩展记录与投影通过 [操作模板](readme-limited-preview-contributor-reward-ledger.md#recognition-ledger-records) 填充，未扩展工具不能被声称已经实施 cross-channel dedup。单纯抓取或重新导出不得重写 canonical 行、approval 或 closure。

新规则采用记录必须固定 producer 的真实采纳决定、评分/记录版本、适用 round/purpose 和输入窗口。本次 PR 提出完整候选，不替任何历史轮次签发采用或追加发放。原 legacy row 保留原 score、band、批准与实际 Distribution Ref；只追加不可变关联和“历史/当前是否可再依赖”的说明，不反算旧分或把旧大档改成新汇总。

启用顺序是先核原记录及相关 legacy 发放、建立可读主记录/别名与当前版本，再接 importer 草案、评分/决定、执行和玩家反馈。任何消费者不支持当前版本时停止依赖的新批准/支付并保留读旧结果；不静默退回按 PR 号当唯一贡献。回滚只禁用新写入/新执行，旧已提交决定、发放、费用和外部通知不能由文件回退撤销，必须查询原件并追加真实处置。

## 11. 验证设计与可追溯性

方法固定同一候选、政策版本、原计划窗口、参与者身份/权限与隔离的虚构样本，分别观察原始记录、当前投影、玩家反馈及模拟执行返回；业务真实执行另需原授权和实际证据。本文与手册不要求本次执行发放、发帖或私人数据采集。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | [接收来源](#recognition-intake-source) | 五类来源与未申请/缺证正常分支，角色和受理分离 | [RI01](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-01-intake)；同候选，required；实际/fixture分别标明 | 原请求、导入/受理记录与返回对账 | 不证明真实生产审核或发放 |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | [稳定身份](#recognition-contribution-identity) | 跨渠道/handle/round/purpose、共同成果、先分后并与未决隔离 | [RI02](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-02-dedup)；同候选，full | 主项、别名、归属证据与历史差分 | 不证明全网未知投稿自动去重 |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | [评分消费](#recognition-scoring-policy) | 独立成员、90 正例、成员/汇总/金额三者分开 | [RI03](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-03-scoring)；同规则，required | 独立手算、候选/合格/排除清单与固定版本 | 不形成点数汇率或真实奖励承诺 |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | [评分消费](#recognition-scoring-policy) | duplicate/C07/失证不加分，不跨窗口再消费 | [RI03](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-03-scoring)；同规则，required | 错误累计/资格负例与未决项隔离 | 不以高分替代归属证明 |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | [决定生命周期](#recognition-decision-lifecycle) | 审核、批准、当前执行与 actual-value review 分开 | [RI04](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-04-decisions)；同权限，required | 决定来源、版本与玩家可读状态 | 不代替原 producer 的实际批准 |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | [决定生命周期](#recognition-decision-lifecycle) | 批准前/已批未发/unknown/已发后失效不抹史或造债 | [RI04](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-04-decisions)；同候选，full | 原事件与追加纠错、当前暂停/解除范围 | 不自动追回或撤销原链结果 |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | [发放消费者](#recognition-distribution-consumer) | 原分配/交易查询、ACK丢失/已发未回填及并发只一次 | [RI05](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-05-distribution)；原执行隔离fixture，full | allocation与批准/原执行/closure的对应 | 不执行真实多签/代币交易 |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | [发放消费者](#recognition-distribution-consumer) | 独立批准、对外签发与实际执行分层；无批准不发起新支付，原在途结果仍保留 | [RI05](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-05-distribution)；原执行隔离fixture，full | 批准版本、两次CAS切点、原签发与实际执行对应 | 不执行真实支付或承诺撤销已外发原件 |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | [反馈与恢复](#recognition-feedback-recovery) | 当前入口显示范围、可用性、状态、blocker与真实下一步 | [RI06](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-06-feedback)；各实际采用表面，required | 当前读权、实际反馈定位与未接线记录 | 不以模板代跨平台/真人证据 |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | [反馈与恢复](#recognition-feedback-recovery) | 关闭/换渠道/重连无重开机会，隐私与历史读保持 | [RI06](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-06-feedback)；同原计划，full | 原scope/window/身份、当前查询与重试差分 | 不授予新通路或私有材料权限 |
| Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001) | [容量与成本](#recognition-bounds) | 有限完整manifest、超限/中断可恢复且不冒称完成 | [RI06](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-06-feedback)；选定有限profile，required | 规模/字节/次数、人时及未知成本 | 非生产性能或无限服务承诺 |
| Trace refs: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a；[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002) | [容量与成本](#recognition-bounds) | 去重和旧分配恢复须包含完整主项、alias 与历史；超限不得截断当未记录 | [RI06](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-06-feedback)；选定有限profile，required | 完整manifest/分片计划、排除与旧分配引用及恢复差分 | 不以部分扫描证明全局无重复或重新授予资格 |

## 12. 决策、风险与实施剩余

| 决策 | 选择与原因 | 排除的替代 | 主责与重核条件 |
| --- | --- | --- | --- |
| ADR-RI-01 | 原贡献主项跨渠道/round稳定，追加修订和当前投影；原始出处及执行不丢 | 用 PR号/handle/新账户重建身份，覆盖原决定 | LiveOps；新来源/迁移须证明旧索引和 current revision 连续 |
| ADR-RI-02 | 一个base+单quality，合格互异集合求和；成员band不变，汇总不付款 | 偷叠quality、金额例外当score、把未决large移为future | producer；本PR提出完整首版，实际采用和后续改权重均保留版本与原范围 |
| ADR-RI-03 | unknown查原执行、allocation不变；失证保历史并阻依赖的新执行 | 换nonce猜重试、失证自动追回或授新资格 | 原执行owner/producer；新增执行通路须保原一次效果与查询条件 |
| ADR-RI-04 | 原有限渠道和原权限上提供同一反馈语义，缺接线准确标明 | 伪造通用API/World权利、以感谢代真实状态 | 各入口/QA；新增已采用入口须接同一读模型与方法 |

当前必要的身份、评分、生命周期、决定、发放消费、恢复及方法均在本合同确定。实际存储/去重/查询接线、真实审核、发放、跨入口和容量验证仍须按其实际候选完成；缺这些运行证据不改写原设计决定，也不宣称当前业务已通过。历史资料的可核完整性是启用相关新决定的实际前置，无法确认时保留明确 blocker 与原责任，不由迁移脚本制造来源。
