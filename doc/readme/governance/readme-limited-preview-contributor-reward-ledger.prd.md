# oasis7 Limited Preview Contributor Reward Ledger

审计轮次: 1

## 1. Executive Summary
- Problem Statement: oasis7 已有 early contributor reward 的评分模板和禁语边界，但还缺少“真实一轮怎么记、怎么审、怎么回填发放记录”的统一 ledger。没有 ledger，团队会在真实发放前退回聊天记录、issue 评论或临时表格，导致奖励审核与执行不可审计。
- Proposed Solution: 新增 round-based contributor reward ledger 专题，固定 round meta、逐条贡献记录、producer 审批状态、实际发放引用与归档字段，让 limited preview 的真实贡献奖励具备可复用、可复核、可归档的台账模板。
- Success Criteria:
  - SC-1: 每轮真实贡献奖励都能用同一份 ledger 模板记录 `Round ID / Candidate ID / Window / Status`。
  - SC-2: 每条奖励建议都必须含 contributor、`Reward Account`、evidence link、score、recommended band、review status。
  - SC-3: 审批后的真实发放必须回填 `Approval ID / Actual Amount / Distribution Ref`，不允许只停留在“口头批准”。
  - SC-3A: 普通 merged PR 的审批默认 ceiling 为 `150 OC`；若某条 PR row 高于该值，ledger 必须保留 exceptional case note，`1500 OC` 不得作为常规 merged PR 预期。
  - SC-3B: planned grant 进入 pending distribution 后仍必须经过一次 actual-value review；若 producer 判断实际增量价值低于原计划，必须先下调档位或金额并留下原因，再进入执行。
  - SC-4: 归档前必须输出 round summary、band summary 与 unresolved items，保证后续治理复盘可追溯。

## 2. User Experience & Functionality
- User Personas:
  - `liveops_community`：需要在 limited preview round 结束后汇总真实贡献并形成正式台账。
  - `producer_system_designer`：需要逐条审看奖励建议档位与审批结果，而不是阅读零散聊天记录。
  - reward execution owner：需要根据审批结果回填实际发放数量和执行引用。
  - `qa_engineer`：需要在必要时复核台账字段完整性与证据可达性。
- User Scenarios & Frequency:
  - 每轮 limited preview 结束后至少维护 1 份 ledger。
  - 每次 producer 完成一轮审批时，更新 review status 与 approval 字段。
  - 每次实际发放完成时，补回 distribution ref 并关闭本轮 ledger。
  - 每次某条贡献来自 GitHub PR 时，优先从 PR intake block 导入 `Reward Account`，减少后补沟通。
  - 每次 liveops 需要批量判断 PR 来源条目是否可入账时，用导入脚本直接区分 `ready / deferred / no_reward_review_requested / invalid_intake`。
- User Stories:
  - PRD-README-LTRL-001: As a `liveops_community`, I want one round-based ledger template, so that real contribution review no longer depends on ad-hoc notes.
  - PRD-README-LTRL-002: As a `producer_system_designer`, I want each contribution row to show score, band, and approval status, so that I can approve conservatively and audibly.
  - PRD-README-LTRL-003: As an execution owner, I want approved rows to carry actual amount and distribution reference fields, so that distribution closure is traceable later.
- Critical User Flows:
  1. Flow-LTRL-001: `结束一轮 limited preview -> liveops 汇总可计分贡献 -> 填写 round meta 与 ledger rows -> 标记 draft`
  2. Flow-LTRL-002: `producer 审阅逐条 recommended band -> 标记 approved/rejected/deferred -> 对普通 merged PR 默认按 <=150 OC 审批；若超过则补 exceptional case note -> 输出 round summary`
  3. Flow-LTRL-003: `execution owner 根据 approved rows 准备发放 -> producer 先按 actual-value review 确认最终档位/金额 -> 回填 actual amount / distribution ref -> 标记 distributed`
  4. Flow-LTRL-004: `本轮关闭 -> 记录 unresolved items / next action -> 归档供 future governance review`
  5. Flow-LTRL-005: `某条贡献来自 GitHub PR -> 从 PR reward intake block 读取 Reward Account / evidence link -> 生成或补齐 ledger row -> 再进入 producer review`
  6. Flow-LTRL-006: `运行导入脚本 -> 若结果为 ready 则生成 draft row；若为 deferred 则生成 deferred row；若为 no_reward_review_requested 则不建 row`
  7. Flow-LTRL-007: `进入一轮 merged PR 奖励复核 -> 按 merged 时间窗运行 round scan -> 保留窗口级 status_counts 与逐条来源 -> 再把 ready/deferred 行导入本轮 ledger`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| Round Meta | `round_id`、`candidate_id`、`window`、`status`、`owner_role` | 初始化本轮 ledger 头部信息 | `planned -> draft -> under_review -> closed` | 每轮只能有 1 个主 ledger | `liveops_community` 维护 |
| Ledger Row | `ledger_id`、`contributor`、`reward_account`、`source_link`、`contribution_type`、`total_score`、`recommended_band` | 逐条录入真实贡献 | `captured -> reviewed` | 同 contributor 可多条，但 `ledger_id` 必须唯一 | `liveops_community` 维护 |
| PR Intake Import | `reward_review_request`、`reward_account`、`evidence_link` | 从 GitHub PR reward intake block 导入或补齐执行字段 | `deleted -> submitted -> imported -> reviewed` | 仅当 source type=`PR` 且作者保留该区块并主动申请 reward review 时使用；raw `public key` 不进入名称层 | `liveops_community` 导入 |
| PR Import Script | `import_status`、`missing_fields`、`validation_error`、`ledger_row` | 解析 PR body 并输出 ledger-ready 结果 | `parsed -> ready/deferred/no_reward_review_requested/invalid_intake` | `Request reward review` 必须显式为 `yes`；未请求或 intake 无效时不建 row | `liveops_community` 执行 |
| Merged PR Round Scan | `merged_after`、`merged_before`、`entry.pr_number`、`entry.merged_at`、`entry.import_status`、`status_counts`、`ledger_draft_md`、`ledger_csv` | 批量扫描一轮 merged PR 的 reward intake，并输出 ledger 候选 | `window_selected -> scanned -> triaged -> imported` | round scan 只能复用单 PR import contract；`ready/deferred` 才能推进到 ledger，`no_reward_review_requested/invalid_intake` 仅保留在报告；draft 导出不能擅自填 producer/distribution 字段 | `liveops_community` 执行 |
| Producer Review | `review_status`、`producer_decision`、`approval_id`、`exceptional_case_note`、`actual_value_note` | 审阅并批准/拒绝/延后 | `reviewed -> approved/rejected/deferred` | 缺证据默认不得批准；普通 merged PR 默认 ceiling `<=150 OC`，若超过则必须记录 exceptional reason；进入待发放前仍可按实际增量价值下调档位与金额 | `producer_system_designer` 决策 |
| Distribution Closure | `actual_amount`、`distribution_ref`、`distribution_date` | 回填真实执行引用 | `approved -> distributed` | 未执行前允许留空；执行后必须补全 | execution owner 回填 |
| Round Summary | `band_totals`、`approved_rows`、`distributed_rows`、`unresolved_items` | 输出本轮汇总与遗留事项 | `draft -> summarized -> archived` | 汇总值按 ledger rows 聚合 | `liveops_community` 汇总，producer 审核 |
- Acceptance Criteria:
  - AC-1: ledger 模板必须包含 `Meta`、`Ledger`、`Band Summary`、`Approval Summary`、`Distribution Closure`、`Next Actions` 六个区块。
  - AC-2: 每条 ledger row 必须至少包含 `Ledger ID`、`Contributor`、`Reward Account`、`Source Link`、`Contribution Type`、`Total Score`、`Recommended Band`、`Review Status`。
  - AC-3: 任何 `approved` row 在后续实际发放后都必须回填 `Approval ID / Actual Amount / Distribution Ref`。
  - AC-4: 模板必须允许 `rejected / deferred / distributed / archived` 等状态，而不是只记录“建议发放”。
  - AC-5: 模板不得包含任何固定 token/point 汇率、公开营销文案或 `play-to-earn` 叙事。
  - AC-6: 若 row 来源是 GitHub PR，ledger 必须能回溯到 PR intake block 里的 `Reward Account`，或显式记录为何改为 `deferred`。
  - AC-7: 仓库必须提供可执行导入脚本，至少支持 `--body-file` 离线解析，并在脚本输出里显式返回 `ready / deferred / no_reward_review_requested / invalid_intake`。
  - AC-8: 若某轮 ledger 以 merged GitHub PR 为主要来源，仓库必须提供按 merged 时间窗批量扫描的脚本入口，输出窗口级 `status_counts` 与逐条 `pr_number / merged_at / source_link / import_status`，并保持与单 PR import 相同的状态 contract。
  - AC-9: merged PR round scan 必须支持直接导出 `ledger-draft-md` 与 `ledger-csv`；导出只自动填充 `ready/deferred` 的 PR 来源行和 round meta 占位，不得擅自补 producer decision、approval ID 或 distribution closure。
  - AC-10: 若某条普通 merged PR row 的 `Actual Amount` 高于 `150 OC`，approval summary 和 row notes 都必须显式写明 exceptional case 理由；`1500 OC` 不得在没有该说明时直接进入待发放状态。
  - AC-11: 若 producer 认为某条 planned grant 高于其实际增量价值，必须在 distribution 前先下调 `Recommended Band` 或 `Actual Amount`，并在 approval summary / row notes 中留下 actual-value review 理由。
- Non-Goals:
  - 本专题不决定每个档位对应的具体 token 数量。
  - 不替代 `readme-limited-preview-contributor-reward-pack` 的评分规则定义。
  - 不替代创世参数或 treasury 审计模板。
  - 不把普通 merged PR 的审批 ceiling 扩写成公开 bounty 报价。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 本专题是 reward pack 的执行收口层。`readme-limited-preview-contributor-reward-pack` 定义“怎么评分”，本 ledger 定义“真实一轮怎么记账、怎么审、怎么回填发放记录、怎么归档”。
- Integration Points:
  - `doc/readme/governance/readme-limited-preview-contributor-reward-pack.prd.md`
  - `doc/readme/governance/readme-limited-preview-contributor-reward-pack.md`
  - `doc/readme/governance/readme-limited-preview-invite-pack-2026-03-22.md`
  - `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md`
  - `scripts/readme-reward-pr-intake-import.py`
  - `scripts/readme-reward-pr-intake-round-scan.py`
- Edge Cases & Error Handling:
  - 同一贡献被多人重复提交：先核真实归属与独立成果，再保留一个计分主项及其余不可变来源/duplicate关联；first-submit不能抢归属，不能删除原intake或已存在的决定/发放。具体见下文主记录合同。
  - 同一 contributor 有多条有效贡献：允许多行，但每行必须独立 `Ledger ID`。
  - producer 已批准但发放尚未执行：`Actual Amount / Distribution Ref` 可暂空，但 `Review Status` 不能写成 `distributed`。
  - producer 若批准某条普通 merged PR 高于 `150 OC`，但没有留下 exceptional case note：该 row 必须退回 `under_review`，不得继续进入待发放列表。
  - 某条 row 在文档里先写了 planned grant，但 producer 复核后认为实际价值偏低：必须先改回更低档位或金额，并保留“为何下调”的审计说明；不得带着旧计划值直接执行。
  - 没有链上 reward account：允许先 `deferred`，不得跳过账户字段直接执行。
  - PR 来源且已删除 reward intake block：视为未申请 reward review；若后续要进入台账，必须补齐 `Reward Account` 并保留 approved follow-up 记录，否则不进入 producer 审批。
  - PR 来源且 intake block 保留但 `Request reward review` 不是显式 `yes`：导入脚本返回 `invalid_intake`，liveops 需先纠正 PR body，再决定是否入账。
  - 若只拿到 raw `public key` 或账户派生材料，必须先收口为 `Reward Account`，不得把 raw `public key` 直接作为台账名称层字段。
  - 证据链接失效：未批准候选的当前处理退回 `draft` 或 `deferred`，不得进入正式批准；已批准或已发放的历史不覆盖，按下文合同追加当前失效/暂停/复核与原件恢复。
  - round scan 未限定 merged 时间窗：该结果只能算 ad hoc exploration，不得直接当作本轮正式 ledger import。
- Non-Functional Requirements:
  - NFR-LTRL-1: 每轮 ledger 的 `Round ID / Candidate ID / Window / Status` 完整率必须为 `100%`。
  - NFR-LTRL-2: 每条非 `rejected` row 至少包含 1 个有效 evidence/source link。
  - NFR-LTRL-3: 每条 `distributed` row 必须具备 `Approval ID` 与 `Distribution Ref`。
  - NFR-LTRL-4: ledger 模板必须可直接复制复用到下一轮，而无需重新设计字段。
  - NFR-LTRL-5: merged PR round scan 输出的状态分类必须与单 PR import 完全一致，不允许单独发明另一套 `ready/deferred` 判定逻辑。
  - NFR-LTRL-6: `ledger-draft-md` 与 `ledger-csv` 的行集合必须完全一致，且都只包含 `ready/deferred` 行。
  - NFR-LTRL-7: 普通 merged PR row 的默认 producer 审批 ceiling 为 `150 OC`；任何 `>150 OC` 的 row 都必须带 exceptional case note，且 `1500 OC` 只能作为极少数 round-specific exceptional 决策。
  - NFR-LTRL-8: planned grant 若在 actual-value review 后被下调，ledger 必须同时保留原 candidate 上下文、最终金额和下调理由，避免只留下结果而丢失复核依据。
- Security & Privacy: ledger 只记录公开 handle、证据链接、链上奖励账户与必要审批引用；不记录私人聊天原文或不必要个人隐私。raw `public key` 仅保留在底层签名/账户绑定流程中，不作为奖励台账名称字段。

<a id="recognition-ledger-contract"></a>
### 4.1 主贡献、不可变决定与当前反馈

Trace refs: O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5；O7-REQ-ff67822b-5943-4728-ad4c-795c94fca95a。直接承接[PART001](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001)及[PART002](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-002)；完整生产者、版本与恢复合同由[专业设计](contributor-recognition-intake-and-review.design.md#recognition-intake-source)维护，操作字段见[附表](readme-limited-preview-contributor-reward-ledger.md#recognition-ledger-records)。

- stable contribution_id标识可核独立成果，Ledger ID只是某轮行位置；channel、handle、Reward Account、round/purpose变化均不创建第二次认可资格。归属不明只hold受影响项，共同成果不复制整份分数，无关已核项继续；不把同收款账户当同一人。
- intake/event、主项revision、建议bundle、decision/approval和allocation/原执行分别有稳定身份、前版与真实来源。当前投影可修订，原事件和真实决定不可静默覆盖；同ID同内容重试返回原记录，同ID异内容或旧版本拒绝冲突。
- 评分消费[唯一首版政策](readme-limited-preview-contributor-reward-pack.prd.md#recognition-score-v1)：成员自身s/band与汇总S/band分开，未决/排除/已消费项明确列出；汇总不是付款行。普通PR的实际分配总额仍依原150 OC默认护栏，拆分收款、换包、补充批准或重试不重置。
- 证据撤回、删除、失效、不可访问或与身份/scope不符时保留最初trace、可用性变化和理由。当前非rejected候选推进时仍须满足原有效证据要求；该要求不强迫把已批准/已发放历史改成rejected，也不授权复制已删内容来伪造可用证据。批准后失证先暂停依赖的新执行并复核；已经签定、提交或结果unknown的原支付按其真实在途边界处置，已发放保留事实，不自动追回/追债/补发。
- 原actual-value review以新决定保留原计划值、当前核对理由及最终金额；不要覆盖旧批准。执行owner必须使用同一主贡献的有效分配与唯一占用，签发/提交前持久关联原执行身份，结果unknown先查原件。新allocation ID、fresh nonce或新round不能重复使用已消费批准。
- 窗口关闭、渠道退出或身份无法确认只能按原计划延期、关闭、补证或有权交接；转交不延长原期限、不重排原审核优先权。每次受理/审核/查询的玩家反馈包含scope、证据状态、当前处理/决定/执行、blocker与真实下一步，不用感谢代签状态。

本节是当前设计目标的补齐，沿用原LiveOps、producer及reward multisig权限；不声明脚本已实现完整去重/CAS/查询，也不批准任何新金额。历史round和distribution文件按原版本保留。方法分别为[RI02](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-02-dedup)、[RI04](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-04-decisions)、[RI05](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-05-distribution)与[RI06](../../testing/manual/contributor-recognition-acceptance.manual.md#contributor-recognition-06-feedback)。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP: 先交付 ledger 模板并接回 `readme` / `p2p token` 主追踪。
  - v1.1: 跑完首轮真实 ledger 后，根据遗漏字段补小修。
  - v2.0: 若 future governance 需要 fully on-chain closure，再补 governance proposal / treasury reference 字段。
- Technical Risks:
  - 风险-1: 若 ledger 不回填实际执行引用，模板仍会退化成“漂亮的建议清单”。
  - 风险-2: 若 round meta 不冻结，后续治理复盘会丢失 candidate / window 上下文。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-README-LTRL-001 | LTRL-1/2 | `test_tier_required` | 模板结构、round meta、ledger row 字段完整性检查 | liveops 真实贡献记录一致性 |
| PRD-README-LTRL-002 | LTRL-2/3 | `test_tier_required` | review status、approval 字段与 round summary 检查 | producer 审批可追溯性 |
| PRD-README-LTRL-003 | LTRL-3 | `test_tier_required` | `Actual Amount / Distribution Ref` 回填字段与 archive 流程检查 | 真实发放闭环与后续治理复盘 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-LTRL-001 | 用独立 round ledger 承接真实贡献奖励结算 | 继续把真实发放散落在 issue、聊天和临时表格 | 没有统一 ledger 就没有统一审计面。 |
| DEC-LTRL-002 | ledger 行同时记录建议档位与真实执行引用 | 只记录建议档位，不记录后续发放引用 | 不记录执行引用就无法形成完整闭环。 |
| DEC-LTRL-003 | 允许 `deferred / rejected / distributed` 多状态 | 所有 row 只有“推荐/未推荐”二元状态 | 真实执行一定存在待补资料、被拒或已执行三类分叉。 |
| DEC-LTRL-004 | reward intake 与台账执行层统一只要求 `Reward Account` | 在台账里继续要求独立 claimant 字段 | 这条链路的目标是形成可执行分发行；review 名称层已有 contributor/public handle，可不再追加一层 claimant 字段。 |
| DEC-LTRL-005 | 对于 GitHub PR 来源的贡献，优先从 PR intake block 导入 `Reward Account` | 继续在 ledger 建档后再到评论/私聊里补账户字段 | 让 PR 在提交时就带齐 payout 字段，可以减少二次追问，并让 ledger 更快形成可审阅条目。 |
| DEC-LTRL-006 | 用仓库脚本输出 PR intake 的导入状态与 ledger-ready row | 继续靠 liveops 在每轮 ledger 时手工判断是否 ready/deferred | ledger 的价值是减少临时表格与口头判断；如果 PR import 还靠人工猜，就没有真正形成可复用入口。 |
| DEC-LTRL-007 | merged PR 的周期性奖励归集继续复用单 PR intake contract，并以时间窗批量扫描包装 | 为 round scan 再定义另一套字段或状态；或每轮继续逐个点开 merged PR 肉眼判断 | 周期性归集的价值在于省去重复人工判断，而不是发明第二套 contract；只要 PR template 已固定，batch scan 就应严格复用同一套判定。 |
| DEC-LTRL-008 | merged PR round scan 直接导出 ledger draft markdown 和 csv，但只填充 `ready/deferred` 的初始行与 round meta 占位 | 只给 json/sumary 让 liveops 自己再手抄；或在导出时提前填 producer/distribution 字段 | 本轮要解决的是“首份台账草案怎么更快生成”，不是提前伪造审批或发放结论；导出层应只节省抄录，不替代后续 review/closure。 |
| DEC-LTRL-009 | 普通 merged PR 的默认真实发放 ceiling 收紧到 `150 OC`，`1500 OC` 只留给极少数 exceptional row | 继续让单个 merged PR 在没有 exceptional note 时也能停留在 `1500 OC` 待发放状态 | 当前 contributor reward 必须保持保守，先把普通 merged PR 的金额预期收紧，再把超额 case 变成显式、少量、可审计的例外。 |
| DEC-LTRL-010 | planned grant 在执行前必须接受 actual-value review，必要时可以下调档位或金额 | 文档一旦先写高额计划值，就按原值直接执行 | round ledger 的作用是把审查、纠偏和执行都留下审计痕迹；若发现计划值高于实际价值，应该先下调，而不是带着旧预期直接发放。 |
