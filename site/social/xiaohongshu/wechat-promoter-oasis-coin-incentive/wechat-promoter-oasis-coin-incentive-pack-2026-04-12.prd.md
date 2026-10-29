# oasis7 Xiaohongshu Blogger and WeChat Official Account Oasis Coin Incentive Pack（2026-04-12）

- 对应设计文档: `site/social/xiaohongshu/wechat-promoter-oasis-coin-incentive/wechat-promoter-oasis-coin-incentive-pack-2026-04-12.design.md`
- 对应GitHub Issue/Project task truth: GitHub task issue evidence comments

审计轮次: 2

## 1. Executive Summary
- Problem Statement: 上一版“媒体推广者”范围过宽，把媒体、KOL、搬运号和普通宣传参与者混在一起，执行时很容易失焦。当前更实际的需求是先把激励对象收窄到“小红书博主”和“微信公众号”，让 `liveops_community` 能按这两类最常见、最容易复核的宣传资产直接执行绿洲币审核。
- Proposed Solution: 在 `readme/governance` 整理为独立的 Xiaohongshu blogger and WeChat official account Oasis Coin incentive pack，只定义两类对象、两类渠道内容形态、证据字段、评分逻辑、固定档位金额、审批链、发放回填、反作弊和禁语边界，先把规则做具体、做窄、做可复用。
- Success Criteria:
  - SC-1: 每条进入奖励评审的记录都必须明确属于 `小红书博主` 或 `微信公众号` 之一，并具备 `asset_url`、归档证据、奖励账户和 reviewer。
  - SC-2: 原始阅读量、播放量、点赞量不能单独触发绿洲币奖励建议，所有建议都必须附带内容质量与生态回流说明。
  - SC-3: 对外文案必须明确当前固定档位金额 `300 / 800 / 1500 OC`，同时不得出现“多少阅读量换多少币”“多少播放量换多少币”的换算承诺。
  - SC-4: 每条已批准记录必须填写 `Approval ID`，保留 `Actual Amount` 与 `Distribution Ref` 字段，并可回链到原始小红书笔记或微信公众号文章。批准但尚未实际发放时，后两项值可暂空，状态不得标为 `distributed`；实际发放后必须回填实际金额和发放引用。批准金额仍按本专题固定档位及既有 producer 审批规则执行，字段暂空不改变金额政策。
  - SC-5: 小红书博主与微信公众号两类对象的可计分/不可计分行为边界必须在文档中直接可执行，无需口头补充。

## 2. User Experience & Functionality
- User Personas:
  - `liveops_community`：需要将“小红书博主 / 微信公众号”激励整理为一份能直接执行的绿洲币审核包。
  - `producer_system_designer`：需要确保绿洲币激励奖励的是高质量宣传贡献，而不是流量采购。
  - 小红书博主 / 微信公众号运营者：需要知道什么样的宣传资产能进入奖励建议池。
- User Scenarios & Frequency:
  - 新资产入池：每次出现可验证的小红书笔记或微信公众号文章后记录一次。
  - 轮次复核：每 1-2 周做一次集中评分和 producer 审核。
  - 对外协作说明：每次向潜在博主或公众号解释激励规则前，先用禁语清单复核。
  - 发放回填：每次 producer 批准后，由执行 owner 回填分发结果。
- User Stories:
  - PRD-README-XWPI-001: As a `liveops_community`, I want one scoped scoring framework for Xiaohongshu bloggers and WeChat official accounts, so that the current incentive loop stays concrete and executable.
  - PRD-README-XWPI-002: As a `producer_system_designer`, I want Oasis Coin recommendations tied to content depth, claim safety, and ecosystem return, so that rewards do not collapse into traffic buying.
  - PRD-README-XWPI-003: As a Xiaohongshu blogger or WeChat official account operator, I want clear countable and non-countable behaviors, so that I know how to contribute in a way that genuinely helps the ecosystem.
  - PRD-README-XWPI-004: As a `liveops_community`, I want every rewarded note or article to be traceable back to a real asset and follow-up effect, so that the incentive loop can improve future channel strategy.
- Critical User Flows:
  1. Flow-XWPI-001: `发现小红书笔记或微信公众号文章 -> 记录 channel / asset_url / creator / reward_account -> 进入待筛选池`
  2. Flow-XWPI-002: `检查事实边界与禁语 -> 识别是否存在 overclaim / 抄袭 / 刷量 -> 通过后进入评分`
  3. Flow-XWPI-003: `按“小红书博主 / 微信公众号”与内容深度给基础分 -> 叠加原创性/讨论质量/生态回流加分 -> 输出建议档位`
  4. Flow-XWPI-004: `producer_system_designer 审核建议档位与固定金额 -> 批准或驳回 -> execution owner 回填 actual amount / distribution ref -> 归档`
  5. Flow-XWPI-005: `外部问“发一篇给多少币” -> 使用 safe copy 解释贡献审核制 -> 阻断任何固定汇率或上线承诺`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 对象分层 | `channel_type`、`creator_handle`、`reward_account` | 只允许 `xiaohongshu_blogger` 或 `wechat_official_account` 两类 | `captured -> classified` | 先按对象和渠道分层，再看内容深度 | `liveops_community` 维护 |
| 宣传资产 intake | `asset_url`、`archive_proof`、`publish_at`、`audience_fit_note` | 记录笔记或文章并准备后续评分 | `captured -> screened` | 缺链接或归档证据不得进入评分 | `liveops_community` 维护 |
| 事实边界检查 | `claim_safety_status`、`forbidden_phrase_hit`、`preview_boundary_note` | 过滤越界宣传与错误表述 | `screened -> blocked/passed` | 先过边界，再谈奖励 | `liveops_community` 初筛，`producer_system_designer` 可否决 |
| 评分模型 | `base_score`、`quality_bonus`、`discussion_bonus`、`ecosystem_bonus`、`penalty` | 计算建议档位 | `passed -> scored -> recommended` | 原始阅读量或播放量只能作为辅助说明，不能独立决定档位 | `liveops_community` 初评 |
| 反作弊检查 | `fraud_flag`、`duplicate_flag`、`plagiarism_flag`、`traffic_risk_note` | 标记刷量、抄袭、重复申报 | `screened -> held/rejected/passed` | 命中严重作弊默认驳回 | `liveops_community` 初筛，`producer_system_designer` 审核 |
| 奖励审批与发放 | `recommended_band`、`actual_amount`、`approval_id`、`distribution_ref`、`archive_note` | producer 审批后回填发放结果 | `recommended -> approved/rejected -> distributed -> archived` | `eligible-small=300 OC`、`eligible-medium=800 OC`、`eligible-large=1500 OC`、`no-token-recommendation=0 OC` | `producer_system_designer` 审批，distribution owner 回填 |
| 对外说明模板 | `safe_phrase`、`forbidden_phrase`、`faq_answer` | 规范对小红书博主和公众号主的外部说明 | `draft -> approved -> adopted` | 命中禁语即阻断 | `liveops_community` 起草，`producer_system_designer` 审核 |
- Acceptance Criteria:
  - AC-1: 专题必须明确只覆盖 `小红书博主` 与 `微信公众号` 两类对象。
  - AC-2: 专题必须明确以下行为不能单独获得奖励建议：纯搬运、纯转发、纯阅读量/播放量、买量、互刷、抄袭搬运、单纯情绪吹捧。
  - AC-3: 每条建议记录必须至少包含 `asset_url`、归档证据、事实边界检查、原创性或新增价值说明、reward account。
  - AC-4: 评分模型必须显式包含内容质量、讨论质量、生态回流和反作弊扣分，且 raw reach 不能单独决定奖励档位。
  - AC-5: 奖励档位必须继续使用 `no-token-recommendation / eligible-small / eligible-medium / eligible-large`，并固定映射为 `0 / 300 / 800 / 1500 OC`；不得写成按阅读量或播放量换算。
  - AC-6: 对外模板必须显式阻断 `发一篇就发币`、`按阅读量发币`、`按播放量发币`、`已上线可玩`、`官方买量` 和 `保底发币` 等表述。
  - AC-7: producer 审批与 distribution ref 回填流程必须在文档中显式出现，并可接入现有 reward ledger / distribution 路径。
- Non-Goals:
  - 本专题不开放逐条议价、临时加价或按平台关系单独报价。
  - 不把宣传激励写成广告投放 rate card。
  - 不扩展到其它媒体/KOL/搬运号类型；如果要扩范围，必须另开专题。
  - 不承诺 `oasis7` 已正式上线、开放公测或进入买量阶段。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 该专题位于 `readme/governance`，作为“小红书博主 / 微信公众号”两类宣传对象的激励治理层，承接小红书笔记与微信公众号文章，把外部传播贡献转成可审计的绿洲币奖励建议，再回流 producer 审批与后续 distribution 流程。
- Integration Points:
  - `doc/readme/governance/readme-limited-preview-contributor-reward-pack.prd.md`
  - `doc/readme/governance/readme-limited-preview-contributor-reward-ledger.prd.md`
  - `doc/readme/governance/readme-xiaohongshu-liveops-runbook-2026-03-23.md`
  - `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md`
- Edge Cases & Error Handling:
  - 同一内容同时发小红书和公众号：默认只保留一个主资产 full credit，另一个作为补充证明或小额加分，不重复给 full credit。
  - 高阅读但低质量：若只有阅读量或播放量，没有准确性、原创性或生态回流，默认 `no-token-recommendation`。
  - 越界宣传：若把 `limited playable technical preview` 说成已上线、公测或官方深度集成，直接阻断奖励。
  - 资产被删除或转私密：先进入 `held`，补齐归档证据后再评审。
  - 多人共同产出：允许一条笔记或文章对应多个贡献者，但必须拆清分工并避免重复 full credit。
  - 刷量或抄袭嫌疑：标记 `fraud_flag` 并暂停审批，未澄清前不得发放。
- Non-Functional Requirements:
  - NFR-XWPI-1: 每条非拒绝记录必须包含可访问的笔记/文章链接或可审计归档证据。
  - NFR-XWPI-2: 每条已发放记录必须包含 `Approval ID`、`Actual Amount` 与 `Distribution Ref`。
  - NFR-XWPI-3: 对外“按阅读量/播放量换币”、`保底发币` 与上线 overclaim 命中率必须为 `0`。
  - NFR-XWPI-4: 每轮候选奖励的 producer 复核 SLA 目标为 `<= 7 days`。
  - NFR-XWPI-5: 被判定为买量、互刷、抄袭或虚假宣传的资产，不得漏入已批准发放记录。
- Security & Privacy: 只记录公开账号、公开资产链接、最小必要的奖励账户与审批引用；不得要求博主或公众号主提供不必要的私密身份信息。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP: 固定小红书博主 / 微信公众号两类对象、评分维度、证据字段、审批链、禁语与反作弊规则。
  - v1.1: 跑完首轮真实审核后，根据误判和高价值样本调整权重。
  - v2.0: 如后续确需扩展到其他媒体对象，再另开专题，不在当前包内继续泛化。
- Technical Risks:
  - 风险-1: 如果团队过早把传播激励写成阅读量或播放量兑换，会被刷量和越界宣传迅速污染。
  - 风险-2: 如果评分过于主观，小红书博主与公众号主之间会产生预期混乱。
  - 风险-3: 如果不强制归档证据，已删除资产或平台限流会导致审核无法追溯。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-README-XWPI-001 | TASK-README-067 | `test_tier_required` | 覆盖对象、可计分/不可计分规则与 evidence 字段抽样复核 | 两类渠道激励执行性 |
| PRD-README-XWPI-002 | TASK-README-067 | `test_tier_required` | 评分模型、审批链、反作弊与不按流量买量边界抽样复核 | 绿洲币激励风控与可信度 |
| PRD-README-XWPI-003 | TASK-README-067 | `test_tier_required` | safe phrase / forbidden phrase 与 FAQ 边界抽样复核 | 对外说明一致性 |
| PRD-README-XWPI-004 | TASK-README-067 | `test_tier_required` | `./scripts/doc-governance-check.sh` | 文档互链与治理一致性 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-XWPI-001 | 当前只覆盖小红书博主与微信公众号两类对象 | 一次覆盖所有媒体/KOL/搬运号 | 用户已明确要“精简和具体化”，先收窄到最清晰的两类对象更利于执行。 |
| DEC-XWPI-002 | 按内容深度、事实准确、讨论转化与生态回流审核绿洲币建议 | 按阅读量、播放量或 flat buyout 直接发币 | 用户要的是把宣传方当作参与者和受益者，而不是做粗放买量。 |
| DEC-XWPI-003 | 继续复用 `eligible-small/medium/large` 奖励档位，但固定映射为 `300 / 800 / 1500 OC` | 继续只给档位、不写金额；或改成按阅读量/播放量浮动给币 | 用户已经明确要直接给出不同价值内容能拿多少绿洲币；固定档位金额更便于 liveops 执行，也仍然保留“先审核、后发放”的治理边界。 |
| DEC-XWPI-004 | 小红书与公众号内容先过 claim boundary 和 anti-fraud gate，再进入评分 | 先给流量分，再事后补做口径检查 | 一旦把错误口径或刷量资产纳入奖励，会直接损害 `oasis7` 的公共可信度。 |
