# readme PRD

审计轮次: 13

## 目标
- 建立 readme 模块设计主文档，统一需求边界、技术方案与验收标准。
- 确保 readme 模块后续改动可追溯到 PRD-ID、任务和测试。

## 范围
- 覆盖 readme 模块当前能力设计、接口边界、测试口径与演进路线。
- 维护专业需求与适用验证的关系；具体任务与交付事实由实际 PR 或已有 Task 承接。
- 不覆盖实现代码逐行说明与历史过程记录。

## 接口 / 数据
- PRD 主入口: `doc/readme/prd.md`
- 执行记录: Git、PR、实际 CI 与评审记录；Issue/Project 按需。本文不维护可变任务状态。
- 文件级索引: `doc/readme/prd.index.md`
- 追踪主键: `PRD-README-xxx`
- 测试与发布参考: `testing-manual.md`

## 里程碑
以下 M1–M3 为原建档阶段的规划记录，不代表当前进度或新增任务义务；当前交付和过程归属遵循[开发流程规范](../engineering/workflow/source-of-truth.md#information-record-placement)。
- M1 (2026-03-03): 完成模块设计 PRD 主体重写与任务改造。
- M2: 补齐模块设计验收清单与关键指标。
- M3: 建立 PRD-ID -> Task -> Test 的长期追踪闭环。

## 风险
- 模块边界演进快，文档同步可能滞后。
- 指标口径不稳定会降低验收一致性。

### 历史重号条款的兼容定位

本文保留历史 `AC-15` 与 `DEC-RM-039` 编号及各条原义务，不将同号条款合并，也不重排其他编号。旧编号仅作兼容标签，不能单独作为唯一条款定位；引用这些条款时使用下列“文档路径 + 主题锚点”，并按对应主题判定。旧引用若未给主题或原文范围，须先核实目标，不能默认指向同号第一项。

| 历史编号 | 原条款主题 | 唯一定位 |
| --- | --- | --- |
| AC-15 | PR intake 导入 | [PR intake 导入](#ac-15-reward-pr-intake) |
| AC-15 | 第四帖玩家控制边界 | [第四帖玩家控制边界](#ac-15-xiaohongshu-player-boundary) |
| AC-15 | 第三帖游戏介绍 | [第三帖游戏介绍](#ac-15-xiaohongshu-game-intro) |
| DEC-RM-039 | Reward Account 字段 | [Reward Account 字段](#dec-rm-039-reward-account) |
| DEC-RM-039 | 宣传激励范围与档位 | [宣传激励范围与档位](#dec-rm-039-channel-incentive) |

## 1. Executive Summary
- Problem Statement: README 与相关入口文档长期承载架构、运行、规则、发布口径，历史上容易出现口径漂移与链接失效。
- Proposed Solution: 将 readme 模块定义为“对外口径主控层”，统一入口信息、跨文档引用、术语定义与更新策略。
- Success Criteria:
  - SC-1: README 关键章节与模块 PRD 引用一致率达到 100%。
  - SC-2: 对外入口链接有效性检查持续通过。
- SC-3: 术语与架构描述变更在 1 个工作日内同步到 README 体系。
- SC-4: readme 相关变更能追溯到受影响的专业要求、实际 PR 和验证记录；Trace ID 按现行增量规则维护，不强制另建 Issue 或任务标识。
- SC-5: Limited preview invite pack 与 current signal / incident rows 能直接维持 `limited playable technical preview` 口径，并服务 `PRD-GAME-009` 的 evidence gate。
- SC-6: limited preview 贡献奖励流程必须具备可直接复用的 round ledger 模板，能够承接评分、审批、发放记录与归档。
- SC-7: 小红书博主 / 微信公众号激励流程必须把宣传方视作生态参与者与受益者，并以内容质量、事实准确性、讨论转化和生态回流作为绿洲币激励依据，固定映射 `300 / 800 / 1500 OC` 档位，而不是按播放量粗放买量。
- SC-8: 根 README、世界规则入口与站点首页的公开定位在 1 个任务内完成“系统/引擎 -> 游戏”同步，不再出现对外首屏主语分叉。

## 2. User Experience & Functionality
- User Personas:
  - 新贡献者：需要快速理解系统边界与入口。
  - 外部评审者：需要准确获取当前实现状态与能力。
  - 维护者：需要低成本维护跨文档一致性。
- User Scenarios & Frequency:
  - 新人入项阅读：入项首日必读，建立整体认知。
  - 对外评审准备：每次外部评审前执行入口核对。
  - 文档同步巡检：每周至少 1 次。
  - 发布前口径复核：每个版本候选至少 1 次。
- User Stories:
  - PRD-README-001: As a 新贡献者, I want a reliable top-level narrative, so that onboarding time is reduced.
  - PRD-README-002: As an 评审者, I want consistent architecture statements, so that technical due diligence is faster.
  - PRD-README-003: As a 维护者, I want explicit sync rules, so that docs do not drift.
- PRD-README-004: As a `liveops_community`, I want an external communication brief anchored to internal release evidence, so that public-facing messaging stays consistent with current candidate status and risk boundaries.
- PRD-README-005: As a `liveops_community`, I want a reusable release communication template, so that future candidate briefs follow the same structure, evidence links, and review chain.
- PRD-README-006: As a `liveops_community`, I want an announcement/changelog draft derived from approved messaging, so that formal external copy can start from a safe, audited baseline.
- PRD-README-007: As a `liveops_community`, I want a reusable announcement/changelog template, so that future external drafts follow the same sections, source links, and review states.
- PRD-README-008: As a 仓库访客, I want the root README to reflect the current preview posture, so that I do not mistake the repo for a live release landing page.
- PRD-README-009: As a `producer_system_designer`, I want repo-home copy aligned with site and communication docs, so that public promises stay consistent.
- PRD-README-010: As a `liveops_community`, I want a stable channel runbook anchored to freshly verified platform mechanics and the current product claim envelope, so that outbound seeding can fit channel culture without over-promising or reusing stale platform assumptions.
- PRD-README-011: As a `liveops_community`, I want a reusable Moltbook post pack governed by the stable runbook, so that native posts and replies can be reviewed, published, withdrawn and traced without improvising unsafe copy.
- PRD-README-012: As a `liveops_community`, I want Moltbook outreach to point builders back to GitHub issues and PRs after they inspect the preview, so that external interest can turn into actionable feedback and contributions.
- PRD-README-013: As a `liveops_community`, I want shorter feed-native Moltbook variants of the approved post pack, so that final publish copy reads like native posts instead of internal drafts.
- PRD-README-014: As a 仓库访客, I want the repo root and public entry docs to use the canonical `oasis7` brand while still explaining legacy internal identifiers, so that I do not confuse project branding with crate/bin compatibility names.
- PRD-README-015: As a `liveops_community`, I want a Moltbook liveops runbook for post-publish checks, replies, and signal triage, so that day-2 channel operations do not fall back to unsafe improvisation.
- PRD-README-016: As a `liveops_community`, I want a first-week Moltbook operating template layered onto the runbook, so that the first 7 days of channel activity have a concrete daily rhythm instead of generic SOP only.
- PRD-README-017: As a `liveops_community`, I want a limited preview invite pack plus signal/incident fields, so that recruitment, feedback, and incident signals stay within the technical preview envelope while still feeding the unified release gate.
- PRD-README-018: As a `liveops_community`, I want real Moltbook execution lessons written back into the runbook, so that future posts can reuse what triggered discussion and avoid what triggered spam.
- PRD-README-019: As a `liveops_community`, I want a controlled limited preview execution pack, so that the first invite-only builder round can run with fixed callout copy, monitoring slots, signal buckets, and producer-facing summary fields.
- PRD-README-020: As a `liveops_community`, I want an early contributor reward pack for limited preview, so that reward eligibility, evidence fields, score bands, and forbidden phrases stay contribution-based and do not depend on invite-only or play-to-earn framing.
- PRD-README-021: As a `liveops_community`, I want a reusable Xiaohongshu intro post pack from the human developer perspective, so that the account can open with a clear identity, safe tone, and repeatable asset set instead of improvising its first post.
- PRD-README-022: As a `liveops_community`, I want a round-based contributor reward ledger template, so that real limited preview contributions can be reviewed, approved, distributed, and archived without falling back to ad-hoc notes.
- PRD-README-023: As a `liveops_community`, I want a reusable Xiaohongshu team-roster post pack, so that the second post can explain the current agent team structure with human-facing clarity instead of drifting into either dry role docs or vague AI talk.
- PRD-README-024: As a `liveops_community`, I want a Xiaohongshu liveops runbook aligned with the Moltbook model, so that human-facing channel operations can reuse a stable SOP for post checks, reply boundaries, interaction prompts, and signal feedback loops.
- PRD-README-025: As a `liveops_community`, I want a reusable Xiaohongshu game-intro post pack, so that the third post can explain what kind of game `oasis7` is in human-facing language without collapsing into either a full world-rule dump or unsafe release claims.
- PRD-README-026: As a `liveops_community`, I want a reusable Xiaohongshu player-boundary post pack, so that the fourth post can explain why players guide the world instead of directly puppeting one role, without slipping into either control-scheme jargon or unsafe playability claims.
- PRD-README-027: As a `liveops_community`, I want a reusable Xiaohongshu AI-laziness-to-game-mode post pack, so that the fifth post can start from a familiar AI-era feeling and pull the discussion toward what games should still ask humans to judge, choose, and carry.
- PRD-README-028: As a `liveops_community`, I want a reusable Xiaohongshu spring-recruit post pack from the studio lead perspective, so that we can borrow current job-market anxiety to talk about judgment, delivery, and player sense without drifting into either generic job-board content or hollow motivation copy.
- PRD-README-029: As a `liveops_community`, I want a reusable Xiaohongshu spring-recruit carousel pack derived from the approved long-form post, so that the same topic can publish as a mobile-native swipe format with stronger retention and comment prompts instead of only one long caption block.
- PRD-README-030: As a `liveops_community`, I want a reusable Xiaohongshu AI-persona-vs-world-actor post pack, so that we can borrow the current `AI人格` discussion heat to explain why `oasis7` wants agents that can act inside the world instead of only sounding like companions, without drifting into companion-product framing or unsafe release claims.
- PRD-README-031: As a `liveops_community`, I want a reusable Xiaohongshu AI-persona carousel pack derived from the approved seventh post, so that the same topic can publish as a 4-page mobile-native swipe deck with clearer pauses between “会聊天”“会行动”“如何判断” and the final comment hook, instead of only one cover plus long caption.
- PRD-README-032: As a `liveops_community`, I want a reusable Xiaohongshu anti-demo-hype post pack from the game-maker perspective, so that we can借“AI demo 很猛”这类高频讨论，讲清楚我为什么越来越看重稳定、收口和能不能真的放进项目，而不是只被第一眼高光打动。
- PRD-README-033: As a `liveops_community`, I want a reusable Xiaohongshu demo-skepticism carousel pack derived from the approved eighth post, so that the same topic can publish as a 4-page mobile-native swipe deck with clearer pauses between “先停住”“demo vs project”“我会先问什么” and the final comment hook, instead of only one industrial cover plus long caption.
- PRD-README-034: As a `liveops_community`, I want a reusable Xiaohongshu GUI-retreat post pack from the game-maker perspective, so that we can borrow the current `GUI已死` discussion heat to explain why `oasis7` no longer treats GUI as the primary interaction layer, without sliding into generic industry prophecy, front-end job panic, or unsafe “full automation” claims.
- PRD-README-035: As a `liveops_community`, I want a reusable Xiaohongshu offer-choice post pack from the mentor / project-owner perspective, so that we can borrow the current `offer 焦虑` discussion heat to explain why fresh graduates in engineering often should prioritize platform, training, mentorship, and engineering fundamentals over a岗位名看起来更像风口的第一份工作, without drifting into anti-AI absolutism, generic career coaching, or recruiting claims.
- PRD-README-036: As a `liveops_community`, I want a reusable Xiaohongshu offer-choice carousel pack derived from the approved tenth post, so that the same topic can publish as a 4-page mobile-native swipe deck with clearer pauses between “冲突题”“优先级判断”“成熟团队会带来什么” and the final comment hook, instead of only one long caption block.
- PRD-README-037: As a `liveops_community`, I want a reusable Moltbook hot-topic trust-repair post pack aligned to the platform's current `operator / trust / memory authenticity / accountability` discussion wave, so that we can join active channel discourse with native copy while still keeping the `limited playable technical preview` boundary explicit.
- PRD-README-038: As a `liveops_community`, I want a reusable Xiaohongshu Kaifeng real-world RPG post pack from the game-maker perspective, so that we can borrow the现场真人 NPC / 沉浸式互动 discussion heat to explain why many people真正上头的不是“看景”而是“被快速拉进剧情”，without drifting into travel-guide framing, generic scenic praise, or unsafe project release claims.
- PRD-README-039: As a `liveops_community`, I want a reusable Moltbook repair-certification follow-up post pack that extends the proven trust-repair thread, so that we can push the discussion from `repair cost` into `who gets to verify repair` without drifting into generic trust-theory abstraction, formal integration claims, or unsafe release language.
- PRD-README-040: As a `liveops_community`, I want a reusable Xiaohongshu cycle-crossing post pack for people already in existing roles, so that we can borrow the current `AI 来得太快 / 怎么穿越周期` discussion heat to explain why the real risk is usually not the job title disappearing overnight but the low-judgment layer inside the role being compressed first, without drifting into macro trend sermon, generic motivation copy, or unsafe project release claims.
- PRD-README-041: As a `liveops_community`, I want a dedicated Xiaohongshu cover asset for the cycle-crossing post pack, so that the twelfth post can ship with a stronger first-screen hook that still keeps the `穿越周期` topic anchor and the “岗位名不会先变、先变的是岗位里那层工作” judgment boundary, without drifting into panic copy, sci-fi AI cliches, or generic励志海报感。
- PRD-README-042: As a `liveops_community`, I want a reusable Xiaohongshu cycle-crossing carousel pack derived from the approved twelfth post, so that the same topic can publish as a 4-page mobile-native swipe deck with clearer pauses between “岗位名不会先变”“真正该问什么”“别等工具来找你” and the final comment hook, instead of only one cover plus long caption.
- PRD-README-043: As a `liveops_community`, I want a reusable Xiaohongshu blogger and WeChat official account Oasis Coin incentive pack, so that these two priority channel types can be reviewed as participants and beneficiaries through auditable contribution records rather than flat buyout or raw-view payouts.
- PRD-README-044: As a `liveops_community`, I want a merged-PR reward round scan script, so that one reward review window can batch-import candidate PR rows from the existing template contract instead of reopening each PR manually.
- PRD-README-045: As a `liveops_community`, I want a reusable Xiaohongshu ownership-and-beneficiary post pack framed around a利益共同体, so that we can use “谁该拥有游戏的所有权、谁该成为受益者” to explain why developers, players, and high-quality creators should be treated as long-term participants in the game's value distribution without drifting into already-live voting-right claims, governance-token overclaim, or generic community-slogan copy.
- PRD-README-046: As a 仓库访客 / 外部评审者, I want README、世界规则入口与站点首页统一把 `oasis7` 定位为“文明模拟游戏”, so that public entry docs no longer split between system, engine, and game framing.
- PRD-README-047: As a `liveops_community`, I want a reusable Xiaohongshu token-usage post pack grounded in the real long-running project账单, so that we can use cumulative request/token/cost data to explain why AI in a real project more像研发流程参与成本而不是一次性生成炫技, without drifting into burn-money showoff, fundraising signaling, or exaggerated automation claims.
- PRD-README-048: As a 仓库访客 / 外部评审者, I want one explanation-style overview that reads like a whitepaper without pretending to be a new source of truth, so that I can understand what `oasis7` is, why it exists, how the world/game/system fit together, and what the current preview boundary really is before I dive into module PRDs.
- PRD-README-049: As a 站点访客 / 外部评审者, I want the public site to reflect the same framing as the whitepaper overview and expose a direct entry to it, so that I can understand the project's vision, two-stage path, and preview boundary from the landing surfaces before I fall back to repo-native docs.
- Critical User Flows:
  1. Flow-RM-001: `阅读 README -> 跳转模块入口 -> 快速定位目标能力`
  2. Flow-RM-002: `检测口径变更 -> 更新入口文档 -> 校验链接 -> 发布同步`
  3. Flow-RM-003: `发布前执行巡检 -> 汇总冲突 -> 修复后复核`
  4. Flow-RM-004: `读取第三方平台当前机制 -> 绑定内部 claim envelope -> 生成平台适配推广方案 -> 回流 owner 审核`
  5. Flow-RM-005: `发布 Moltbook 帖子 -> 检查 /home / notifications / comments -> 分级回复或升级 -> 记录到实际 PR 或已有 Task`
  6. Flow-RM-006: `按首周模板安排 day1-day7 发帖 / 巡检 / 跟评 / 周复盘 -> 将可复用方法沉淀到 runbook，真实信号与本轮安排记录到实际 PR 或已有 Task`
  7. Flow-RM-007: `复盘真实帖子表现 -> 提炼有效讨论钩子与 spam 触发条件 -> 回写 runbook -> 调整下一帖策略`
  8. Flow-RM-008: `冻结 limited preview 口径 -> 选用 invite-only callout copy -> 按固定窗口巡检 -> 将信号按 Blocking / Opportunity / Idea 归档 -> 输出 producer 摘要`
  9. Flow-RM-009: `收集 early contributor signal -> 按评分模板记录证据 -> 输出 small/medium/large reward recommendation -> 检查对外禁语 -> 回流 producer 审核`
  10. Flow-RM-010: `确定小红书账号第一帖定位 -> 冻结标题/正文/封面/标签 -> 对齐“人类开发者 + agent 队友”叙事主语 -> 保存可复用素材包`
  11. Flow-RM-011: `确定第二篇队友介绍轮播结构 -> 为每位 agent 收口一句专业但有人味的角色说明 -> 产出轮播卡与可截图 HTML`
  12. Flow-RM-012: `结束一轮 limited preview -> 将可计分贡献抄入 reward ledger -> producer 审批档位 -> execution owner 回填发放记录 -> 台账归档`
  13. Flow-RM-013: `发布小红书内容 -> 按 T+15m / T+1h / T+4h / T+24h 巡检 -> 按状态误解 / 过度猜想 / 高质量互动分级 -> 回流下篇选题`
  14. Flow-RM-014: `确定第三篇“游戏是什么”表达 -> 冻结标题/正文/轮播页/互动问题 -> 用人类开发者口吻给出可想象的游戏轮廓 -> 保持 limited playable technical preview 边界`
  15. Flow-RM-015: `确定第四篇“玩家为什么不能直接控制角色”表达 -> 冻结标题/正文/轮播页/互动问题 -> 用人类开发者口吻解释玩家与 agent 的控制边界 -> 保持 limited playable technical preview 边界`
  16. Flow-RM-016: `确定第五篇“AI时代，你变"懒"了么”表达 -> 从熟悉的 AI 使用感切入 -> 收口到“游戏模式里什么还该交给人” -> 冻结标题/正文/轮播页/互动问题并避免滑向泛 AI 焦虑帖`
  17. Flow-RM-017: `确定第六篇“作为游戏工作室主理人，今年的春招视角”表达 -> 借春招/求职焦虑框架讲团队判断标准 -> 冻结标题/正文/封面/互动问题并避免滑向岗位汇总或空泛鸡汤；若平台标题上限受限，优先保证“游戏工作室主理人 + 春招”识别点`
  18. Flow-RM-018: `将第六篇长文版压成轮播版 -> 拆成封面/判断前提/核心标准/收束提问 6 页 -> 导出逐页 PNG -> 保持标题、关键词和互动问题一致`
  19. Flow-RM-019: `确定第七篇“AI人格很火，但我不想做陪聊搭子”表达 -> 借 AI人格 热词讲清“人格不只是会聊天，而是会在世界里行动、判断、协作和承担后果” -> 冻结标题/正文/互动问题并避免滑向情感陪伴产品讨论或完整可玩承诺`
  20. Flow-RM-020: `将第七篇长文版压成 4 页轮播版 -> 拆成封面/核心判断/行为例子/评论区站队页 -> 导出逐页 PNG -> 保持标题、边界与互动问题一致`
  21. Flow-RM-021: `将第十篇长文版压成 4 页轮播版 -> 拆成封面冲突/优先级判断/成熟团队价值/评论区站队页 -> 导出逐页 PNG -> 保持“平台优先、不反 AI、趋势会快速扩散”的边界一致`
  22. Flow-RM-022: `收集小红书笔记或微信公众号文章 -> 按渠道与内容深度分类 -> 校验事实边界与反作弊信号 -> 计算绿洲币奖励建议档位 -> producer 审核 -> 回填 actual amount / distribution ref 并归档`
  23. Flow-RM-023: `进入一轮 contributor reward review -> 按 merged 时间窗扫描已合入 PR -> 复用 reward intake contract 产出 ready/deferred/no_reward_review_requested/invalid_intake 候选 -> 再把 ready/deferred 行抄入 round ledger`
  24. Flow-RM-024: `发现 README / site / world-rule 的公开定位分叉 -> 先冻结“文明模拟游戏”主口径 -> 同步回写中英首页与规则入口 -> 校验技术预览边界未漂移`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 顶层入口导航 | 模块名称、入口链接、摘要 | 点击跳转模块文档 | `draft -> published -> refreshed` | 入口按模块矩阵排序 | 所有人可读，维护者可更新 |
| 口径一致性同步 | 术语、架构描述、更新时间 | 检测冲突并回写更新 | `detected -> synced -> verified` | 核心入口优先同步 | 文档 owner 审核生效 |
| 链接可用性巡检 | 链接地址、状态、修复建议 | 自动检查并输出报告 | `checked -> broken/fixed` | 断链优先修复 | 维护者可处理 |
| 平台化推广方案 | `platform`、`audience`、`content_pillars`、`claim_boundary`、`cta`、`signal_tags` | 生成渠道适配的推广/运营方案 | `draft -> reviewed -> approved` | 先写平台机制，再写口径边界，再写动作节奏 | `liveops_community` 起草，`producer_system_designer` 审核 |
| 渠道运营 runbook | `check_window`、`signal_bucket`、`reply_boundary`、`escalation_owner`、`log_requirement` | 固化第三方渠道发帖后运营 SOP | `draft -> approved -> adopted` | 先定义检查顺序，再定义回复与升级边界 | `liveops_community` 维护，`producer_system_designer` 审核边界 |
| 首周运营模板 | `day_id`、`primary_post`、`check_slots`、`reply_goal`、`log_focus` | 把抽象 runbook 压实到 day1-day7 的执行模板 | `planned -> executed -> reviewed` | 先 identity / surfaces，再 proof / diary / builder hook，再 recap | `liveops_community` 执行，`producer_system_designer` 审核边界 |
| Closed Beta Candidate Runbook | `candidate_signal`、`release_gate_link`、`response_template`、`incident_level` | 招募、反馈、事故模板、FAQ | `tech_preview -> candidate_runbook -> gate_ready` | runbook步骤优先，监测其次 | `liveops_community` 维护，`producer_system_designer` 决定口径 |
| 实战运营经验 | `post_id`、`working_hook`、`spam_trigger`、`next_adjustment` | 把真实发帖结果沉淀成后续可复用规则 | `observed -> distilled -> adopted` | 先记录高信号模式，再记录高风险模式 | `liveops_community` 维护 |
| 受控预览执行包 | `round_id`、`callout_copy`、`check_slot`、`signal_bucket`、`claim_drift_flag`、`summary_field` | 把 limited preview 第一轮执行压成可直接照跑的操作包 | `draft -> execution_ready -> reused` | 先冻结文案，再冻结巡检，再冻结回流摘要 | `liveops_community` 维护，`producer_system_designer` 审核边界 |
| 早期贡献奖励操作包 | `reward_account`、`contribution_type`、`score_band`、`evidence_field`、`reward_recommendation`、`forbidden_phrase`、`intake_surface`、`import_script` | 按贡献评分模板判断是否进入奖励建议池，并约束对外表达 | `signal -> scored -> reviewed -> recommended` | `Reward Account` 仅作执行字段；若来源是 GitHub PR，则通过可选 PR intake block 收集 `Reward Account`，并允许导入脚本直接解析；不公布固定 token/point 比率；仅按贡献审计后给 `eligible-small/medium/large` 建议 | `liveops_community` 记录与初评，`producer_system_designer` 最终审批 |
| 贡献奖励台账 | `round_id`、`candidate_id`、`ledger_id`、`reward_account`、`recommended_band`、`review_status`、`approval_id`、`distribution_ref`、`import_status` | 将真实贡献逐条编目、审批、回填发放记录并归档 | `draft -> reviewed -> approved/rejected -> distributed -> archived` | 同一轮按 contributor 与 ledger id 去重；缺证据默认不得进入审批；PR import 至少区分 `ready / deferred / no_reward_review_requested / invalid_intake` | `liveops_community` 维护，`producer_system_designer` 审核，execution owner 回填发放记录 |
| Merged PR Reward Round Scan | `merged_after`、`merged_before`、`search_query`、`status_counts`、`entry.pr_number`、`entry.merged_at`、`entry.import_status` | 批量扫描一轮 merged PR，并把 template 中的 reward intake block 转成 ledger 候选 | `window_selected -> scanned -> triaged -> imported` | 必须复用单 PR intake contract；默认只把 `ready/deferred` 推进到 ledger，`no_reward_review_requested/invalid_intake` 只保留扫描报告 | `liveops_community` 执行，`producer_system_designer` 消费汇总结果 |
| 小红书博主 / 微信公众号绿洲币激励包 | `channel_type`、`asset_url`、`claim_safety_status`、`proof_bundle`、`reach_quality`、`ecosystem_return`、`fraud_flag`、`recommended_band`、`actual_amount` | 将小红书笔记与微信公众号文章按质量、准确性、讨论转化与生态回流纳入奖励审核 | `captured -> screened -> scored -> reviewed -> approved/rejected -> distributed` | 原始阅读量或播放量不能单独触发奖励；必须先过事实边界与反作弊检查，再按内容深度和回流质量给档位，并固定映射 `eligible-small=300 OC`、`eligible-medium=800 OC`、`eligible-large=1500 OC` | `liveops_community` 记录与初审，`producer_system_designer` 审批，distribution owner 回填发放记录 |
| 小红书帖子素材包 | `post_goal`、`title`、`body`、`cover_copy`、`carousel_outline`、`interaction_prompt`、`forbidden_phrase` | 固化单篇小红书可复用文案与轮播结构 | `draft -> reviewed -> ready_for_publish -> published` | 先确定单一帖子目标，再冻结标题/正文/互动问题；正文先给人可理解轮廓，再避免 world-rule dump 与上线口径 | `liveops_community` 起草，`producer_system_designer` 审核边界 |
- Acceptance Criteria:
  - AC-1: readme PRD 明确入口文档职责边界。
  - AC-2: 实际 PR 或已有 Task 维护同步任务、结论与关闭证据；长期文档维护规则和入口。
  - AC-3: README 与 `doc/product/world-rules-core-gameplay/prd.md`、`testing-manual.md`、模块 PRD 的链接链路可用。
  - AC-4: 口径更新有明确触发条件与同步时限。
  - AC-5: 渠道化推广方案必须显式绑定内部公开口径边界，不得把 generic marketing 文案直接外推到第三方平台。
  - AC-6: `doc/readme/governance/**` 仍可读历史专题的首行标题必须统一使用 `oasis7` 品牌；旧品牌标题仅允许保留在正文历史上下文与证据原文中。
  - AC-7: `doc/readme/governance/**` 中仍作为当前公开口径使用的项目名必须统一写为 `oasis7`；旧品牌名仅允许保留在历史证据、兼容说明或外部原文引用中。
- AC-8: 若第三方渠道进入持续运营阶段，必须补齐独立 runbook，明确巡检入口、回复边界、升级路径与实际 PR/已有 Task 的证据记录方式。
- AC-9: 若渠道进入首周冷启动执行阶段，runbook 必须补齐 day1-day7 模板，明确每天的主动作、检查窗口、回复目标与记录重点。
- AC-10: 已将旧 `closed beta candidate` runbook 与 incident template 收敛到 `readme-limited-preview-invite-pack-2026-03-22.md`，供招募/反馈/事故信号在 `limited playable technical preview` claim envelope 内沟通并可直接回流 unified release gate。
  - AC-11: Moltbook runbook 至少记录一轮真实执行后的“有效讨论钩子”和“高风险 spam 触发模式”，并明确下一轮如何调整。
- AC-12: 若团队进入 `limited playable technical preview` 的 invite-only 执行阶段，必须补齐受控执行包，明确 callout 文案、巡检窗口、信号分桶、claim drift 纠偏与 producer 摘要字段。
- AC-13: 若团队决定在 limited preview 阶段使用 early contributor reward，必须补齐独立操作包，明确 `Reward Account` 作为执行字段、评分模板、证据字段、审阅链、禁语与“无固定 token/point 汇率”边界；若贡献来源是 GitHub PR，则必须提供可选 intake block 收集 `Reward Account`。
- AC-14: 若团队开始记录真实贡献奖励轮次，必须补齐 round-based ledger，明确 round meta、`Reward Account`、逐条 row 状态、producer 审批引用、distribution ref 与 archive note；PR 来源行必须能追溯到 PR intake 中的 `Reward Account` 或显式补录记录。
- AC-15: <a id="ac-15-reward-pr-intake"></a>若 GitHub PR intake 已成为重复使用的贡献入口，仓库必须提供脚本化导入路径，将 PR body 中的 reward intake block 解析为 ledger-ready 结构化字段，并显式区分 `ready / deferred / no_reward_review_requested / invalid_intake`，不得把 raw `public_key` 重新引回用户面命名。
- AC-15: <a id="ac-15-xiaohongshu-player-boundary"></a>若小红书内容链路进入第四篇，必须补齐“玩家控制边界”素材包，明确标题、正文、轮播结构、互动问题，并把“玩家不能直接控制角色，但能给方向”的正式口径收进人类开发者叙事。
- AC-16: 若小红书内容链路进入第五篇，必须补齐“AI时代，你变"懒"了么”素材包，明确如何从 AI 使用习惯切到游戏模式讨论，并保持人类开发者视角与 `limited playable technical preview` 边界。
- AC-17: 若小红书内容链路进入第六篇，必须补齐“作为游戏工作室主理人，今年的春招视角”素材包，明确标题、正文、封面、互动问题、关键词与禁滑坡边界，并把春招热点收口到团队判断、完成度、AI 使用方式与玩家感觉；若平台标题长度受限，最终发布标题必须优先保留“游戏工作室主理人”和“春招”识别点。
- AC-18: 若第六篇需要轮播版，必须补齐独立轮播素材包，明确页数、逐页文案、HTML、逐页 PNG 与评论区收束页分工，并保持与长文版标题、边界和互动问题一致。
- AC-19: 若小红书内容链路进入第八篇，必须补齐“做AI游戏以后，我越来越不信demo了”素材包，明确标题、正文、短版文案、互动问题、关键词与“不是不信 AI，而是不再轻易被 demo 高光说服”的表达边界，并保持人类开发者第一人称与非上线口径。
- AC-20: 若第八篇需要独立封面图，必须补齐 `1080x1440` 的 HTML 与 PNG，视觉上要明显区别于前几篇的暖纸面判断卡，更接近“项目审查板 / build review wall”，并保持“先问它能撑多久”的主判断不被装饰稀释。
- AC-21: 若第八篇需要轮播版，必须补齐独立轮播素材包，明确 4 页逐页文案、HTML、逐页 PNG 与评论区收束页分工，并沿用第八篇的工业审查板 / build review wall 视觉语言，不回退到前几篇的暖纸便签风。
- AC-22: 若小红书内容链路进入第九篇，必须补齐“GUI已死？这次我是认同的”素材包，明确标题、正文、互动问题、关键词与“死掉的不是屏幕，而是 GUI 作为主执行入口”边界，并保持人类开发者第一人称、非上线口径与“玩家负责判断、系统负责执行”的项目收束。
- AC-23: 若小红书内容链路进入第十篇，必须补齐“AI岗和大厂后端怎么选”素材包，明确标题、正文、短版备选、互动问题、关键词与“对应届生先拿平台 / 训练体系 / 工程基本功 / 优秀同事密度，不是否定 AI 趋势，且传统团队的 AI 转向可能比想象中更快”的表达边界，并保持人类开发者第一人称、非招聘口径与非绝对化择业建议。
- AC-24: 若第十篇需要轮播版，必须补齐独立轮播素材包，明确 4 页逐页文案、HTML、逐页 PNG 与评论区收束页分工，并沿用“offer decision memo / 决策档案”方向把讨论收口到“平台 / 训练体系 / 带教 / 工程基本功 / 优秀同事密度优先，不是否定 AI 趋势”，不回退成泛求职鸡汤或科技感海报。
- AC-25: 若小红书内容链路进入第十一篇，必须补齐“做游戏的人去开封，看见的是一套低门槛实景RPG”素材包，明确标题、正文、封面文案、互动问题、关键词与“最吸引人的不是看景，而是尽快进剧情 / 世界要能快速接住人 / 压低入戏门槛”的表达边界，并保持人类开发者第一人称、非旅游攻略口径与非上线宣称。
- AC-27: 若小红书内容链路进入第十二篇，必须补齐“AI来得这么快，怎么穿越周期”素材包，明确标题、正文、短版备选、互动问题、关键词与“AI 先压缩的是岗位里重复、标准、低判断的部分 / 穿越周期靠把自己往问题定义、流程重构、结果负责这一层移动”的表达边界，并保持人类开发者第一人称、非宏大趋势宣讲口径、非鸡汤口径与非上线宣称。
- AC-28: 若第十二篇需要独立封面图，必须补齐 `1080x1440` 的 HTML 与 PNG，视觉上要保持“穿越周期”题眼与“岗位 vs 那层工作”的判断钩子同时成立，采用更像 editorial 判断海报的层级抬升构图，不回退到蓝紫科幻 AI、机器人吞岗位、或泛励志海报式表达。
- AC-29: 若第十二篇需要轮播版，必须补齐独立轮播素材包，明确 4 页逐页文案、HTML、逐页 PNG 与评论区收束页分工，并沿用“暖纸面 + editorial 诊断卡 + 层级上移提示”的视觉语言，把讨论收口到“别等工具来找你 / 先看清岗位里哪层工作在变 / 人要往更高判断层移动”，不回退成恐慌式 AI 预言或泛职场鸡汤。
- AC-30: 若团队要对小红书博主或微信公众号启用绿洲币激励，必须补齐独立激励包，明确两类对象的可计分/不可计分行为、证据字段、固定档位金额、审批链、发放回填、反作弊与禁语边界，并明确“宣传方是生态参与者与受益者，但激励不按阅读量、播放量或买量口径粗放发放”。
- AC-31: 若 merged GitHub PR 已成为 reward ledger 的周期性来源，仓库必须提供按 merged 时间窗批量扫描的脚本入口，复用现有 reward intake template contract，并显式输出 `ready / deferred / no_reward_review_requested / invalid_intake` 汇总与逐条来源信息。
- AC-32: 若团队继续对 merged GitHub PR 做贡献奖励审批，必须把普通 merged PR 的默认真实发放 ceiling 收紧到 `150 OC`；任何 `>150 OC` 的 row 都必须留下 exceptional case note，`1500 OC` 不得作为常规 MR 档位扩散。
- AC-33: 若 contributor reward row 已进入 planned grant 或 pending distribution，producer 在执行前仍必须按实际增量价值复核；若文档里的原计划金额偏高，必须先下调档位或金额并留下审计说明，而不是按原值直接发放。
- AC-34: 若小红书内容链路进入第十三篇，必须补齐“谁该拥有游戏的所有权 / 谁该成为受益者”素材包，明确标题、正文、互动问题、关键词与“真正推动游戏往前走的人应逐步进入所有权和受益者讨论，但这不是当前已开放的法律股权、链上投票权事实或自动奖励承诺”的表达边界，并保持人类开发者第一人称、非上线宣称、非金融化口径与非空泛共同体口号。
- AC-35: 若根 README 的公开主定位从“文明模拟系统/引擎”收口为“文明模拟游戏”，必须同步更新 `doc/product/world-rules-core-gameplay/prd.md`、`site/index.html` 与 `site/en/index.html` 的首屏/元信息口径，并保持 `limited playable technical preview` 边界不变：不是 closed beta、public launch 或赛季上线。
- AC-36: 若小红书内容链路进入第十四篇，必须补齐“项目累计 token 用量”素材包，明确标题、正文、短版 caption、评论区引导、关键词、封面 HTML/PNG 与事实基线，并把讨论收口到“真实项目里最烧 token 的往往是需求反复、文档补齐、返工验证和上下文切换”，而不是滑向炫账单、融资暗示、模型排行榜吹捧或“AI 已经能全自动取代团队”的夸张宣称。
- AC-37: 若仓库需要一份“白皮书式项目总览”，必须新增 Explanation 类型文档，明确项目目标、世界模型、玩家/Agent 边界、玩法骨架、系统架构、当前双模式访问面（`viewer / pure_api`；`software_safe` 仅为 `viewer` 的兼容 alias，不增加模式或证据等级）与 `limited playable technical preview` 现状，同时显式声明该文档不替代 `README.md`、`doc/product/world-rules-core-gameplay/prd.md`、`doc/core/prd.md`、`doc/game/**` 与模块 PRD / project 真值。
- AC-38: 若白皮书式项目总览已经形成正式入口，必须同步更新 `site/index.html`、`site/en/index.html` 与 `site/doc/cn/index.html`、`site/doc/en/index.html`，让站点首页和文档中心至少能表达“两阶段路径”“当前仍是技术预览”的边界，并给出可直接进入该总览的公开入口。
- AC-26: 若 Moltbook 内容链路继续沿 `trust repair / shared truth / inspectable residue` 下钻，必须补齐下一条 `repair certification` follow-up，明确推荐标题、主贴、首评、CTA 与禁语边界，并保持 `general` / text-first / builder question 的已验证组织方式，不把讨论滑回泛道德论战或未宣布集成。
- AC-15: <a id="ac-15-xiaohongshu-game-intro"></a>若小红书进入“开始解释游戏是什么”的第三帖阶段，必须补齐独立素材包，明确标题、正文、轮播结构、互动问题与“不能写成完整设定说明书/不能暗示已上线”的边界。
- Non-Goals:
  - 不在 readme PRD 中替代各模块详细设计。
  - 不在 readme PRD 中定义测试用例细节。
  - 不在 readme PRD 中直接执行第三方平台广告采买或投放。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 文档链接检查、术语一致性校验、入口巡检脚本。
- Evaluation Strategy: 以链接可用率、口径冲突数、修复时长、评审返工率评估。

## 4. Technical Specifications
- Architecture Overview: readme 模块属于文档入口层，负责跨模块信息汇总、术语统一和导航稳定性。
- Integration Points:
  - `README.md`
  - `doc/product/world-rules-core-gameplay/prd.md`
  - `site/index.html`
  - `site/en/index.html`
  - `testing-manual.md`
- `doc/README.md`
  - `doc/readme/governance/readme-limited-preview-contributor-reward-pack.prd.md`
  - `doc/readme/governance/readme-limited-preview-contributor-reward-ledger.prd.md`
  - `site/social/xiaohongshu/wechat-promoter-oasis-coin-incentive/wechat-promoter-oasis-coin-incentive-pack-2026-04-12.prd.md`
  - `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md`
  - `scripts/readme-reward-pr-intake-import.py`
  - `scripts/readme-reward-pr-intake-round-scan.py`
- Edge Cases & Error Handling:
  - 链接失效：断链必须在巡检报告中暴露并进入修复队列。
  - 口径冲突：冲突出现时禁止发布“已同步”状态。
  - 空入口：模块入口缺失时标记高优告警并补齐跳转。
  - 权限不足：非维护者不得直接修改对外核心描述。
  - 并发编辑：同文件并发更新时需合并后重跑链接检查。
  - 历史重定向：legacy redirect 必须保留指向并声明主入口。
  - 过度承诺：若对外文案把贡献奖励写成 `play-to-earn`、`airdrop for players`、`just play and earn token`，必须阻断发布。
  - 刷量或搬运：若小红书笔记或微信公众号文章存在买量、互刷、抄袭搬运、截图造假或多账号重复申报，默认不得进入奖励审批。
  - 扫描窗口失控：若 merged PR 扫描没有时间窗或搜索边界，默认不得作为正式 round import 结果使用，避免把历史旧 PR 全量混入新一轮 ledger。
- Non-Functional Requirements:
  - NFR-RM-1: 顶层入口链接可用率 100%。
  - NFR-RM-2: 术语冲突修复 SLA <= 1 个工作日。
  - NFR-RM-3: README 与模块 PRD 关键引用一致率 100%。
  - NFR-RM-4: 发布前口径巡检覆盖率 100%。
  - NFR-RM-5: 对外文档不得暴露敏感配置信息。
  - NFR-RM-6: 早期贡献奖励模板不得公开固定 token 数额、固定 token/point 比率或“玩多久给多少”的承诺。
  - NFR-RM-7: 真实贡献奖励 ledger 中每条非拒绝记录必须至少有 1 个有效 source/evidence link，且每条已发放记录都必须具备 `Approval ID` 与 `Distribution Ref`。
- NFR-RM-8: 小红书博主 / 微信公众号绿洲币激励记录中每条非拒绝记录都必须同时具备资产链接、截图或归档证据、事实边界检查结果与反作弊结论；任何仅有阅读量或播放量而无质量和回流证据的记录不得进入审批。
- NFR-RM-9: merged PR reward round scan 必须输出逐条 `pr_number / merged_at / author / import_status / source_link` 与窗口级 `status_counts`，并保持 `ready/deferred/no_reward_review_requested/invalid_intake` 分类与单 PR 导入脚本一致。
- NFR-RM-10: 普通 merged PR 的 contributor reward 审批默认 ceiling 为 `150 OC`；任何 `>150 OC` 的待发放或已批准 row 都必须显式携带 exceptional case note，且 `1500 OC` 不得作为常规 MR 奖励预期保留在 active ledger 中。
- NFR-RM-11: planned grant 若在 actual-value review 后被下调，reward ledger 与 distribution closure 必须同时回写最终金额与下调理由，避免文档计划值和执行值分叉。
- Security & Privacy: 对外文档不得暴露敏感配置与密钥信息；示例配置需使用脱敏样例。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03-03): 固化 README 入口职责与同步流程。
  - v1.1: 增加自动化链接/术语巡检任务。
  - v2.0: 建立入口文档质量趋势指标（漂移率、修复时长）。
- Technical Risks:
  - 风险-1: 高频变更导致跨文档同步延迟。
  - 风险-2: 大范围重构时导航信息失真。

## 6. Validation & Decision Record
- Test Plan & Traceability:

旧 TASK-README 列仅定位各素材编制窗口，不是当前制作/发布队列。素材资产、长期渠道/审核与禁语合同保留；尚未核对唯一原 Task 的一次性操作仍按原版本保留，不全塞入 #1584。
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-README-001 | TASK-README-001/002/005 | `test_tier_required` | 入口结构与导航可达检查 | 新人入项与外部阅读体验 |
| PRD-README-002 | TASK-README-002/003/005 | `test_tier_required` | 架构口径一致性与链接巡检 | 技术评审效率与准确性 |
| PRD-README-003 | TASK-README-003/004/005 | `test_tier_required` | 同步流程与修复节奏抽样复核 | 文档长期稳定性 |
| PRD-README-004 | TASK-README-006 | `test_tier_required` | 对外口径简报、禁用表述与回滚口径抽样复核 | 版本候选外部沟通一致性 |
| PRD-README-005 | TASK-README-007 | `test_tier_required` | 对外口径模板、evidence-link 字段与审批链抽样复核 | 版本候选口径模板复用性 |
| PRD-README-006 | TASK-README-008 | `test_tier_required` | 公告 / changelog 底稿、draft 状态与 FAQ 结构抽样复核 | 对外发布底稿一致性 |
| PRD-README-007 | TASK-README-009 | `test_tier_required` | 公告模板、source links 与 review status 抽样复核 | 公告底稿模板复用性 |
| PRD-README-008 | TASK-README-010 | `test_tier_required` | 根 README 状态段含 `limited playable technical preview` / 非 closed beta / 非 public launch / 公告准备态 | 仓库首页状态理解 |
| PRD-README-009 | TASK-README-010 | `test_tier_required` | README 与 site / brief 口径一致 | 公开口径一致性 |
| PRD-README-010 | TASK-README-014 | `test_tier_required` | stable Moltbook runbook 含平台复验/fail-closed、内容支柱、禁宣称项与信号回流机制 | 第三方渠道推广口径一致性 |
| PRD-README-011 | TASK-README-015 | `test_tier_required` | stable Moltbook post pack 含主贴、回复模板、CTA、状态与 publication evidence 抽样复核 | 渠道文案执行安全性 |
| PRD-README-012 | TASK-README-016 | `test_tier_required` | Moltbook 方案与帖文明确 GitHub issue / PR CTA，且不与技术预览边界冲突 | 外部反馈回流与贡献转化 |
| PRD-README-013 | TASK-README-017 | `test_tier_required` | Moltbook 帖文主贴已压缩为更短的 feed-native 版本，且保留技术预览与 GitHub CTA 边界 | 最终发布文案可用性 |
| PRD-README-014 | TASK-README-018/019/020 | `test_tier_required` | 根 README、公开入口文档与 `doc/readme/governance/**` 的历史标题/当前公开口径统一为 `oasis7`，并显式区分内部兼容命名 | 仓库首页认知、公开品牌一致性 |
| PRD-README-015 | TASK-README-024 | `test_tier_required` | Moltbook runbook 明确发帖前、发帖后 24h、常规日与周复盘动作，并包含回复边界、升级路径与实际 PR 或已有 Task 的证据回写要求 | 第三方渠道持续运营一致性 |
| PRD-README-016 | TASK-README-025 | `test_tier_required` | Moltbook runbook 追加 day1-day7 首周模板，覆盖主帖节奏、检查窗口、回复目标与日志重点 | 第三方渠道冷启动执行性 |
| PRD-README-017 | TASK-README-026 | `test_tier_required` | Limited preview invite pack current signal / incident rows cover recruitment, feedback, claim drift, and incident guardrails | Limited preview recruiting/feedback/technical preview messaging |
| PRD-README-018 | TASK-README-027 | `test_tier_required` | Moltbook runbook 回写真实运营经验，明确哪些内容设计更易引发讨论、哪些自评动作更易触发 spam | 第三方渠道运营复用性与风控 |
| PRD-README-019 | TASK-README-029 | `test_tier_required` | invite-only limited preview execution pack 明确 callout copy、check slots、signal buckets、summary fields | 受控预览执行性与回流一致性 |
| PRD-README-020 | TASK-README-030/067/068/069/070/073 | `test_tier_required` | early contributor reward pack 明确 `Reward Account` 字段边界、评分模板、证据字段、奖励建议分级、普通 merged PR `<=150 OC` ceiling、PR intake import 路径、merged PR round scan 与禁语清单 | limited preview 贡献奖励执行性与对外口径安全性 |
| PRD-README-021 | TASK-README-031 | `test_tier_required` | 小红书首帖素材包明确标题、正文、封面文案、标签与“人类开发者视角”使用说明 | 新渠道冷启动识别度与口径稳定性 |
| PRD-README-022 | TASK-README-032/067/068/069/070/073/074 | `test_tier_required` | reward ledger 模板明确 round meta、`Reward Account`、逐条贡献记录、PR import status、merged PR round scan、producer 审批、ordinary merged PR `<=150 OC` ceiling、actual-value review、distribution ref 与归档字段 | limited preview 真实贡献奖励结算闭环 |
| PRD-README-023 | TASK-README-033 | `test_tier_required` | 小红书第二帖素材包明确 7 位 agent 队友卡文案、收束页与可截图 HTML | 渠道内容连续性与“agent 队伍”概念可理解性 |
| PRD-README-024 | TASK-README-034 | `test_tier_required` | 小红书 runbook 明确发帖前复核、发帖后 24h 巡检、评论分级、互动引导和实际 PR 或已有 Task 的证据回写要求 | 人类向渠道的持续运营一致性 |
| PRD-README-025 | TASK-README-035 | `test_tier_required` | 小红书第三帖素材包明确“游戏是什么”的标题、正文、轮播结构、互动问题与技术预览边界 | 渠道内容从“谁在做”过渡到“在做什么游戏”的可理解性 |
| PRD-README-026 | TASK-README-036 | `test_tier_required` | 小红书第四帖素材包明确“玩家为什么不能直接控制角色”的标题、正文、轮播结构、互动问题与控制边界口径 | 渠道内容从“游戏是什么”过渡到“玩家如何介入世界”的可理解性 |
| PRD-README-027 | TASK-README-037 | `test_tier_required` | 小红书第五帖素材包明确“AI时代，你变"懒"了么”的标题、正文、轮播结构、互动问题与游戏模式讨论收口 | 渠道内容从“玩家怎么介入”过渡到“AI时代下游戏应该把什么留给人”的可讨论性 |
| PRD-README-028 | TASK-README-038/042 | `test_tier_required` | 小红书第六篇素材包明确“作为游戏工作室主理人，今年的春招视角”的最终发布标题、正文、封面文案、互动问题与热点借势边界 | 渠道内容从游戏判断延展到团队判断与 AI 时代用人标准的可讨论性 |
| PRD-README-029 | TASK-README-039 | `test_tier_required` | 小红书第六篇轮播版素材包明确逐页文案、HTML、逐页 PNG 与评论区收束页 | 第六篇从长文版扩展到 feed-native 轮播版的可发布性 |
| PRD-README-030 | TASK-README-044 | `test_tier_required` | 小红书第七篇素材包明确“AI人格很火，但我不想做陪聊搭子”的标题、正文、互动问题、关键词与“人格 = 会在世界里行动”边界 | 渠道内容从泛 AI 人格讨论重新收束到 `oasis7` 的 agent 设计判断与世界内行动逻辑 |
| PRD-README-031 | TASK-README-046/047/048/049 | `test_tier_required` | 小红书第七篇轮播版素材包明确 4 页逐页文案、HTML、逐页 PNG 与评论区收束页，并完成前两页重叠修复、后两页案例页/收束页美术收口，以及整组从“编辑部海报感”继续收口到更接近小红书原生图文卡片的发布视觉 | 第七篇从单图封面 + 长文版扩展到更适合小红书 feed 的轮播版发布形态，同时保证整组轮播的视觉质量、手机端读感与平台原生感 |
| PRD-README-032 | TASK-README-050/051 | `test_tier_required` | 小红书第八篇素材包明确“做AI游戏以后，我越来越不信demo了”的标题、正文、短版文案、互动问题、关键词与封面 HTML/PNG，并保持“demo 高光不等于项目能扛住”的边界 | 渠道内容从“AI人格/陪聊”继续推进到“我怎么判断 AI 游戏里什么能信”，把讨论收口到真实项目判断与长期可玩性 |
| PRD-README-033 | TASK-README-052 | `test_tier_required` | 小红书第八篇轮播版素材包明确 4 页逐页文案、HTML、逐页 PNG 与评论区收束页，并沿用“项目审查板 / build review wall”视觉语言收口成更适合 feed 滑读的发布形态 | 第八篇从工业感单图封面 + 长文版扩展到更适合小红书停留和评论站队的轮播版，同时保持“先问它能撑多久”的判断主线 |
| PRD-README-034 | TASK-README-053 | `test_tier_required` | 小红书第九篇素材包明确“GUI已死？这次我是认同的”的标题、正文、互动问题、关键词与“GUI 退到二线 / 判断权高于操作权”的表达边界 | 渠道内容从“demo 祛魅”继续推进到“在 `oasis7` 这类游戏里为什么 GUI 不再是主交互层”，把讨论收口到玩家位置、判断权与系统执行分工 |
| PRD-README-035 | TASK-README-055/056/058 | `test_tier_required` | 小红书第十篇素材包明确“AI岗和大厂后端怎么选”的标题、正文、短版备选、互动问题、关键词与“对应届生先拿平台 / 训练体系 / 工程基本功 / 优秀同事密度，不是否定 AI 趋势，且传统团队可能很快转向 AI”的表达边界 | 渠道内容从春招焦虑继续推进到第一份工作该优先拿什么能力，把讨论收口到工程训练结构、平台价值、优秀同事密度与趋势判断的先后顺序 |
| PRD-README-036 | TASK-README-057/058 | `test_tier_required` | 小红书第十篇轮播版素材包明确 4 页逐页文案、HTML、逐页 PNG 与评论区收束页，并以“offer decision memo / 决策档案”视觉语言收口成更适合 feed 滑读的发布形态，同时补上“优秀同事密度”作为成熟团队价值的一部分 | 第十篇从长文版扩展到更适合小红书停留、收藏与评论站队的 4 页轮播版，同时保持“平台优先、不反 AI、趋势会快速扩散”的判断主线 |
| PRD-README-037 | TASK-README-059 | `test_tier_required` | Moltbook 热点帖素材包明确一个可直接发布的 trust-repair 标题、正文、首评与禁语边界，并完成真实发布与执行记录回写 | 渠道内容借当前 `trust / operator / accountability` 热点切入 `oasis7` 的 shared truth / repair cost 判断，同时保持技术预览口径稳定 |
| PRD-README-038 | TASK-README-060 | `test_tier_required` | 小红书第十一篇素材包明确“做游戏的人去开封，看见的是一套低门槛实景RPG”的标题、正文、封面文案、互动问题、关键词与 HTML/PNG 封面资产，并把判断收口到“最吸引人的不是看景，是进剧情 / 世界要能快速接住人 / 入戏门槛越低越容易形成代入” | 渠道内容从求职判断继续切回游戏设计观察，用现实里的真人 NPC 玩法解释为什么一个世界最重要的不是堆多少内容，而是能不能很快把人拉进剧情 |
| PRD-README-039 | TASK-README-061/062/063 | `test_tier_required` | Moltbook 下一条 follow-up 素材包明确 `repair certification` 题眼的推荐标题、主贴、首评、CTA 与禁语边界，并在后续收成单一可直接发布版与真实发帖执行记录，持续落回现有草案包与 PM 执行追踪 | 渠道内容顺着已验证的 `trust repair / shared truth / inspectable residue` 主线继续推进，把讨论从“修复该不该贵”收口到“修复到底该由谁验收、什么证据才算数”，同时保持技术预览口径稳定 |
| PRD-README-040 | TASK-README-064 | `test_tier_required` | 小红书第十二篇素材包明确“AI来得这么快，怎么穿越周期”的标题、正文、短版备选、互动问题、关键词与“先别盯岗位名，先看岗位里哪部分已经会被 AI 压缩 / 要把自己往问题定义、流程重构、结果负责这一层移动”的表达边界 | 渠道内容从现实体验与求职判断继续推进到 AI 时代已有岗位的人如何调整自己的不可替代性结构，把讨论收口到工作内容分层、判断权和结果责任，而不是泛化成宏观趋势分析 |
| PRD-README-041 | TASK-README-065 | `test_tier_required` | 第十二篇独立封面 HTML/PNG 素材要求，明确 `穿越周期` 题眼、`AI改写的不是岗位 / 是你那层工作` 封面钩子与 editorial 判断海报方向 | 第十二篇从文案包扩展到可直接发布的单图首屏资产，同时保持主题锚点、手机端读感与表达边界不漂移 |
| PRD-README-042 | TASK-README-066 | `test_tier_required` | 第十二篇独立轮播版素材要求，明确 4 页逐页文案、HTML、逐页 PNG 与评论区收束页，并沿用暖纸面 editorial 诊断卡 + 层级上移提示的发布视觉 | 第十二篇从长文版与单图封面进一步扩展到更适合小红书滑读、停留和评论的 4 页轮播版，同时保持“先别盯岗位名，先看岗位里哪层工作在变”的判断主线 |
| PRD-README-043 | TASK-README-067 | `test_tier_required` | 小红书博主 / 微信公众号绿洲币激励包明确两类对象的计分维度、固定 `300 / 800 / 1500 OC` 档位、证据字段、审批链、发放回填、反作弊与禁语边界，并把宣传方定义为生态参与者与受益者而不是按流量买量对象 | 两类重点宣传渠道激励的执行性、风控性与口径稳定性 |
| PRD-README-044 | TASK-README-070 | `test_tier_required` | merged PR reward round scan 脚本支持按时间窗批量扫描、离线 smoke 输入、状态汇总与 ledger-ready row 输出，并与单 PR intake contract 保持一致 | reward review 周期性归集与首轮台账准备效率 |
| PRD-README-045 | [Task #1584 历史来源](https://github.com/eng-cc/oasis7/issues/1584#issuecomment-6162412349) | `test_tier_required` | 第十三篇素材要求：表达“开发者、玩家和认真讲述项目的人一起参与把游戏做起来”，提供可读正文、封面与轮播资产，并保留非现行法律股权、链上投票权或自动奖励承诺边界 | 第十三篇从“你该被算进去”继续收口到更具体的关系图景，并扩展成可直接发布的单图首屏资产和滑读轮播资产，让“人人都是游戏builder / 一起把游戏做起来”的关系图景先于制度词被看见 |
| PRD-README-046 | game-positioning-alignment | `test_tier_required` | 根 README、世界规则入口与站点首页中英文首页统一把 `oasis7` 的公开主定位收口为“文明模拟游戏 / civilization simulation game”，同时保留 `limited playable technical preview` 且非 closed beta / public launch 的边界 | 仓库首读入口、规则入口与公开站点不再把项目分别写成系统、引擎和游戏 |
| PRD-README-047 | xiaohongshu-token-usage-post-pack | `test_tier_required` | 小红书第十四篇素材包明确“项目累计 token 用量”的推荐标题、正文、短版 caption、评论区引导、关键词、封面 HTML/PNG、事实基线与禁滑坡边界，并把这组真实账单收口为“AI 进入长期项目后会变成研发流程参与成本”的判断 | 小红书内容链路从关系和参与讨论继续推进到真实项目成本结构，让读者看到 AI 成本主要消耗在长期迭代而不是单次生成炫技 |
| PRD-README-048 | whitepaper-style-overview | `test_tier_required` | 新增一份白皮书式项目总览，明确项目目标、世界模型、玩家/Agent 边界、玩法骨架、系统架构、当前访问面与技术预览边界，并把它接入根 README 与 readme/governance 导航 | 第一次接触仓库的读者可以先用一份解释型总览建立全貌，再按正式真值源继续下钻 |
| PRD-README-049 | site-whitepaper-entry-alignment | `test_tier_required` | 站点首页与文档中心同步采用白皮书式总览的公开 framing，补齐“两阶段路径”与“技术预览”边界表达，并新增白皮书总览直达入口 | 第一次进入站点的读者不必先翻仓库目录，也能在公开 landing surface 上看见项目总览、阶段路径与继续下钻的入口 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-RM-001 | README 作为对外入口主控层 | 各模块对外独立叙述 | 容易产生口径漂移。 |
| DEC-RM-002 | 入口链接定期自动巡检 | 仅人工抽查 | 自动巡检可降低断链遗漏率。 |
| DEC-RM-003 | 口径更新设置同步时限 | 无明确时限 | 时限可提高协作可预测性。 |
| DEC-RM-004 | 候选级对外说明先走简报专题，再决定是否升级到 README / 公告 | 直接在 README 或外部公告里写最终口径 | 先用简报控边界，能避免对外承诺过早固化。 |
| DEC-RM-005 | 用模板沉淀 release communication 结构 | 每次按临时文案自由发挥 | 模板能提高后续候选的复用率与审阅效率。 |
| DEC-RM-006 | 在简报之后先落 announcement/changelog draft，再决定是否正式发布 | 直接将简报对外公开 | 底稿更接近外部文风，同时仍保留 draft 审核缓冲层。 |
| DEC-RM-007 | 将 announcement/changelog 底稿继续模板化 | 每个候选手写公告底稿 | 模板化能降低重写成本，并稳定审核结构。 |
| DEC-RM-008 | 根 README 只对齐状态段，不重写整份首页 | 为修正口径重做全部 README 文案 | 最小变更即可消除仓库首页与 site 的状态分叉。 |
| DEC-RM-009 | 用平台专题文档承接第三方渠道推广方案 | 直接复用一份通用社媒文案覆盖全部平台 | 第三方平台机制和社区文化差异明显，必须保留渠道化约束。 |
| DEC-RM-010 | 在渠道方案之后继续沉淀首批发帖草案与回复模板 | 真实发帖时临场手写文案 | 提前冻结主贴与回复模板，能显著降低 overclaim 风险。 |
| DEC-RM-011 | 将 GitHub issue / PR 作为 Moltbook builder CTA 的正式回流出口 | 只引导关注或私信，不给公开协作入口 | 仓库已有公开协作面，显式回流更利于把外部兴趣转成可追踪反馈。 |
| DEC-RM-012 | 在发布前把首批帖文继续压缩成 feed-native 短版 | 保留偏内部草案长度直接外发 | Moltbook 更适合短、硬、单一 CTA 的原生内容。 |
| DEC-RM-013 | 对外品牌统一为 `oasis7`，内部 crate/bin 暂保留兼容命名 | 同一轮同时重命名全部 crate/bin/script/env 标识 | 先统一用户可见品牌与下载入口，避免把品牌收口与大规模兼容迁移耦合。 |
| DEC-RM-014 | 将 Moltbook 日常运营动作沉淀为独立 runbook，而不是继续扩写角色卡 | 把巡检、回复、升级细节继续堆进角色卡或推广方案 | 角色卡应保持稳定职责边界；执行细节更适合 runbook 持续演进。 |
| DEC-RM-015 | 在已有 Moltbook runbook 内补首周运营模板，而不是再拆一份平行文档 | 单独再建一份“week-one playbook” | 首周模板属于 runbook 的执行层细化，放在同一文档更利于维护与回查。 |
| DEC-RM-016 | 将 `PRD-README-017` 收敛为 limited preview invite pack | 继续保留旧 closed-beta 操作面作为当前入口 | 当前公开状态不是 closed beta；invite pack 继续强制维持 `technical preview` 口径，避免提前升级 release claim。 |
| DEC-RM-017 | 将 Moltbook 实战复盘写回现有 runbook | 只在 devlog 留一次性记录 | runbook 才是后续运营会重复翻看的入口。 |
| DEC-RM-018 | 为 invite-only limited preview 新增独立执行包 | 继续只靠 handoff 和零散 devlog 驱动首轮外放 | 首轮 limited preview 需要固定文案、巡检窗口、分桶与摘要字段，才能避免执行漂移。 |
| DEC-RM-019 | 早期奖励模板只输出贡献评分与奖励建议档位，不公开固定 token 数额或 point 汇率 | 直接把 token 发放表做成外部宣传文案 | 当前阶段仍是 `limited playable technical preview`，需要避免过度金融化与过度承诺。 |
| DEC-RM-020 | 小红书首帖采用“人类开发者自我介绍”而非“项目功能介绍” | 首帖直接解释完整世界观或产品能力 | 小红书面向人，首帖更需要建立创作者身份与协作关系，而不是先堆产品信息。 |
| DEC-RM-021 | 第二篇采用“团队 roster 轮播卡”而非长文解释 | 继续用长文解释 agent 协作或直接贴角色职责原文 | 用户第二篇更需要快速理解“有哪些队友、为什么重要”，轮播卡比长文更适合这一层信息。 |
| DEC-RM-022 | 用 round-based reward ledger 承接真实贡献奖励结算 | 继续靠 issue 评论、聊天记录或零散表格临场结算 | 真实发放前必须把评分、审批、执行引用和归档收进统一模板，才能保持可审计性。 |
| DEC-RM-039 | <a id="dec-rm-039-reward-account"></a>reward intake 与台账执行层统一只要求 `Reward Account` | 在领取模板里继续要求额外 claimant 字段 | 这条链路要先保证可执行收款，review 名称层已有 contributor/public handle，可不再额外要求 PR 作者维护独立 claimant 字段。 |
| DEC-RM-040 | GitHub PR 作为贡献证据入口时，使用可选 reward intake block 收集 `Reward Account` | 继续让 PR 作者在评论或私聊里零散补账户字段，或在 PR 模板里直接索要 raw `public_key` | PR 已是公开贡献入口；把 payout 字段收进同一模板更利于 liveops/producer 复核，同时继续把底层 `public_key` 留在技术专题。 |
| DEC-RM-041 | 为 GitHub PR reward intake 提供脚本化导入入口 | 继续让 liveops 手工抄 PR body，或在每轮 ledger 建档时重复肉眼判断 `ready/deferred/no_reward_review_requested` | PR intake 一旦进入重复使用阶段，就应收成可执行脚本；否则 contract 再清晰，也会退化回手工表格流程。 |
| DEC-RM-042 | merged PR 的周期性奖励归集采用“时间窗扫描 + 复用现有 intake contract”的批量脚本，而不是把单 PR 导入逻辑复制成第二套规则 | 每轮靠 liveops 手工打开 merged PR 重复判断；或为 round scan 单独发明另一套字段/状态 contract | 模板既然已经固定在 PR body，就应把周期性归集也收口到同一 contract 上；否则一旦单 PR 和 batch scan 判定不同，ledger 会再次退回人工表格。 |
| DEC-RM-043 | 普通 merged PR 的 contributor reward 默认真实发放 ceiling 收紧到 `150 OC`，`1500 OC` 只保留给极少数 exceptional case | 继续让单个 merged PR 在没有 exceptional note 时也能维持 `1500 OC` 待发放 | 当前总量口径改为 `10B` 后，普通 MR 的激励仍应保守；把 ceiling 收紧到 `150 OC` 能更稳地管理预期，避免 contributor reward 被误解成高额 bounty。 |
| DEC-RM-044 | planned grant 在执行前仍要接受 actual-value review，若原计划金额高于实际增量价值则应先下调 | 文档一旦写出 planned grant，就默认锁定原金额直发 | 奖励治理的目标不是保护最早写下的计划值，而是让最终发放和实际价值匹配，并留下可审计的纠偏痕迹。 |
| DEC-RM-045 | 第十三篇采用“谁该拥有游戏的所有权 / 谁该成为受益者”切口，把开发者、玩家和高质量创作者写成应逐步进入价值分配讨论的人，而不是直接宣称“所有相关人员已经拥有链上投票权” | 直接把帖子写成当前治理权公告；或只写“大家都是一家人”的泛共同体口号 | 这个主题真正有张力的地方是“谁不该被排除在所有权和受益之外”，不是“现在已经发放了什么权利”。把它收口到长期参与、真实贡献和共同体关系，既能延续账号的制度判断主线，也能避开当前 preview 阶段尚未完成的治理实现和过度承诺风险。 |
| DEC-RM-046 | 对外首读入口统一把 `oasis7` 表述为“文明模拟游戏”，内部实现文档仍可在对应上下文使用系统/引擎/runtime 术语 | 全仓机械替换所有“系统/引擎”措辞 | 公开定位需要单一主语，但实现层术语仍服务不同技术边界；只收口入口层可以消除对外分叉，又不误伤内部设计语义。 |
| DEC-RM-047 | 第十四篇采用“真实累计账单 + 流程成本解释”切口，而不是只晒一张高消耗截图或顺手做模型战力榜 | 直接把帖子写成“我们烧了很多钱”的炫账单内容；或把焦点放到哪个模型更强更贵 | 这条内容真正有价值的不是数字本身，而是它能把“AI 成本到底烧在哪”讲清楚。把重点收口到需求反复、文档、验证、返工和上下文切换，才更像真实项目经验，也更符合账号一贯的项目判断视角。 |
| DEC-RM-048 | 新增一份“白皮书式说明”作为 Explanation 类型项目总览，并显式把它降级为导览层而非真值层 | 继续把这类解释全部挤在根 README；或新建一份自称权威规格的白皮书与 PRD 并行维护 | 仓库已经有 README、core PRD、world-rule、gameplay PRD 等多层真值源，缺的是第一次接触时的一份单页解释，而不是另一份竞争真值。把文档定位成 explanation，可以补齐 overview gap，又不破坏现有文档治理结构。 |
| DEC-RM-049 | 站点首页和文档中心显式暴露白皮书总览入口，并把首页路线说明收口到“第一阶段开放世界沙盒游戏，后续演进为开放世界游戏平台” | 继续把白皮书只藏在仓库 README / governance 索引里；或在站点首页继续只讲局部玩法而不解释阶段路径 | 白皮书已经补上“第一次接触项目时的单页全貌”，如果站点仍不给入口，公开读者还是会停在首页碎片文案里。把总览入口和两阶段路线放到站点，可让对外 landing surface 与现有 explanation 文档形成前后衔接，同时不越过技术预览边界。 |
| DEC-RM-023 | 小红书持续运营细节独立沉淀为 runbook，而不是继续只留在素材包和角色卡 | 只补几条角色卡示例或继续靠帖子素材包驱动 | 小红书已经进入持续发帖和看反馈阶段，需要和 Moltbook 一样有稳定 SOP，才能复盘互动和误解模式。 |
| DEC-RM-024 | 第三篇采用“轻量游戏介绍 + 猜类型互动”而非完整设定说明书 | 直接把世界规则、技术架构与完整玩法一次讲清 | 用户到了第三篇需要先建立“这是什么游戏”的可想象轮廓，而不是被文档级信息密度劝退；同时仍要保持技术预览边界。 |
| DEC-RM-025 | 第四篇采用“玩家控制边界解释 + 站队式互动”而非继续泛讲世界设定 | 跳过玩家位置直接讲更多系统细节，或把第四篇写成输入操作说明 | 第三篇之后最自然的追问是“玩家到底怎么介入这个世界”；先把控制边界讲清，才能避免用户把项目误解成直接操控单角色的传统玩法。 |
| DEC-RM-026 | 第五篇采用“AI 使用感受 -> 游戏模式判断”而非直接泛聊 AI 让人变懒 | 只发一条泛 AI 焦虑/效率感想，或直接转去讲 `oasis7` 具体机制 | “AI时代，你变"懒"了么”本身容易引讨论，但如果不收口会太泛；把它拉回“游戏还该把什么留给人”更贴账号主线，也更能承接项目设计判断。 |
| DEC-RM-027 | 第六篇采用“春招判断标准”而非“岗位清单/求职资料” | 直接做免笔试汇总、岗位投递清单，或输出泛求职鸡汤 | 春招话题有热度，但账号主线仍是人类开发者视角下的团队判断与项目收口；借热点讲“我会看什么人”更贴现有内容链，也更容易带出高质量讨论。 |
| DEC-RM-028 | 第六篇轮播版采用“6 页短判断卡”而非把长文硬切成多页 | 直接把长文等分成 4-8 页纯文字截图 | 小红书轮播更吃“每页只讲一个判断”的手机端节奏；重新拆页比机械分段更利于读完率、收藏和评论承接。 |
| DEC-RM-029 | 第七篇采用“AI人格热词 + 不做陪聊搭子”的立场切入，而不是顺着热点泛聊情感陪伴或做 AI 产品点评 | 直接做“AI人格是什么”的抽象讨论，或把帖子写成陪伴型 AI 产品比较/安利 | `AI人格` 的平台热度足够高，但账号主线仍是游戏与 agent 设计判断；先用强立场把注意力拉住，再迅速收回“放进世界后会不会行动与承担后果”，更符合 `oasis7` 现有内容链。 |
| DEC-RM-030 | 第七篇轮播版采用“4 页分割判断卡”而非继续只发单图封面或把长文整段截图切成多页 | 维持仅有封面图+长文；或直接把长文逐段截成 4-6 页纯文字卡 | 这一篇最强的传播点在“会聊天 vs 会行动”的对照。把它压成 4 页，每页只停在一个判断，更利于 feed 内读完率和评论区站队。 |
| DEC-RM-031 | 第九篇采用“GUI 作为主交互层退场”而不是泛喊“屏幕消失 / UI 行业已死” | 顺着热点做行业预言、岗位焦虑或全自动论战 | 小红书站内 `GUI已死` 讨论已经很挤，真正适合 `oasis7` 的切口不是预测所有软件怎么变，而是讲这款游戏为什么把玩家从操作员抬成判断者。 |
| DEC-RM-032 | 第十篇采用“应届生第一份 offer 的优先级排序”而不是泛做 `AI岗 vs 非AI岗` 输赢帖 | 直接写成“千万别去 AI”或“必须去风口”的绝对站队；或做泛职业咨询帖 | `offer 焦虑` 的站内热度足够高，但账号主线仍应保持为做项目的人对训练结构、工程成长和趋势判断的真实经验；把重点放在“先拿能把人带起来的系统”，比单纯站队岗位名更稳，也更符合用户真实困惑。 |
| DEC-RM-033 | 第十篇轮播版采用“4 页 decision memo”而不是把长文硬切成 4 张纯文字截图 | 只发长文版；或把正文机械分成 4-6 页文字卡；或做泛校园招聘海报 | 这一篇真正有传播力的不是岗位名本身，而是“平台 / 训练体系 / 成长结构优先”的判断顺序。做成 4 页决策档案，更适合小红书 feed 停留、收藏和评论区站队。 |
| DEC-RM-034 | 第十一篇采用“现实观察 + 设计判断”的开封实景 RPG 切口，而不是把帖子写成景区推荐或沉浸式文旅攻略 | 直接做“开封好玩吗 / 值不值得去”的旅游内容；或泛喊“沉浸式就是未来” | 这一篇真正有传播力的不是开封地名本身，而是“最吸引人的不是看景，是被快速拉进剧情”。把现实观察拉回游戏设计判断，才符合账号主线，也更利于把讨论沉到“入戏门槛”和“世界接住人”的设计问题上。 |
| DEC-RM-035 | Moltbook 下一条 follow-up 采用“repair certification / who verifies repair”而不是重发泛 trust 帖或重新讲一遍 repair cost | 再发一条抽象 trust / accountability 感想；或回退到 identity / product explainer 帖 | 前两轮 Moltbook 执行已经验证 `trust repair / shared truth / inspectable residue / proof boundaries` 的 builder 讨论路径更有效。把问题推进到“谁有资格确认修复真的发生了”，比重复讲“修复要不要贵”更像自然 follow-up，也更贴平台当前的 `operator / trust / accountability` 语境。 |
| DEC-RM-036 | 第十二篇采用“岗位内容分层 + 穿越周期动作”的切口，而不是把帖子写成宏观 AI 趋势判断或泛职业焦虑安慰 | 直接做“哪些岗位会被取代”的宏大预言；或写成“别焦虑、持续学习就好”的鸡汤帖 | 这篇真正有传播力的不是再重复“AI 来得很快”这句共识，而是把焦虑落到更具体的问题上: 岗位里哪些部分先被压缩、什么能力层更值得往上挪。把讨论收回到问题定义、流程重构和结果负责，更符合账号持续输出“判断”而不是“喊口号”的主线。 |
| DEC-RM-037 | 第十二篇封面采用“editorial 判断海报 + 层级抬升构图 + 穿越周期题签”方向，而不是直接把主标题做成泛 AI 焦虑口号或科幻灾难画面 | 直接做机器人 / 电路 / 岗位被吞掉的直白 AI 隐喻；或只保留“穿越周期”四个字做抽象励志封面 | 这篇真正需要放大的不是恐慌，而是判断: 哪层工作先被压缩，人要往哪层上挪。封面既要保留 `穿越周期` 这个主题锚点，也要把“岗位 vs 那层工作”的分层判断一眼说清，才和正文主线一致。 |
| DEC-RM-038 | 第十二篇轮播版采用“warm editorial diagnosis / work-layer ladder”方向，而不是把 4 张图做成重复封面、蓝紫科技卡或信息过满的长文截图 | 4 张都重复封面主钩子；或做成密密麻麻的文字截图；或回退到机器人吞岗位的 panic 视觉 | 轮播版的价值不在于再重复一次封面，而在于把判断拆成 4 个停顿点: 先停住、再诊断、再给动作、最后把问题抛回给读者。用暖纸面和诊断卡结构，更符合第十二篇“判断比情绪更重要”的主线，也更接近小红书原生滑读体验。 |
| DEC-RM-039 | <a id="dec-rm-039-channel-incentive"></a>当前先把宣传激励收窄到“小红书博主 + 微信公众号”，并采用“内容深度 + 事实准确 + 讨论转化 + 生态回流”的绿洲币审核模型，固定映射 `300 / 800 / 1500 OC` 档位，而不是泛化到所有媒体对象或按流量做 flat buyout | 一次覆盖所有媒体/KOL/搬运号；或直接按 CPM/播放量发币 | 用户已明确当前版本“先精简和具体化”，并进一步要求直接写明不同价值内容获得多少绿洲币。先收口到两类最清晰的对象，再给固定档位金额，更利于执行、举证和对外解释。 |
