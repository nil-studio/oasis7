# `readme/governance` 热点子域入口

更新时间: 2026-07-04

## 从这里开始
- 想先用一份长说明快速看懂“项目是什么、为什么要做、当前做到哪一步”：先读 `readme-project-overview-whitepaper-2026-04-25.md`
- 想确认 README 口径控制、季度复核或当前公开状态：先读根 `../../../README.md`、`readme-project-overview-whitepaper-2026-04-25.md`、`../../../scripts/readme-link-check.sh` 或 `readme-quarterly-review-cycle-2026-03-11.prd.md`
- 想确认 release communication 的产品边界：先读 `../../product/player-entry-distribution/release-communications-and-public-claims.prd.md`；执行时使用 `readme-release-communication-template.md` 或 `readme-release-announcement-template.md`
- 想确认 Moltbook 渠道边界与公开 claim：先读发行沟通产品分册；持续运营使用 `readme-moltbook-liveops-runbook.md`，帖文与回复素材使用 `readme-moltbook-post-pack.md`
- 想确认 limited playable technical preview 中“参与或贡献不自动形成权益”的产品边界：先读 [`参与和认可边界`](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md)；如需贡献奖励治理、ledger、distribution closure 或 merged PR reward round scan，再读 `readme-limited-preview-contributor-reward-pack.prd.md` 或 `readme-limited-preview-contributor-reward-ledger.prd.md`；invite pack 仅作为历史素材按需检索
- 想确认小红书持续运营、已批准素材包或小红书博主 / 微信公众号激励：先读 `readme-xiaohongshu-liveops-runbook-2026-03-23.md`、`../../../site/social/xiaohongshu/README.md`、`../../../site/social/xiaohongshu/token-usage/token-usage-post-pack-2026-04-20.md`、`../../../site/social/xiaohongshu/future-ownership/future-ownership-post-pack-2026-04-13.md` 或 `../../../site/social/xiaohongshu/wechat-promoter-oasis-coin-incentive/wechat-promoter-oasis-coin-incentive-pack-2026-04-12.md`
- 想确认当前通用资源与领域/模块记录的边界：先读根 `../../../README.md` 与世界规则产品 PRD 的[资源模型与跨模块 provenance](../../product/world-rules-core-gameplay/prd.md#resource-model-and-cross-module-provenance)；区块链、分布式状态和确定性执行边界进入[权威世界基础设施产品 PRD](../../product/world-infrastructure/prd.md)
- 想精确找某份专题文档，而不是按问题阅读：回到 `../prd.index.md`

## 入口分工
- 当前页只承担 `governance/` 子目录 landing page 职责，不复制完整长表。
- `../README.md` 是 `readme` 模块级 landing page，负责跨 `governance / gap / production` 分流。
- `../prd.index.md` 是 `readme` 模块完整文件级索引，适合已知主题后按文件名查找。

## 密度与库存
- 不在入口冻结容易漂移的文件数；当前库存统一以 `./scripts/doc-inventory-report.sh` 为准。
- 当前子域属于 `readme` 模块最高密度热点路径；小红书素材包已迁入 `site/social/xiaohongshu/`，本页保留渠道 runbook 与簇级入口。

## 首读主题簇

### 0. 白皮书式项目总览
- 首读入口:
  - `readme-project-overview-whitepaper-2026-04-25.md`
- 适合问题:
  - `oasis7` 到底是什么项目
  - 为什么它既像游戏、又有 runtime / WASM / consensus 这些重系统结构
  - 当前 `limited playable technical preview` 的公开边界是什么
  - 第一次接触仓库时，先读哪一层再下钻

### 1. 治理控制与季度复核
- 首读入口:
  - `../../../README.md`
  - `readme-project-overview-whitepaper-2026-04-25.md`
  - `../../../scripts/readme-link-check.sh`
  - `readme-quarterly-review-cycle-2026-03-11.prd.md`
- 适合问题:
  - README 对外口径一致性和链接检查该看哪里
  - 季度复核模板和 remediation 节奏在哪
  - 根 README 当前公开状态和后续状态变更该从哪里确认

### 已删除治理专题
- 已删除:
  - `readme-consistency-audit-checklist-2026-03-11.md`
  - `readme-consistency-audit-checklist-2026-03-11.{prd,design,project}.md`
  - `readme-link-check-automation-2026-03-11.{prd,design,project}.md`
  - `readme-root-status-alignment-2026-03-11.{prd,design,project}.md`
- 当前承接:
  - README 顶层链接检查的可执行入口是 `../../../scripts/readme-link-check.sh`，当前治理节奏由 `readme-quarterly-review-cycle-2026-03-11.prd.md` 与 `PR、实际 CI 与评审记录（Issue 按需）` 承接。
  - 根 README 公开状态真值是根 `../../../README.md`；公开状态复核从白皮书总览、季度复核与 release communication surfaces 进入。
- 追溯边界:
  - 如需查看 2026-03-11 一次性专题原文，使用 git history 与 PR、实际 CI 与评审记录（Issue 按需）；不要在当前入口中恢复这些旧专题作为活跃文档。

### 2. Release communication 与 announcement 模板
- 首读入口:
  - `../../product/player-entry-distribution/release-communications-and-public-claims.prd.md`（产品承诺与公开 claim 生命周期）
  - `readme-release-communication-template.md`（内部口径简报操作模板）
  - `readme-release-announcement-template.md`（公告 / changelog 草稿操作模板）
- 适合问题:
  - 后续 release brief / announcement / changelog 的可复用结构该从哪里开始
  - 哪些模板定义 release communication 的默认边界
  - 如何避免把历史 2026-03-11 release-candidate 实例误读为当前发布 lane
  - 当前对外状态仍以根 `../../../README.md` 为准：limited playable technical preview，不是 closed beta 或 public launch

### 3. Moltbook 运营与 follow-up
- 首读入口:
  - `../../product/player-entry-distribution/release-communications-and-public-claims.prd.md`
  - `readme-moltbook-liveops-runbook.md`
  - `readme-moltbook-post-pack.md`
- 适合问题:
  - Moltbook 渠道推广和持续运营该看哪里
  - 已批准主贴、首评、reply boundary 与 follow-up 素材在哪
  - 当前信任修复 / repair certification 讨论链从哪里进入
  - 2026-03 promotion plan 与三组专题包装已退役；历史平台观察、排期和草稿从 Git history、PR 与实际验证记录追溯

### 4. Limited playable technical preview 贡献奖励与台账执行
- 首读入口:
  - [`参与和认可边界`](../../product/player-entry-distribution/participation-and-recognition-boundaries.prd.md)（产品层：可审核贡献与非自动权益边界）
  - `readme-limited-preview-contributor-reward-pack.prd.md`
  - `readme-limited-preview-contributor-reward-ledger.prd.md`
- 适合问题:
  - 有限预览参与、访问、游玩或贡献是否自动形成代币、支付、所有权、治理或后续访问权
  - 贡献奖励、ledger、distribution closure 与 merged PR round scan 当前该从哪里看
  - reward 审批、distribution ref、actual-value review 需要看哪组文档
  - 哪些奖励表述被禁止，如何避免 `play-to-earn`、登录奖励或固定 token 汇率误读

### 历史压缩：release-candidate 与 closed-beta candidate
- 追溯入口:
  - Git history 与对应 PR、实际 CI 与评审记录（Issue 按需）
- 适合问题:
  - 2026-03-11 版本候选实例当时如何生成 release brief / announcement draft
  - 旧 closed-beta-candidate 预备 runbook 如何被并入当前 limited preview invite / execution 入口
- 当前边界:
  - 这些是历史实例或预备 runbook，不再作为默认活跃首读入口。
  - 当前公开状态以根 `../../../README.md` 为准：limited playable technical preview；不是 closed beta、public launch 或正式玩家发布。
  - release communication 产品边界从产品分册进入，操作复用从两份无日期模板进入；四组 2026-03-11 brief/template/draft 专题三件套已退役删除。旧 closed-beta runbook 与 playability 模板已退役删除；当前 claim-control 从 `readme-limited-preview-invite-pack-2026-03-22.md` 与产品分册进入。Round 1 已关闭，其发布、渠道 fallback、信号与关闭证据从 Git history 和 GitHub issue `eng-cc/oasis7#48` 追溯。

### 5. 小红书与外宣激励
- 首读入口:
  - `readme-xiaohongshu-liveops-runbook-2026-03-23.md`
  - `../../../site/social/xiaohongshu/README.md`
  - `../../../site/social/xiaohongshu/token-usage/token-usage-post-pack-2026-04-20.md`
  - `../../../site/social/xiaohongshu/future-ownership/future-ownership-post-pack-2026-04-13.md`
  - `../../../site/social/xiaohongshu/wechat-promoter-oasis-coin-incentive/wechat-promoter-oasis-coin-incentive-pack-2026-04-12.md`
- 适合问题:
  - 小红书持续运营、评论区节奏与信号回流该看哪里
  - 当前已批准的主题帖 / 轮播包 / 素材包入口在哪
  - 小红书博主 / 微信公众号激励边界和证据字段是什么

### 6. 公开定位、世界规则与资源模型
- 首读入口:
  - `../../../README.md`
  - `readme-project-overview-whitepaper-2026-04-25.md`
  - `../../product/world-rules-core-gameplay/prd.md#resource-model-and-cross-module-provenance`
  - `../../product/world-rules-core-gameplay/prd.md`
- 适合问题:
  - README 对外定位与世界规则入口的关系怎么理解
  - 资源模型、世界规则与公开主定位怎样互相约束
  - 哪些专题更适合作为“公开口径主控层”的首读入口

### 已删除世界规则入口收敛专题
- 已删除：`readme-world-rules-consolidation.{prd,design,project}.md`
- 当前承接：根 `../../../README.md` 只维护公开摘要与权威入口，`../../product/README.md` 维护四模块产品树和产品/专业边界，[世界规则与玩法系统产品 PRD](../../product/world-rules-core-gameplay/prd.md)维护长期玩家承诺、世界不变量与跨域验收。
- 追溯边界：该三件套只记录一次已完成的 README 导航收敛、旧 `world-rule.md` 路由与任务过程；历史里程碑从 Git history、PR 与实际验证记录追溯，不在产品树保留迁移包装。

### 已删除资源模型口径修订专题
- 已删除：`readme-resource-model-layering.{prd,design,project}.md`
- 当前承接：根 `../../../README.md` 维护当前公开技术摘要，世界规则与玩法系统产品 PRD 维护长期资源模型与跨模块 provenance；gameplay、runtime 与 WASM 专业合同维护具体规则和证据。
- 追溯边界：该三件套只记录一次已完成的 README 口径修订与 2026-03-03 命名迁移，当前无测试、runbook 或代码依赖；历史任务从 Git history 与 GitHub task evidence 追溯。

## 定向检索边界
- 如果你已经知道准确文件名，直接回 `../prd.index.md`，不要指望本页替代完整索引。
- 旧 2026-03-11 一次性 role handoff briefs 已退役删除；现行规则从正式专题、根 `../../../README.md`、release communication surfaces 与 `../prd.index.md` 进入；原过程从 Git history 与实际 PR/已有 Task 追溯，不依赖退役 `.pm`。
- 旧 `TASK-README-014/015` Moltbook role handoff briefs 已退役删除；当前渠道方法与素材从 `readme-moltbook-liveops-runbook.md`、`readme-moltbook-post-pack.md` 和 `../prd.index.md` 进入；旧 promotion 与 handoff 仅从 Git history 与实际 PR/已有 Task 追溯。
- execution_log、轮播包与其它 supporting doc 继续保留可检索性，但不应重新成为默认首读入口。
- 如果某个主题未来形成新的主文档，应优先进入主文档，而不是继续把增量素材包维持为默认入口。

## 维护约定
- 新增 `governance/` 文档后，若改变了默认首读路径，应同步更新本页。
- 新增小红书素材包时，默认写入 `site/social/xiaohongshu/<post-slug>/`，并在包内 `README.md` 标注可发布导出图、视觉源、复盘图与相关文案文档。
- 本页只维护簇级入口，不维护完整文件清单。
- 若未来 `governance/` 内部继续分裂出更高密度簇，再另开簇内治理专题，而不是把本页扩写成长表。
