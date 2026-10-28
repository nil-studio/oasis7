# site PRD

审计轮次: 6

## 目标
- 建立 site 模块设计主文档，统一需求边界、技术方案与验收标准。
- 确保 site 模块后续改动可追溯到 PRD-ID、任务和测试。

## 范围
- 覆盖 site 模块当前能力设计、接口边界、测试口径与演进路线。
- 覆盖稳定站点要求及验证方法；本次要求与执行证据的关联由对应 PR/已有 Task 承接。
- 不覆盖实现代码逐行说明与历史过程记录。

## 接口 / 数据
- PRD 主入口: `doc/site/prd.md`
- 项目管理入口: 对应 PR、实际 CI 与评审记录（Issue 按需）；分工见[开发流程规范](../engineering/workflow/source-of-truth.md#information-record-placement)
- 文件级索引: `doc/site/prd.index.md`
- 追踪主键: `PRD-SITE-xxx`
- 测试与发布参考: `testing-manual.md`

## 里程碑

M1–M3 仅是 2026-03 的历史建档/同步记录，未核到唯一原执行 Task，原文保留，不视为当前进度；长期追踪按实际 PR、CI 与评审执行，Issue/Project 按需。
- M1 (2026-03-03): 完成模块设计 PRD 主体重写与任务改造。
- M2 (2026-03-07): 完成站点手册镜像语义同步与状态文档回写。
- M3 (2026-03-07): 完成对外“可玩状态”口径与真实产品状态对齐。
- M4: 建立 PRD-ID -> Task -> Test 的长期追踪闭环。

## 风险
- 模块边界演进快，文档同步可能滞后。
- 指标口径不稳定会降低验收一致性。
## 1. Executive Summary
- Problem Statement: 站点页面、发布下载入口、文档镜像与 SEO 优化在多轮迭代中快速演进；若对外叙事与真实可玩状态不一致，会直接损伤用户信任。
- Proposed Solution: 将 site PRD 作为站点设计主入口，统一页面结构、内容同步、发布链路、质量门禁与“真实状态口径”约束。
- Success Criteria:
  - SC-1: 首页关键信息架构与模块设计口径保持一致。
  - SC-2: 发布下载链接在版本发布后可用率达到 100%。
  - SC-3: 关键页面的可访问性与性能指标持续满足目标阈值。
  - SC-4: 站点改动关联实际适用要求与验证证据；已有 PRD-SITE-ID 用于需求追踪，不是普通任务准入身份，不为简单编辑虚构要求。
  - SC-5: 首页与文档入口页 100% 统一标注 `limited playable technical preview`，并明确它不是 closed beta、public launch、season live、普通玩家正式版本或公开网页加入流程。
  - SC-6: 首页不得出现与真实状态冲突的“已可玩/赛季进行中”表述（违规数 0）。
  - SC-7: `doc/site/github-pages/**` 活跃专题中的当前校验命令、viewer crate 路径与 wasm 包名必须统一使用 `oasis7_viewer` / `crates/oasis7*` 口径；旧品牌 viewer 包名与源码路径仅允许保留在历史证据或外部原文引用中。
  - SC-8: 首页首屏与首个信息段必须在 30 秒内回答“这是什么游戏 / 玩家如何参与 / 现在可以做什么”，避免陌生访客把公开站点误读为纯工程状态页。
  - SC-9: 首页必须显式区分“公开访客入口”“builder/developer 验证路径”“未来平台化方向”三层，不允许把当前公开能力、诊断命令和未来模块平台目标混写成同一层承诺。
  - SC-10: 中英首页必须共享同一组首页 claim gate，至少锁定可玩状态、默认公开访问面、下载边界、正式公告状态与未来平台仍未开放这五类关键口径。
  - SC-11: 移动端首页在脚本失效时仍保留可达导航入口，且键盘用户进入页面后可直接跳过顶栏进入正文。
  - SC-12: `site/doc/{cn,en}/index.html` 必须明确定位为首页之后的次级深读入口，不得继续充当第一次对外介绍项目的主宣传面，也不得指向缺失的首页锚点。

<a id="site-page-requirements"></a>
## 2. User Experience & Functionality
- User Personas:
  - 新访问者：需要快速理解项目价值与安装入口。
  - 技术用户：需要稳定访问文档与发布资产。
  - 站点维护者：需要统一发布与验收标准。
- User Scenarios & Frequency:
  - 首页信息浏览：每位新访问者首次访问执行。
  - 文档与下载访问：技术用户按需高频访问。
  - 发布前巡检：每次版本发布前执行一次完整检查。
  - 发布后回归：每次发布后执行稳定性与断链复核。
- User Stories:
  - PRD-SITE-001: As a 新访问者, I want a clear homepage narrative, so that I can understand the product quickly.
  - PRD-SITE-002: As a 技术用户, I want trustworthy download and docs links, so that I can install and verify efficiently.
  - PRD-SITE-003: As a 维护者, I want measurable quality gates, so that releases are predictable.
  - PRD-SITE-004: As a 对外内容维护者, I want marketing copy to match real product readiness, so that user trust is not damaged by over-claiming.
- PRD-SITE-005: As a 新访问者, I want the public site to distinguish preview build notes from formal public release messaging, so that I do not mistake technical artifacts for a live launch.
- PRD-SITE-006: As a `liveops_community`, I want a public placeholder for upcoming release communication, so that formal announcement rollout has a stable site anchor.
- PRD-SITE-007: As a `producer_system_designer`, I want public site copy to remain aligned with candidate posture, so that release promises never outrun internal review status.
- PRD-SITE-008: As a 新访问者, I want the public site and release downloads to use the canonical `oasis7` repo path and asset names, so that branding, links, and downloaded files stay consistent.
- PRD-SITE-009: As a 新访问者, I want the homepage to explain the game genre, player role, and current availability before technical validation details, so that I can decide within the first screen whether oasis7 is worth following.
- PRD-SITE-010: As a 新访问者, I want the homepage to separate current public entry, builder verification, and future platform direction, so that I do not mistake preview validation or roadmap language for a live player promise.
- PRD-SITE-011: As a 技术用户或自动化代理, I want a stable public `oasis7` skill Markdown URL, so that I can fetch the Local Provider workflow without cloning the repo first.
<a id="prd-site-012"></a>
- PRD-SITE-012: As a `producer_system_designer` or external collaborator, I want a public HTML roadshow deck inside the site, so that I can present the project in a controlled sequence without rebuilding the public homepage into a slide script.

Trace refs: O7-REQ-6cbda46f-bba2-423f-8197-2a15a4b1d5a6；既有产品结果由 [接入发行 SC-11](../product/player-entry-distribution/prd.md#sc-entry-011)唯一声明；本专业原文继续拥有静态发布、导航、维护权限及验收细节，不改变以下原要求或决策。

- Critical User Flows:
  1. Flow-SITE-001: `访问首页 -> 理解价值与入口 -> 跳转安装/文档`
  2. Flow-SITE-002: `发布前执行链接检查 -> 处理断链 -> 复测通过`
  3. Flow-SITE-003: `发布后监控质量指标 -> 发现退化 -> 回滚或修复`
  4. Flow-SITE-004: `确认真实可玩状态 -> 回写首页/文档入口口径 -> 复核中英一致性 -> 发布`
  5. Flow-SITE-005: `陌生访客访问首页 -> 在首屏理解游戏类型/玩家角色/当前开放状态 -> 决定继续看玩法、技术预览或文档`
  6. Flow-SITE-006: `陌生访客访问首页 -> 区分当前可做的事/开发者验证/未来目标态 -> 选择继续看证据、下载预览或回到文档`
  7. Flow-SITE-007: `访客先通过首页理解产品与公开边界 -> 再进入 docs hub 深读完整总览/玩法/验证文档`
  8. Flow-SITE-008: `执行者或代理进入 docs hub -> 直接打开公开 oasis7 skill Markdown -> 按 skill 中的命令启动 Local Provider real-play`
  9. Flow-SITE-009: `访客或对外沟通者进入 docs hub / 直接打开 deck -> 按固定顺序浏览项目定位、玩法差异、当前证据与阶段边界 -> 决定是否继续深读或跟进`
  10. Flow-SITE-010: `陌生访客进入首页 -> 首屏理解游戏与玩家位置 -> 顺着一条后果链理解玩法 -> 查看当前真实证据与预览边界 -> 决定下载、深读或继续跟进`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 首页信息架构 | 版块标题、类型定义、玩家角色、当前开放状态、入口链接、版本信息 | 点击跳转玩法说明/技术预览/文档/下载 | `draft -> published -> revised` | 先回答“是什么/怎么参与/现在能做什么”，再下沉技术细节 | 站点维护者可修改 |
| 发布下载链路 | 版本号、资产地址、校验信息 | 发布后自动校验可用性 | `prepared -> published -> verified` | 最新版本优先展示 | 发布负责人审批上线 |
| 质量门禁巡检 | 链接状态、性能指标、可访问性结果 | 巡检失败阻断发布 | `checking -> passed/blocked` | 严重问题优先修复 | 维护者可解阻断（需说明） |
| 对外状态口径 | 产品状态（playable / preview）、禁用词列表、状态标识文案 | 发布前校验首页与文档入口文案是否匹配真实状态 | `draft -> aligned -> published` | 真实状态字段优先于营销文案 | 模块维护者可改，发布责任人复核 |
| 首页入口分层与 claim gate | 首屏定位、默认公开访问面、builder 验证入口、未来方向、下载边界、中英镜像 claim | 发布前校验首页是否把当前公开能力/开发者路径/未来平台目标分层表达，并校验关键 claim 中英一致 | `draft -> aligned -> published -> gated` | 先公开访客入口，再 builder 验证，再 future roadmap；诊断路径不得进入 primary CTA | 模块维护者可改，发布责任人复核 |
| 公开 skill 镜像 | skill 名称、摘要、raw Markdown 链接 | docs hub 直跳 raw skill Markdown；raw URL 可直接抓取 | `draft -> mirrored -> published -> refreshed` | 只保留 raw Markdown 分发入口；内容必须与 repo-local skill 真值一致 | 模块维护者可改，发布责任人复核 |
| <a id="deck-functional-contract"></a>HTML 路演 Deck | deck 入口 URL、页面语言、固定 slide 顺序、当前状态提示、回链入口 | docs hub / 公开入口直跳 deck；键盘、触摸与 URL hash 导航可用 | `draft -> linked -> published -> revised` | deck 负责顺序化讲述项目，不替代首页公开边界，也不替代 PRD 真值 | 模块维护者可改，发布责任人复核 |
| 首页游戏优先叙事 | 游戏定位、玩家角色、局势后果链、真实证据、预览边界、下载/深读路径 | 访客按单一阅读路径理解游戏，再按意图进入证据、下载或文档 | `dense -> compressed -> evidenced -> gated` | 首屏与后果链优先；builder、架构和信任链细节渐进披露；最多五个首要导航入口 | 模块维护者可改，视觉交互与对外口径角色复核 |
- Acceptance Criteria:
  - AC-1: site PRD 定义页面层级、内容同步和发布链路。
  - AC-2: 对应 PR/已有 Task 记录本次范围、适用要求、验证版本与结果；Issue/Project 按需，长期 PRD 不承载可变任务状态。
  - AC-3: 与 `site/doc` 和 GitHub Pages 相关设计文档口径一致。
  - AC-4: 发布前完成链接有效性与基础质量检查。
  - AC-5: 首页与文档入口页（`site/index.html`、`site/en/index.html`、`site/doc/cn/index.html`、`site/doc/en/index.html`）对“是否可玩”状态的表达必须一致且与真实状态相符。
  - AC-6: `doc/site/github-pages/**` 仍可读历史专题的首行标题必须统一使用 `oasis7` 品牌；旧 `oasis7*` 标题仅允许保留在正文历史上下文与证据原文中。
  - AC-7: `doc/site/github-pages/**` 活跃专题中的当前 `cargo check -p` 命令、viewer crate 路径与 wasm 包名必须写为 `oasis7_viewer` / `crates/oasis7*`；旧品牌 viewer 包名与源码路径仅允许保留在历史证据或外部原文引用中。
  - AC-8: `site/index.html` 与 `site/en/index.html` 的首屏和首个正文段必须明确交代游戏类型、玩家扮演的角色、核心差异点，以及“当前仍是技术预览”的边界，且中英结构保持同构。
  - AC-9: 首页必须明确说明 `viewer` / `viewer.html` 是唯一公开 Web 验证入口；`software_safe` 仅作为兼容 alias 保留；`--no-llm` 仅允许出现在诊断/排障语境，不得继续作为首页 primary path 展示。
  - AC-10: 首页必须以访客能理解的语言拆开“当前公开可做的事”“builder/developer 验证路径”“未来模块平台方向”，并明确当前未开放 creator-facing module/platform。
  - AC-11: `site/index.html` 与 `site/en/index.html` 都必须通过统一的 homepage claim/parity check，覆盖可玩状态、下载边界、正式公告状态、公开访问面与未来平台边界。
  - AC-12: 移动端顶栏在无 JS 情况下仍能看到导航链接；首页提供 skip link 直达 `main`。
  - AC-13: `site/doc/cn/index.html` 与 `site/doc/en/index.html` 的 hero、primary CTA 与入口卡片必须明确说明“首页优先、docs hub 次级深读”，并且不允许继续链接到缺失的首页锚点。
  - AC-14: 公开站点必须新增可直接抓取的 `site/skills/oasis7.md`；docs hub 中英页都要能直接跳达这个 raw skill 入口。
  <a id="ac-15"></a>
  - AC-15: 公开站点必须新增一个可直接访问的 HTML deck 页面，用固定 slide 顺序说明项目定位、玩法差异、当前阶段与后续路径，且不把首页改造成演示文稿模式。

  Trace refs: O7-REQ-6cbda46f-bba2-423f-8197-2a15a4b1d5a6；[SC-11 原义接收](../product/player-entry-distribution/prd.md#sc-entry-011)，本 AC 原义与适用边界保持。

  <a id="ac-16"></a>
  - AC-16: deck 实现必须适配当前 `site/**` 静态发布链路；允许引入浏览器端演示库，但不得要求新增 Node/Vite/SPA 构建步骤才能发布到 Pages。

  Trace refs: O7-REQ-6cbda46f-bba2-423f-8197-2a15a4b1d5a6；[SC-11 原义接收](../product/player-entry-distribution/prd.md#sc-entry-011)，本 AC 原义与适用边界保持。

  <a id="ac-17"></a>
  - AC-17: docs hub 或首页必须存在清晰 deck 入口；deck 页面本身必须保留回到首页 / docs hub 的导航，并显式标注当前仍为 `limited playable technical preview`，不是 closed beta 或正式玩家发布。

  Trace refs: O7-REQ-6cbda46f-bba2-423f-8197-2a15a4b1d5a6；[SC-11 原义接收](../product/player-entry-distribution/prd.md#sc-entry-011)，本 AC 原义与适用边界保持。

  - AC-18: 中英首页必须按“游戏与玩家位置 -> 一局后果链 -> 当前真实证据与预览边界 -> 下载/深读/跟进”形成单一主阅读路径，减少重复解释，不得把 builder 术语重新前置。
  - AC-19: 桌面首页首要导航入口不超过五个，并按已有内容分配；当前采用玩法、开发进展、世界观、技术预览。世界观到明确标为小说的导读；空世界记录留在页尾或次级状态。文档、架构和开发者路径通过页尾或渐进披露保持可达。
  - AC-20: 首页使用真实 Viewer 截图时必须标注受控场景与证据边界，不得把截图表达成公开网页可玩入口或正式发布证明。
  - AC-21: 1440x900、390x844 与 360x800 的浏览器证据必须确认首屏层级、无横向溢出、移动端导航与 CTA 可达；既有 link/manual/download/homepage-claim gates 必须继续通过。
  - AC-22: 首页对 persistent 的表述必须限定为世界状态、历史与后果可持续/可回放；不得承诺 7x24 公共托管、在线 SLA，或本地 runtime 停止后仍后台推进。
- Non-Goals:
  - 不在 site PRD 中定义 runtime/p2p 低层实现。
  - 不覆盖内部测试流程细节（由 testing 模块负责）。
  - 不重做 deck、story、manual 或 docs hub，不引入 SPA/构建系统，不新增概念图，也不改变 runtime、installer 或 release 能力；现有故事入口和章节状态的有界正确性修复按下节合同执行。

<a id="story-entry-and-state-contract"></a>
### 故事首访与章节状态合同

承接 PRD-SITE-002 的公开入口可用性；设计见 [故事路由与状态机](design.md#story-reader-design)。
本节只修现有故事页的入口和阅读行为，不要求翻译正文、新增英文故事页、重写小说或引入 SPA。

| 专业验收 | 用户可观察结果 |
| --- | --- |
| STORY-AC-1 | 干净英文首选浏览器直达 `/story/` 或 `/story/index.html` 时，保留有内容的中文页面并可加载章节；只对明确有对应语言页面的入口自动选语言 |
| STORY-AC-2 | 手动选择优先并持久化；首页双语、文档路径排除、根域/项目子路径及 query/hash 均保持，单语言页面不消耗首页首次语言判断 |
| STORY-AC-3 | 页面只显示当前章节的真实 `loading/ready/error/empty` 状态；生产页面不显示状态示例集合，不虚构阅读进度同步故障 |
| STORY-AC-4 | 404/网络失败进入 error，提供当前章节重试或实际原文入口；200 空串/全空白进入 empty，不宣告正文已加载；正常内容进入 ready |
| STORY-AC-5 | 快速切章时，旧请求的成功或失败都不能覆盖最新章节、选中项、状态或 aria-busy；重试仍针对当前选择 |

以同一候选从干净英文直达故事入口后继续覆盖延迟、失败、空白、正常与快速切换矩阵，回归首页和文档入口。
静态链接检查和 Pages CI 只证明各自静态合同，不能替代上述行为及适用浏览器/视觉验证；未运行的场景保持未验证。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 静态站构建链路、链接检查、截图/视觉基线流程。
- Evaluation Strategy: 以页面可用性、链接有效率、发布后回滚次数与问题修复时长评估。

## 4. Technical Specifications
- Architecture Overview: site 模块负责对外展示与文档镜像层，与 readme/testing/world-simulator 等模块联动维护入口一致性。
- Integration Points:
  - `site/`
  - `site/doc/`
  - `site/deck/`
  - `site/skills/`
  - `doc/site/github-pages/`
  - `doc/site/manual/`
  - `doc/readme/prd.md`
  - `site/skills/oasis7.md`
### 当前 Pages、下载与镜像合同
- `site/index.html` 与 `site/en/index.html` 是公开 CN/EN 首页；`site/assets/app.js` 和 `site/assets/styles.css` 是其静态交互与无脚本降级的实现面。它们必须继续把 `viewer` 表达为默认 Web 验证入口、把 `software_safe` 限定为兼容 alias，并保持 `limited playable technical preview` 的公开边界。
- 首页不初始化已隐藏的 Canvas；保留的滚动高亮等交互仅是渐进增强，不能改变无 JS、键盘或 reduced-motion 访问下的内容、导航、CTA 与公开 claim；共享架构图必须保留稳定 SVG 引用和可访问语义。具体视觉层级、响应式和页面覆盖规则以 `doc/site/design.md` 为准。
- Pages 发布在 `.github/workflows/pages.yml` 中先执行四个当前门禁：`scripts/site-link-check.sh`、`scripts/site-homepage-claim-check.sh`、`scripts/site-manual-sync-check.sh` 与 `scripts/site-download-check.sh`；任一失败都不应部署静态站点。前两个分别守本地资源可达性与 CN/EN claim/no-JS 导航，后两个分别守 Viewer 手册镜像和下载面。
- 下载面只分发 `eng-cc/oasis7` GitHub Releases 的 Windows `.exe`、macOS `.dmg`、Linux `.deb` 与 checksum；Linux 运维恢复工具另以带 manifest/checksum 的 `oasis7-linux-x64-ops-tools.tar.gz` 独立资产提供。首页可按平台选择主 CTA，但必须在下载前保留版本、安装、信任与支持边界。资产存在或检查通过不构成普通用户广泛发行、Windows codesigning 或 macOS notarization 已就绪的声明。
- 升级仅支持重新下载最新主包并手动覆盖/替换。`config.toml`、`.oasis7_launcher_ux_state.json` 与 `output/chain-runtime/<node_id>/reward-runtime-execution-world/` 是需要用户先备份的相对路径状态；不得把该说明升级为应用内更新、自动迁移或跨目录状态保留承诺。
- `site/doc/{cn,en}/viewer-manual.html` 是静态发布镜像，不是 Viewer 手册真值；其 canonical source 是 `doc/world-simulator/viewer/viewer-manual.manual.md`，并由同步门禁约束当前 agent-browser 命令基线。
- Edge Cases & Error Handling:
  - 断链：发现下载或文档断链时阻断发布并进入修复流程。
  - 空页面：关键页面内容缺失时展示维护提示并记录异常。
  - 权限不足：发布权限缺失时拒绝上线并提示责任人。
  - 超时：构建/巡检超时时输出中间结果并允许重试。
  - 并发发布：同版本并发发布时只允许一个发布会话生效。
  - 数据异常：版本元数据错误时不展示到公开页面。
  - 状态漂移：若页面把 `limited playable technical preview` 表达成 closed beta、public launch、season live、普通玩家正式版本或公开网页加入流程，发布流程必须阻断并回写文案。
  - 入口漂移：若首页把 builder 命令、未来平台目标或诊断入口写成当前公开主入口，发布流程必须阻断并回写文案。
  - 无脚本访问：若移动端脚本失效导致导航入口不可达，发布流程必须阻断并回写实现。
  - skill 漂移：若 `site/skills/oasis7.md` 与当前 repo-native Local Provider 启动命令、桥接命令或公开边界不一致，发布流程必须阻断并回写公开 skill 内容。
- Non-Functional Requirements:
  - NFR-SITE-1: 发布后关键链接可用率 100%。
  - NFR-SITE-2: 核心页面性能与可访问性指标达到门禁阈值。
  - NFR-SITE-3: 多语言内容口径一致并可追溯。
  - NFR-SITE-4: 发布回滚流程可在限定时间内执行。
  - NFR-SITE-5: 站点输出不得暴露内部敏感配置。
  - NFR-SITE-6: “可玩状态口径一致性”检查在发布前必须完成且误报率 <= 5%（按月统计）。
  - NFR-SITE-7: 首页 claim/parity gate 必须覆盖 CN/EN 双首页，并在 Pages workflow 中执行，避免关键对外承诺静默漂移。
  - NFR-SITE-8: 首页主要导航在移动端不依赖脚本才能可达；键盘访问路径需具备可见 skip link。
  - NFR-SITE-9: 公开 skill Markdown URL 必须稳定、可直接访问，且其内容在同一次发布中与当前 repo-native Local Provider 工作流保持语义一致。
  - NFR-SITE-10: Pages workflow 的四项站点门禁与公开下载资产名必须和当前静态首页同步；门禁成功只证明该静态合同，不替代 release gate、安装器验收或玩家可玩性结论。
- Security & Privacy: 站点不得暴露内部凭据与敏感配置；下载链路需具备来源可验证性。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03-03): 固化站点信息架构与发布验收口径。
  - v1.1: 增加多语言内容一致性与截图回归基线。
  - v2.0: 建立站点发布质量趋势跟踪（性能、可访问性、失效率）。
- Technical Risks:
  - 风险-1: 内容更新频率高导致页面口径漂移。
  - 风险-2: 发布资产链接策略变化引入断链风险。
  - 风险-3: 对外文案过度承诺导致信任流失与社区负反馈。

## 6. Validation & Decision Record
以下 TASK-SITE 标识与旧切片名称保留历史来源关系，不作为新任务身份、现行排期或完成证明；未核到准确原 PR/Task 接收方前不迁移或改写原映射。新变更按实际适用要求关联当前 PR/已有 Task。

- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-SITE-001 | TASK-SITE-001/002/005 + public-copy-tightening | `test_tier_required` | 首页与 docs hub 入口结构检查 | 用户首次访问体验 |
| PRD-SITE-002 | TASK-SITE-002/003/005/006/020 | `test_tier_required` | 下载与文档链接巡检、手册镜像语义对齐校验 | 安装与文档可用性 |
| PRD-SITE-003 | TASK-SITE-003/004/005/007/020 | `test_tier_required` + `test_tier_full` | 发布门禁与回归节奏复核、项目状态回写核对 | 发布稳定性与回滚能力 |
| PRD-SITE-004 | TASK-SITE-008/009 + public-copy-tightening | `test_tier_required` | 中英文首页与文档入口页状态文案一致性核验 | 对外信任与信息准确性 |
| PRD-SITE-005 | TASK-SITE-010 | `test_tier_required` | 首页/下载区已区分构建说明与正式公告 | 公开站点状态理解 |
| PRD-SITE-006 | TASK-SITE-010 | `test_tier_required` | 站点存在统一“公开说明准备态”占位 | 发布沟通入口一致性 |
| PRD-SITE-007 | TASK-SITE-010 | `test_tier_required` | 技术预览主口径与新占位并存且无冲突 | 对外承诺边界控制 |
| PRD-SITE-008 | TASK-SITE-015/016/017/018 | `test_tier_required` | `site/**`、GitHub Release 下载入口、release workflow 当前路径/包名与 `doc/site/github-pages/**` 历史专题标题全部切换到 `oasis7` 品牌与 `eng-cc/oasis7` 路径 | 对外品牌一致性、下载链路稳定性 |
| PRD-SITE-009 | homepage-game-explainer + public-copy-tightening | `test_tier_required` | 中英首页首屏与 docs hub 首段信息架构核对、文档治理检查、静态站点可视回归 | 新访客首访理解效率、公开定位清晰度 |
| PRD-SITE-010 | homepage-entry-claim-boundary-hardening + public-copy-tightening | `test_tier_required` | 首页 claim/parity gate、CN/EN 首页分层文案核对、docs hub 角色定位检查、移动端导航/a11y 检查 | 首页入口真值、builder 路径边界、未来平台口径控制 |
| PRD-SITE-011 | public-oasis7-skill | `test_tier_required` | 中英 docs hub raw skill 入口核对、`site/skills/oasis7.md` 对 `scripts/setup-provider-oasis7-runtime.sh`、`scripts/provider-parity-p0.sh` 与 `oasis7_game_launcher` 当前公开命令的语义一致性检查 | 对外 skill 可获取性、自动化抓取稳定性 |
| <a id="validation-site-012"></a>PRD-SITE-012 | html-roadshow-deck | `test_tier_required` | docs hub / deck 入口核对、静态链接检查、deck 页面导航与当前状态文案核对 | 对外顺序化讲述能力、静态发布兼容性 |
| PRD-SITE-001/004/009/010 | github-pages-ui-content-refresh-20260717 | `test_tier_required` | 首页 claim/link/manual/download gates、CN/EN 结构核对、桌面与移动端浏览器截图及交互 smoke | 首访理解效率、公开口径、响应式视觉层级 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-SITE-001 | 发布前强制执行质量巡检 | 发布后补检查 | 可提前发现阻断问题。 |
| DEC-SITE-002 | 下载链路绑定版本与校验信息 | 仅展示下载地址 | 可提升来源可信度。 |
| DEC-SITE-003 | 站点口径与 readme 联动维护 | 独立维护站点文案 | 可降低对外口径漂移。 |
| DEC-SITE-004 | 对外“可玩状态”按真实产品状态保守表达（未可玩即明确预览） | 继续使用高承诺营销口径 | 可避免误导用户并降低信任风险。 |
| DEC-SITE-005 | 站点先补“说明准备态”占位，再等待正式公告入口 | 直接在公开站点暗示正式发布已临近 | 站点公开承诺必须保持晚于内部正式沟通动作。 |
| DEC-SITE-006 | 公开站点、GitHub Pages canonical 与 release 资产名统一为 `oasis7` | 保留旧 `oasis7` 外显名称仅改仓库 slug | 外部访问者最先接触的是站点与下载名，品牌必须先在这一层完全一致。 |
| DEC-SITE-007 | 首页优先讲清“游戏是什么、玩家像什么、现在能做什么”，技术验证与下载说明下沉到后续版块 | 继续让首屏以技术预览、命令链路和工程状态为主 | 对陌生访客来说，先理解产品语义，再判断是否深入技术细节，才是更低摩擦的公开入口。 |
| DEC-SITE-008 | 首页公开入口按“访客理解 -> builder 验证 -> 未来方向”三层分流，并显式把 `viewer` 设为默认 formal Web 入口，旧 `software_safe` 只保留为兼容 alias | 继续把访问面 taxonomy、开发命令和未来平台目标混写在同一层 | 可同时降低误导风险、减少陌生访客理解负担，并保持 runtime 真值可审计。 |
| DEC-SITE-009 | Pages 发布前新增 homepage claim/parity gate 与基础无脚本/a11y 约束 | 继续只依赖链接/手册/下载静态检查 | 首页承担最高风险的对外承诺，需要单独门禁而不是把口径漂移留给人工发现。 |
| DEC-SITE-010 | docs hub 公开层只承担“深读/验证导航”，第一次对外介绍仍以首页为主 | 让 docs hub 与首页并列承担首次产品介绍 | docs hub 混入太多协作/验证语境时，陌生访客会更快把站点误读成工程入口而不是游戏公开面。 |
| DEC-SITE-011 | 对外 `oasis7` skill 只保留站内 raw Markdown 直链，并在 docs hub 暴露入口 | 新增独立 HTML 说明页或只暴露 GitHub blob 路径 | 用户目标是直接获取 skill 内容；单一 raw 入口更短、更稳定，也减少公开镜像维护面。 |
| <a id="dec-site-012"></a>DEC-SITE-012 | 路演材料以站内 HTML deck 形式挂在 `site/` 下，并复用静态 Pages 链路 | 继续只靠首页/长文档承担路演，或为 deck 单独引入 SPA 构建工程 | deck 需要的是顺序化讲述，不是新的前端系统；保持静态站发布可减少维护面并保留直接访问能力。 |
| DEC-SITE-013 | 首页采用游戏优先的叙事压缩，保留现有视觉语言、claim gate 与静态发布基础 | 只做表面样式微调，或改造成重动效沉浸式展示 | 当前主要问题是重复内容与同质卡片削弱层级；压缩信息架构能改善首访理解，同时避免概念图和新交互扩大承诺与回归风险。 |
| DEC-SITE-014 | 将当前 Pages、下载、镜像、信任与手动升级合同收敛到 site root PRD/project，并把 dated 专题降为历史证据 | 继续让 dated release-pipeline 专题承担当前首读真值 | 可执行脚本、workflow 与 root authority 已是当前合同；保留旧专题为默认入口会把历史 CI/就绪记录误读为当前发布承诺。 |
