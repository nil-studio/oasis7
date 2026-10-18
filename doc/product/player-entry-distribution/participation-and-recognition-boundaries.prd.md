# 参与和认可边界

## 文档身份

- 所属产品模块：玩家接入与发行
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- 专业域权威：[`readme 产品与治理 PRD`](../../readme/prd.md)、[`testing PRD`](../../testing/prd.md)、[`README.md`](../../../README.md)
- 专业与执行入口：[`doc/readme/prd.md`](../../readme/prd.md)
- Last reviewed：2026-09-13

本文是长期产品分册，定义有限预览中参与、贡献、认可与权益之间的产品边界。它不定义奖励金额、比例、兑换、资格评分、审批字段、发放流程、渠道活动或历史记录的字段、存储与归档实现；具体 intake、审核、台账和对外执行状态仍由适用专业权威定义。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 适用边界：仅豁免重复的产品体验配对 design；专业系统设计仍按各功能状态和准确承接独立判定，不因此变为“不适用”。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 直接表达贡献信号、审核、失效和补救边界；独立 design 会重复该资格与 claim 分离合同。
- 历史设计判定记录：历史分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，历史闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
## 1. 产品目标

有限预览中的参与者可以通过反馈、测试、建设、文档或其他可验证的协作行为，形成可由团队审核的贡献信号。玩家应能理解：参与和贡献可以被认真对待，但产品不会把进入、游玩、互动或贡献本身表述为自动取得任何权益。

## 2. 参与、贡献与认可边界

- 有效贡献应具有与其性质相称的可核验事实、结果或证据；单纯访问、在线时长、泛化互动或未经核验的主张不自动构成认可依据。
- 贡献信号可以进入审核、反馈、感谢、协作或其他适用的后续路径，但审核本身不等于批准、发放、发布、合作成立或未来承诺。
- 认可、分配或其他后续决定必须单独批准、可追溯且在相应治理计划中执行；产品入口、有限预览资格或贡献标签都不能替代该决定。
- 参与、访问、游玩、互动、贡献、审核结果或任何建议标签均不自动产生代币、支付、所有权、治理权、后续访问权或其他可转让/不可转让权益。
- 不同贡献类型、不同计划或不同渠道可以有各自的专业规则和执行边界；任何特定计划都不构成全体玩家、贡献者或渠道参与者的通用权利，也不保证每种贡献必然具备资格。

### 2.1 贡献信号的生命周期与反馈

- 贡献信号可先在明确的计划、渠道或反馈范围内形成“已收到/待核验”的 intake receipt；只有在具备足以核验其事实、结果或来源的证据时，才算进入该范围的专业审核路径。产品层只承诺可追溯的语义，不冻结 intake 字段、评分或台账实现；玩家面的语义标签只映射、不替代适用专业权威的 canonical 状态/台账，例如 `captured -> scored -> reviewed` 以及适用时的 `ready/deferred`，具体以 [`doc/readme/prd.md`](../../readme/prd.md) 及其治理专题为准。
- 玩家表面必须区分“已收到/待核验”（intake receipt）、“核验中”“结果已记录”和“已关闭、延期或需补充”；这些是可读语义，不能替代适用专业权威的 canonical status/ledger。任何一个阶段都不等于批准、发放、发布、合作成立或未来权益。只有适用计划另行完成的批准和执行，才可表达相应的后续决定。
- 每次产生 intake receipt 或进入专业审核路径时，都应返回玩家可读的范围、证据引用或摘要（或明确说明目前无可用证据）、当前语义状态、是否需要补充以及真实下一步。缺少有效入口或证据时，应明确说明是未进入该计划、等待补充、无法核验还是暂缓，不得用“感谢参与”代替结果；进入专业审核与否以及具体处置仍以适用权威为准。
- 重复提交、同一贡献跨渠道重复申报或重连重试不得产生第二条认可依据、第二次审核优先权或新的权益；玩家应能看到其与原信号的关联或合并结果。不可变的 intake/decision trace（含首次记录及其来源引用）不能被静默覆盖。
- 证据被撤回、删除、失效、无法访问或被发现与申报者/计划范围不符时，保留不可变的 intake/decision trace、证据引用/可用性状态和真实处置原因；这不要求保留已删除的源内容本身，也不得把删除后不可访问的内容呈现为仍可用。信号只能保留为待补充、延期或不可核验的记录，或按适用计划关闭；不得追溯制造认可，也不得把缺证转换成惩罚性债务。若计划允许补充，玩家只能按该计划的真实窗口和入口重新提交证据。
- 计划窗口结束、渠道退出或贡献者身份无法确认时，已收到的信号仍保留其来源和当时范围；后续只能明确记录延期、关闭或转入适用路径。窗口关闭、短暂转让、切换入口或重新注册均不能重置资格、延长期限或复制审核机会。

## 3. 权威与冲突处理

| 产品层拥有 | 专业与执行权威 |
| --- | --- |
| 参与可形成可审核贡献、认可不自动产生权益、产品不发布通用奖励承诺 | `doc/readme/prd.md` 及其治理专题拥有贡献规则、审核、执行、记录和对外沟通；根 `README.md` 拥有当前公开状态与 claim envelope |

产品层不得用认可语言暗示固定金额、比例、兑换、收益率、按时长获得回报或 play-to-earn。涉及具体资格、审核结论、分配、渠道活动或公开说明时，以相应专业计划、当前证据和审批结果为准；冲突或缺证时采用更窄边界。

## 3.1 决策、失败、恢复与未决

- 决策：产品只决定参与信号可被提交、核验、记录和转入适用计划；是否认可、分配或形成权益由相应治理计划和执行权威单独决定。
- 失败与恢复：入口不可用、证据不足、身份不确定、范围不符或窗口关闭时，玩家应看到真实 blocker，并按适用计划补充证据、等待核验、转入正确入口或结束流程；重试和切换入口不能伪造认可或重置机会。
- 未决：具体资格、评分、审批、台账与发布路径仍由 `liveops_community` 及 readme 治理专题确认；缺少当前权威时保持为未核验、延期或关闭。

## 3.2 叶级产品要求与验收

<a id="req-entry-part-001"></a>
### REQ-ENTRY-PART-001：贡献信号必须与批准权益分离

Trace ID: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5

- 要求：产品必须区分已收到/待核验、核验中、结果已记录、关闭/延期/需补充与已批准/执行的语义；参与、访问、游玩、互动或贡献本身不得自动产生代币、支付、所有权、治理权或后续访问权。
- 验收：AC-ENTRY-PART-001


- 功能点：[REQ-ENTRY-PART-001](#req-entry-part-001)；验收 [AC-ENTRY-PART-001](#ac-entry-part-001)。
- 适用范围：贡献信号从收到、核验到记录/关闭与批准/执行的独立状态；沿用本条原有阶段和前置条件。
- 系统设计状态：未核实
- 系统设计依据：已核对 [reward pack 的 intake 状态](../../readme/governance/readme-limited-preview-contributor-reward-pack.md#32-scripted-import)和 [ledger 的批准/执行分离](../../readme/governance/readme-limited-preview-contributor-reward-ledger.md#5-approval-summary)。这些是既有操作合同，不能替代所有玩家入口的状态映射、证据可用性和反馈技术设计；该完整技术范围仍未核实。
- 实现状态：部分实现
- 实现与验证依据：PR intake导入已交付：显式请求进入ready/deferred等导入状态，ready仅生成draft，缺字段为deferred；未填批准/实际发放字段，未自动付款或授予世界权益。 [intake独立状态与draft/deferred输出](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/readme_reward_pr_intake_lib.py#L150-L232)；[ready只生成draft的回归定义](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/readme-reward-pr-intake-import-smoke.sh#L20-L64)；[审查/批准/实际分发列独立](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#L20-L26)。[交付PR #56](https://github.com/nil-studio/oasis7/pull/56)；[交付PR #61](https://github.com/nil-studio/oasis7/pull/61)。本次未执行仓库测试、浏览器、安装、部署或实际业务操作；测试源码仅为定义。 PR #56/#61报告对应导入/round smoke检查；属于历史候选自报，未在本固定main复跑。
- 核对日期与基线：2026-10-14；需求与专业文档基线 `7f8a77069c9f7d316500dce11c6ab65da5ac36b3`；只读文本快照审读；入口和验收范围见上述功能点；未执行运行环境或浏览器验收。
- 剩余范围：系统设计范围与未覆盖边界沿本条原设计依据；实现与验证：其他玩家入口和跨渠道的收到/核验中/结果记录/关闭与已批准/执行完整状态映射、实际审核/玩家反馈及非自动权益的全范围验收。

- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="ac-entry-part-001"></a>
### AC-ENTRY-PART-001：审核路径不会代签权益

- 覆盖要求：REQ-ENTRY-PART-001
- 场景与结果：代表性贡献进入 intake、核验、记录、延期或关闭时，玩家能看到范围、证据摘要/无证据说明、当前状态、blocker 和下一步；只有适用计划另行批准并执行后才表达认可或权益。
- 证据边界：intake 字段、评分、台账、审批与渠道执行由 readme 治理和 LiveOps authority/QA 验证；产品层不定义奖励数值或资格算法。

<a id="req-entry-part-002"></a>
### REQ-ENTRY-PART-002：重复或失效信号不得复制认可机会

Trace ID: O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a

- 要求：重复提交、跨渠道申报、重连重试、证据撤回/失效、超出计划范围或窗口关闭时，产品必须保持不可变 trace、来源和处置原因，不得产生第二条认可依据、优先权、资格或隐性债务。
- 验收：AC-ENTRY-PART-002


- 功能点：[REQ-ENTRY-PART-002](#req-entry-part-002)；验收 [AC-ENTRY-PART-002](#ac-entry-part-002)。
- 适用范围：重复、跨渠道、重试、撤回、失效及窗口关闭时的来源、去重和处置；沿用本条原有阶段和前置条件。
- 系统设计状态：未核实
- 系统设计依据：已核对 [ledger 来源/去重字段](../../readme/governance/readme-limited-preview-contributor-reward-ledger.md#2-ledger)与[有界 round 导入](../../readme/governance/readme-limited-preview-contributor-reward-ledger.md#32-merged-pr-round-scan)。字段存在不证明跨渠道去重、不可变 trace、撤回/失效证据与窗口关闭的完整技术处置；该完整技术范围仍未核实。
- 实现状态：未核实
- 实现与验证依据：已核PR/body→build_result→逐entry ledger row→stdout，以及历史人工approved/pending round。程序保留来源和部分处置原因，但每entry append、PR号命名不具同贡献跨渠道唯一性；人工unique/exclusion只证明该轮决定。没有把普通auth nonce、合同去重或字段名冒充参与认可去重。 [import-entry](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/scripts/readme-reward-pr-intake-import.py#L67-L114)；[intake](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/scripts/readme_reward_pr_intake_lib.py#L150-L232)；[scan](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/scripts/readme-reward-pr-intake-round-scan.py#L104-L153)；[scan-output](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/scripts/readme-reward-pr-intake-round-scan.py#L204-L361)；[actual-round](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/doc/readme/governance/readme-limited-preview-contributor-reward-ledger-round-2026-04-13.md#L3-L24)；[actual-pending](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/doc/readme/governance/readme-limited-preview-contributor-reward-distribution-closure-round-2026-04-13.md#L3-L30)。本轮仅固定源码/文档静态审读，未执行业务测试；未取得本卡同候选完整验收证据。实际交付PR映射未逐条核清，不把旧测试定义或历史round当新运行证据。
- 核对日期与基线：2026-10-17；需求PR #4842 `0901927131b3e7c919fc6b092e77ae8895579989`；源码main `e27f4d990bc4d481097c003ff75afc582fd8f65d`；缓存逐Git blob匹配此固定树。本轮无安装/部署/业务执行，无新产品验收。
- 剩余范围：尚需同贡献跨渠道不可变trace/主记录和重复、重连、撤回失效、超范围/窗口关闭的supersede/revoke/reject权威处置，及其反馈/执行消费者；核验至多一个认可依据/机会/资格且无隐性债务。当前工具会重复输出候选行，但这不证明已实际重复发奖。

- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="ac-entry-part-002"></a>
### AC-ENTRY-PART-002：失效证据回到真实补救路径

- 覆盖要求：REQ-ENTRY-PART-002
- 场景与结果：重复或跨渠道信号被关联/合并，失效证据显示不可核验、待补充、延期或关闭；窗口关闭、入口切换或重新注册不能重置资格，玩家只能按计划补证、等待、改投适用路径或结束。
- 证据边界：canonical status/ledger、来源引用、证据可用性和执行记录由 readme/LiveOps/QA authority 定义；产品层不把删除的源内容呈现为仍可访问。

## 4. 组合验收

- PR-1：有限预览入口和说明能区分“可提交或被审核的贡献信号”与“已批准的后续决定”。
- PR-2：产品层不把参与、访问、游玩、互动或任一贡献类型表述为自动代币、支付、所有权、治理或后续访问权。
- PR-3：产品层不发布金额、比例、日期、固定兑换/映射或 play-to-earn 承诺，也不把某个渠道计划扩展为通用权利。
- PR-4：涉及认可或分配的具体说明能回链到适用治理计划、审批和可追溯执行证据；缺少这些依据时保持为未承诺状态。
- PR-5：代表性贡献信号能区分 intake receipt（已收到/待核验）、核验中、结果已记录和已关闭/延期/需补充，并能与适用专业权威的 canonical status/ledger 对账；每一阶段都明确不等于批准或权益，只有单独的批准与执行才能表达后续决定。
- PR-6：重复提交、跨渠道重复申报、重连重试、撤回/失效/不可访问证据、超出计划范围或窗口关闭的样例，均不会产生第二次认可依据、优先权、资格或隐性债务，并保留不可变 intake/decision trace、来源引用、证据可用性与真实处置原因；不把已删除源内容伪装成仍可访问。
- PR-7：玩家可在当前受支持入口读到申报范围、证据引用或摘要（或无可用证据的明确说明）、当前语义状态、主要 blocker 与下一步；这些反馈能回映适用专业 canonical status/ledger，需要补充、等待、改投适用计划或停止时，不以感谢文案或空白状态代替。
- PR-8：跨入口和计划收口的组合证据证明，身份不确定、证据冲突或审核人不可用时只会依据适用专业权威延期、关闭或转入明确适用路径；切换入口、短暂转让或重新注册不能重置窗口、资格或审核机会。

### 4.1 验收追踪

| 成功标准 | 专业 owner | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| PR-1 / PR-4 / PR-5 | producer_system_designer / liveops_community / qa_engineer | `doc/readme/prd.md`; `doc/readme/governance/readme-limited-preview-contributor-reward-pack.prd.md`; `README.md` | 有限预览入口、贡献审核说明与治理计划对账；验证 intake receipt 与适用 canonical status/ledger（如 `captured/scored/reviewed`）的映射，玩家能区分收到、核验、结果记录和批准/执行 | test_tier_required |
| PR-6 / PR-8 | producer_system_designer / liveops_community / qa_engineer | `doc/readme/prd.md`; `doc/readme/governance/readme-limited-preview-contributor-reward-ledger.prd.md`; `doc/testing/prd.md` | 重复、重试、撤回/失效证据、范围/窗口关闭、身份冲突和入口切换负例；不可变 trace、来源引用、证据可用性、处置状态和非自动权益保持可追溯 | test_tier_full |
| PR-7 | producer_system_designer / liveops_community / viewer_engineer / qa_engineer | `doc/readme/prd.md`; `doc/testing/prd.md` | 当前受支持玩家入口对范围、证据摘要或无证据说明、状态映射、blocker 与下一步的可读反馈；缺证或不可用入口不以感谢/空白代签结果 | test_tier_required |

## 5. Non-Goals

- 不定义贡献评分、资格阈值、金额、比例、代币兑换或发放机制。
- 不替代治理计划、渠道运营包、审核台账、执行记录或公开沟通流程。
- 不把有限预览参与包装为投资、收益计划、空投、登录奖励、时长奖励或 play-to-earn。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-ENTRY-PART-001](#req-entry-part-001) / [AC-ENTRY-PART-001](#ac-entry-part-001) | `producer_system_designer` | [`readme 产品与治理 PRD`](../../readme/prd.md#目标)、[`testing PRD`](../../testing/prd.md)、[`README.md`](../../../README.md) | 待执行方法：按 AC 抽样 intake、核验、记录、批准及执行，逐项对账适用治理状态；待取得审核和玩家反馈记录；导航关系不等于通过证据 | `test_tier_required` |
| [REQ-ENTRY-PART-002](#req-entry-part-002) / [AC-ENTRY-PART-002](#ac-entry-part-002) | `producer_system_designer` | [`readme 产品与治理 PRD`](../../readme/prd.md#目标)、[`testing PRD`](../../testing/prd.md)、[`README.md`](../../../README.md) | 待执行方法：按 AC 核对跨渠道重复、撤回、失效及窗口关闭负例，检查不可变来源和处置；待取得实际去重/审核记录；导航关系不等于通过证据 | `test_tier_required` |
