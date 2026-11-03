# readme PRD 文件级索引

审计轮次: 14

更新时间：2026-11-03

## 入口
- 模块 PRD：`doc/readme/prd.md`
- 模块设计总览：`doc/readme/design.md`
- 模块执行记录：Git、PR、实际 CI 与评审记录；Issue/Project 按需，PRD 不承载可变任务状态。
- 当前热点子域入口：`doc/readme/governance/README.md`
- README gap 收口资料入口：`doc/readme/gap/README.md`
- production 证据与素材入口：`doc/readme/production/README.md`

## 首读分流
- 想先回答 README 模块在管什么、哪些内容属于正式对外口径：先读 `doc/readme/prd.md`
- 想先回答当前在推进什么：读取实际 PR 或已有 Task；想确认长期治理和运营边界：先读 `doc/readme/prd.md`
- 想先进入 `governance` 热点子域，并按治理控制 / release communication 模板 / Moltbook / limited preview 贡献奖励 / 小红书 / 公开定位分流：先读 `doc/readme/governance/README.md`
- 想先区分 README gap 的总收口、具体增量与历史追溯：先读 `doc/readme/gap/README.md`
- 想追溯 production 收口证据或判断 dated production 素材是否仍可删除：先读 `doc/readme/production/README.md`
- 想继续按子域或文件名下钻：使用下方热点子域导航，再跳到对应清单区域

## 密度与库存

- 不在索引中冻结容易漂移的文件数与专题组数；当前库存统一以 `./scripts/doc-inventory-report.sh` 为准。
- release communication 与受控测试支持的产品承诺进入 `doc/product/player-entry-distribution/`；readme/governance 承接操作模板、专业支持设计和执行手册，不另立产品承诺。

## 热点子域导航
| 子域 | 文件数 | 适合回答的问题 |
| --- | --- | --- |
| `governance/` | 动态 | release communication 操作模板、玩家支持与服务生命周期、Moltbook/Xiaohongshu runbook、贡献奖励治理、material/execution 入口；产品承诺从 `doc/product/` 进入 |
| `gap/` | 动态 | README 与实现/流程之间还有哪些正式缺口、哪些差距仍待收口；先从 `gap/README.md` 区分主收口、增量与历史追溯 |
| `production/` | 16 | 生产收口、阶段边界、readiness 与受台账约束的素材；先由 `production/README.md` 分流 |

## 活跃补充文档

- [玩家支持与服务生命周期](governance/player-support-and-service-lifecycle.design.md)与[执行手册](governance/player-support-and-service-lifecycle.manual.md)：承接受控测试的反馈入口结果、最小信息、分流复核、公开追加账本边界，以及事故、暂停和结束告知；提供具体文案与桌面演练，实际支持和恢复证据留在原任务。

### 未结履约义务

- `doc/readme/governance/readme-limited-preview-contributor-reward-ledger-round-2026-04-13.md` 与 `readme-limited-preview-contributor-reward-distribution-closure-round-2026-04-13.md`：两笔已批准的 100 OC + 50 OC 仍为 `pending_distribution`，Execution Owner / Distribution Ref / Distribution Date 未填写。它们不是当前奖励活动或公开发布承诺，但在授权执行者完成发放或明确的治理 supersession、写入不可变引用并关闭源 ledger 前，不得归档、删除或报告为已结清。
- `doc/readme/governance/README.md`：`governance/` 热点子域 landing page，按治理控制、release communication 模板、Moltbook、limited preview 贡献奖励、小红书与公开定位分流读者。
- `doc/readme/gap/README.md`：`gap/` 子域 landing page，先定位跨 Gap 1–5 的主收口，再按具体增量或历史追溯下钻；不复制 leaf 规格或项目证据。
- `doc/readme/production/README.md`：production 子域 landing page，按当前可检索专题、历史压缩三件套及仍受项目台账约束的素材分流；不会把历史收口证据误作当前执行入口。
- `doc/readme/governance/readme-project-overview-whitepaper-2026-04-25.md`：面向第一次接触仓库读者的白皮书式项目总览，适合先理解“项目是什么、为什么存在、当前做到哪一步”，再下钻正式真值源。
- `doc/readme/governance/readme-moltbook-liveops-runbook.md`：Moltbook 持续运营 canonical runbook；产品 claim 由发行沟通产品分册拥有。
- `doc/readme/governance/readme-moltbook-post-pack.md`：可复用帖文与回复素材库；每次发布前必须重新绑定当前 claim 与 publication evidence。
- `doc/readme/governance/readme-xiaohongshu-liveops-runbook-2026-03-23.md`：小红书持续运营 SOP，不并入下方模块 PRD 三件套长表。
- `site/social/xiaohongshu/README.md`：小红书内容包目录规范，适合查找每篇内容的文案、视觉源文件、导出图与 package manifest。
- `site/social/xiaohongshu/token-usage/token-usage-post-pack-2026-04-20.md`：小红书第十四篇素材包入口，已收口为“项目累计 token 用量不是炫账单，而是研发流程参与成本”版本，适合直接判断标题、正文、短版 caption、评论区引导、事实基线与禁滑坡边界。
- `site/social/xiaohongshu/future-ownership/future-ownership-post-pack-2026-04-13.md`：小红书第十三篇素材包入口，现已收口为“开发者、玩家和认真把它讲出去的人一起参与把游戏做起来”的版本，并补齐共同参与主题封面与 4 页轮播入口，适合直接判断正文、评论区问题、封面/轮播资产与禁滑坡边界。
- `site/social/xiaohongshu/wechat-promoter-oasis-coin-incentive/wechat-promoter-oasis-coin-incentive-pack-2026-04-12.md`：小红书博主 / 微信公众号绿洲币激励入口，适合判断两类宣传对象的奖励边界、证据字段与禁语。
- 根 `README.md`：当前公开状态真值；配合 `doc/readme/governance/readme-project-overview-whitepaper-2026-04-25.md`、一致性 checklist、季度复核与 release communication surfaces 判断最新对外口径。

## 默认阅读面边界
- 本页首屏只负责分流，不再要求读者从第一行开始顺扫完整长表。
- README 不再平铺“当前推荐入口”或“近期专题”长名单；完整清单继续保留在下方，用于精确文件名检索和互链可达性。
- 已完成且只承担历史证据职责的治理微专题进入“历史压缩专题清单”，不再计入默认活跃阅读面。
- runbook、material 与 execution_log 仍保留可检索性，但默认不与模块 PRD 三件套一起暴露在首屏。

## 索引分层
- `canonical`：维护现行需求和设计的正式文档；Git、PR、实际 CI 与评审记录承载交付事实，Issue/Project 按需。历史专题不因成对文件仍在就自动成为当前权威。
- `runbook`：已正式建档的运营 SOP，回答“运营同学今天应该怎么执行”。
- `material`：帖子草案、邀请包、奖励包、轮播包等投放素材，回答“今天要发什么/给什么”。
- `execution_log`：某轮真实执行记录，仅用于复盘与追溯。

## 覆盖规则
- 纳入规则：纳入 `doc/readme/**` 下所有 `*.prd.md` 与相关 PR 和实际验证记录。
- 活跃补充：仍被当前模块 PRD / 项目态直接引用的 `runbook`、`material`、`execution_log` supporting doc，可在“活跃补充文档”区定向列出，但不并入下方三件套长表。
- 历史压缩：已完成、无下一步、且当前治理入口已由脚本/现行专业 authority/后续复核专题覆盖的专题，保留文件原址和互链，但从默认活跃清单降级到历史压缩清单。
- 排除规则：不纳入 `doc/devlog/**` 与其他非 PRD 配对文档。
- 按需进入：素材包、执行记录、帖子草案、审计 checklist 与历史收口材料继续保留可检索性；除非它们重新成为当前 operator 或 owner 的直接入口，否则不进入默认首屏。

## 当前默认活跃专题清单（按文件名精确检索）
| 专题 PRD | 专题设计文档 | 专题项目文档 |
| --- | --- | --- |
| `doc/readme/governance/readme-quarterly-review-cycle-2026-03-11.prd.md` | `doc/readme/governance/readme-quarterly-review-template-2026-03-11.md` | `doc/readme/governance/readme-remediation-log-template-2026-03-11.md` |
| `site/social/xiaohongshu/wechat-promoter-oasis-coin-incentive/wechat-promoter-oasis-coin-incentive-pack-2026-04-12.prd.md` | `site/social/xiaohongshu/wechat-promoter-oasis-coin-incentive/wechat-promoter-oasis-coin-incentive-pack-2026-04-12.design.md` | PR、实际 CI 与评审记录（Issue 按需） |

## 当前活跃 PRD-only 治理专题

| 专题 PRD | 关联执行入口 |
| --- | --- |
| `doc/readme/governance/readme-limited-preview-contributor-reward-pack.prd.md` | `doc/readme/governance/readme-limited-preview-contributor-reward-pack.md` |
| `doc/readme/governance/readme-limited-preview-contributor-reward-ledger.prd.md` | `doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md` |

## 历史压缩专题清单（保留原址与互链）
| 历史 PRD | 设计承接 | 执行来源 | 保留理由 |
| --- | --- | --- | --- |
| `doc/readme/gap/readme-gap-distributed-prod-hardening-gap12345.prd.md` | [同文历史边界与现行去向](gap/readme-gap-distributed-prod-hardening-gap12345.prd.md#historical-retention-and-routing)；旧 design 仅兼容 | 原 Git / PR / CI / 评审 | 保留多样本、重试、审计与原失败；不是当前操作入口 |
| `doc/readme/production/readme-prod-gap1245-wasm-repl-topology-player.prd.md` | [同文历史边界与现行去向](production/readme-prod-gap1245-wasm-repl-topology-player.prd.md#historical-retention-and-routing)；旧 design 仅兼容 | 原 Git / PR / CI / 评审 | 保留 WASM、复制与玩家身份原合同；旧 Viewer 节点参数不再适用 |

## 已退役删除专题
| 专题 | 当前承接 | 删除理由 |
| --- | --- | --- |
| README production P0/P1、LLM P1/P2、LLM/DistFS/consensus 三组 closure triplet | `doc/product/` 四模块产品树；`doc/world-simulator/llm/`、`doc/world-runtime/`、`doc/p2p/`、`doc/world-simulator/viewer/` 专业权威；Git history / PR 与实际验证记录 | 三组文件只包装已完成的实现任务和接口细节，不再拥有产品或专业语义；节点、拓扑与共识门控仍归 P2P/runtime 而非 viewer |
| README consistency checklist dated source set | 根 `README.md`；`scripts/readme-link-check.sh`；`doc/readme/governance/readme-quarterly-review-cycle-2026-03-11.prd.md`；`doc/readme/prd.md` | 一次性人工 checklist 已被可执行链接检查、季度复核和模块台账吸收；不再保留重复 dated 三件套与 supporting copy |
| 2026-03-11 release communication / announcement 四组 dated triplet | `doc/product/player-entry-distribution/release-communications-and-public-claims.prd.md`；`doc/readme/governance/readme-release-communication-template.md`；`doc/readme/governance/readme-release-announcement-template.md` | 长期产品合同已进入产品树，操作模板去日期稳定化；历史 candidate brief / draft、模板设计过程和任务状态只从 Git history 与 GitHub evidence 追溯 |
| 2026-03 Moltbook promotion/post/runbook 三组 dated triplet 与 promotion plan | `doc/product/player-entry-distribution/release-communications-and-public-claims.prd.md`；`doc/readme/governance/readme-moltbook-liveops-runbook.md`；`doc/readme/governance/readme-moltbook-post-pack.md` | 渠道产品边界进入产品树，稳定策略合并到无日期 runbook，素材库去日期；历史平台快照、固定排期和任务包装只从 Git history 与 GitHub evidence 追溯 |
| 2026-03-22 closed-beta candidate runbook triplet 与 Round 1 execution log | `doc/readme/governance/readme-limited-preview-invite-pack-2026-03-22.md`；Round 1 发布、渠道 fallback、信号和关闭证据从 Git history 与 GitHub issue `eng-cc/oasis7#48` 追溯 | closed-beta-candidate 不是当前公开状态；旧 runbook 仍暴露的可复用沟通语义收敛到 limited preview invite pack。Round 1 已于 2026-07-28 关闭且无待处理 follow-up，不再保留失效监控指令作为活跃 execution 入口 |
| 2026-03-22 closed-beta candidate feedback / incident templates | `doc/readme/governance/readme-limited-preview-invite-pack-2026-03-22.md` 的 signal bucket、claim drift、producer summary 字段 | 旧模板命名继续暗示 closed beta candidate 当前可操作；保留历史 evidence，不保留当前 template surface |
| README gap runtime/WASM/consensus/market/LLM 七组完成 triplet | `doc/world-runtime/prd.md`、`doc/world-runtime/module/`、`doc/world-runtime/wasm/`、`doc/p2p/consensus/`、`doc/world-simulator/llm/`、`doc/game/gameplay/gameplay-top-level-design.prd.md` | 仍有效合同已按专业 owner 合并；生产 source compile 旧口径被 binary + receipt 当前安全边界替代，历史任务包装从 Git history 与 GitHub evidence 追溯 |

## Material / Execution / SOP 补充入口
| 文档路径 | 类型 | 用途 |
| --- | --- | --- |
| `doc/readme/governance/readme-project-overview-whitepaper-2026-04-25.md` | `overview` | 面向第一次接触项目读者的白皮书式 Explanation 总览 |
| `doc/readme/governance/readme-limited-preview-invite-pack-2026-03-22.md` | `material` | invite-only limited preview 招募与沟通包 |
| `site/social/xiaohongshu/token-usage/token-usage-post-pack-2026-04-20.md` | `material` | 小红书真实累计 token 用量主题帖素材包，已收口为“AI 进入长期项目后会变成研发流程参与成本”的推荐发布版 |
| `site/social/xiaohongshu/future-ownership/future-ownership-post-pack-2026-04-13.md` | `material` | 小红书“共同参与把游戏做起来”主题帖素材包，已收口为开发者、玩家和传播者共同参与的推荐发布版，并补齐封面与 4 页轮播 HTML/PNG 资产 |
| `site/social/xiaohongshu/wechat-promoter-oasis-coin-incentive/wechat-promoter-oasis-coin-incentive-pack-2026-04-12.md` | `material` | 小红书博主 / 微信公众号绿洲币激励操作包 |
| `doc/readme/governance/readme-xiaohongshu-liveops-runbook-2026-03-23.md` | `runbook` | 小红书持续运营 SOP |
| `site/social/xiaohongshu/cycle/cycle-carousel-pack-2026-04-09.md` | `material` | 小红书 AI 时代岗位穿越周期主题轮播图素材包 |
| `site/social/xiaohongshu/cycle/cycle-post-pack-2026-04-08.md` | `material` | 小红书 AI 时代岗位穿越周期主题帖素材包 |
| `site/social/xiaohongshu/intro/intro-post-pack-2026-03-22.md` | `material` | 小红书首篇自我介绍帖素材包 |
| `site/social/xiaohongshu/team-roster/team-roster-post-pack-2026-03-22.md` | `material` | 小红书团队阵容帖素材包 |
| `site/social/xiaohongshu/game-intro/game-intro-post-pack-2026-03-24.md` | `material` | 小红书游戏介绍帖素材包 |
| `site/social/xiaohongshu/player-boundary/player-boundary-post-pack-2026-03-25.md` | `material` | 小红书玩家边界帖素材包 |
| `site/social/xiaohongshu/ai-laziness-game-mode/ai-laziness-game-mode-post-pack-2026-03-26.md` | `material` | 小红书 AI 懒惰模式讨论帖素材包 |
| `site/social/xiaohongshu/spring-recruit/spring-recruit-post-pack-2026-03-29.md` | `material` | 小红书春招主题帖素材包 |
| `site/social/xiaohongshu/spring-recruit/spring-recruit-carousel-pack-2026-03-29.md` | `material` | 小红书春招轮播图素材包 |
| `site/social/xiaohongshu/ai-persona/ai-persona-world-post-pack-2026-03-30.md` | `material` | 小红书 AI 人格 vs 世界内行动主题帖素材包 |
| `site/social/xiaohongshu/ai-persona/ai-persona-carousel-pack-2026-03-30.md` | `material` | 小红书 AI 人格主题轮播图素材包 |
| `site/social/xiaohongshu/demo-skepticism/demo-skepticism-post-pack-2026-03-31.md` | `material` | 小红书 demo 祛魅主题帖素材包 |
| `site/social/xiaohongshu/demo-skepticism/demo-skepticism-carousel-pack-2026-03-31.md` | `material` | 小红书 demo 祛魅主题轮播图素材包 |
| `site/social/xiaohongshu/gui-death/gui-death-post-pack-2026-04-01.md` | `material` | 小红书 GUI 退场 / 判断权主题帖素材包 |
| `site/social/xiaohongshu/kaifeng-rpg/kaifeng-rpg-post-pack-2026-04-06.md` | `material` | 小红书开封真人 NPC / 低门槛实景 RPG 主题帖素材包 |
| `site/social/xiaohongshu/offer-choice/offer-choice-carousel-pack-2026-04-03.md` | `material` | 小红书 offer 选择 / 平台优先主题轮播图素材包 |
| `site/social/xiaohongshu/offer-choice/offer-choice-post-pack-2026-04-03.md` | `material` | 小红书 offer 选择 / 平台优先主题帖素材包 |

## 说明
- 本索引用于保证模块专题文档在根入口文档树中可达。
- 文档配对规则：`*.prd.md`、`*.design.md` 与相关 PR 和实际验证记录。
- ROUND-002 的 Gap 主从关系仅作历史定位；当前节点、复制、WASM 和拓扑操作从两份历史 PRD 的“现行去向”进入专业文档。
- `material` 与 `execution_log` 仅是补充入口，不替代 `canonical` 权威口径；当二者与正式 PRD 有冲突时，以 `canonical` 为准。
- `doc/readme/governance/README.md` 是热点子域 landing page，不替代本页的完整长表索引。
- 已删除的 README 顶层链接检查与根 README 公开状态对齐一次性专题不再保留原址；当前分别由 `scripts/readme-link-check.sh`、根 `README.md`、`doc/readme/prd.md`、一致性 checklist、季度复核专题、release communication surfaces、git history 与 PR、实际 CI 与评审记录（Issue 按需） 追溯。
- 旧 `TASK-README-014/015` Moltbook 一次性 role handoff briefs 已退役删除；当前渠道方法与素材从 `doc/readme/governance/readme-moltbook-liveops-runbook.md` 和 `doc/readme/governance/readme-moltbook-post-pack.md` 进入；旧 promotion 与 handoff 从 Git history 与实际 PR/已有 Task 追溯，不依赖退役 `.pm`。
