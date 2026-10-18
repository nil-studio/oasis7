# 官网发现、玩法展示与持续关注

## 文档身份

- 所属产品模块：玩家接入与发行
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：active
- 状态边界：静态实现与完整验收分别表达，不代表已经部署或发行升级
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-16
- 设计判定：`paired-design`。
- 配对产品设计：[`public-site-discovery-and-showcase.design.md`](public-site-discovery-and-showcase.design.md)
- 专业系统设计：[`官网发现与展示系统设计`](../../site/github-pages/public-site-discovery-and-showcase.design.md)
- 专业权威：[`site PRD`](../../site/prd.md)、[`site 设计`](../../site/design.md#homepage-layout-authority)、[`公开口径`](release-communications-and-public-claims.prd.md#public-claim-four-layer-model)、[`README`](../../../README.md)

<a id="paired-document-navigation"></a>
## 配套文档与阅读顺序

这三份文档属于同一个官网专题，按产品与专业实现职责分别维护；以下导航只聚合入口，不复制正文。

| 产品需求 | 产品体验与视觉设计（设计师首读） | 系统实现设计（开发首读） |
| --- | --- | --- |
| [目标、范围与验收](public-site-discovery-and-showcase.prd.md) | [文案、布局、素材、分镜与交付物](public-site-discovery-and-showcase.design.md) | [结构、状态、资源与验证](../../site/github-pages/public-site-discovery-and-showcase.design.md) |

设计师从产品体验与视觉设计开始，再按需核对产品要求；开发者从系统实现设计开始，并回查上游体验。文件位置不改变上述职责，文档存在不表示实现或验收完成。

## 1. 问题与目标

一位没参与过开发的访客听说 oasis7，打开官网。他先想知道自己在游戏里做什么、AI 替自己做什么、一个选择会带来什么后果，再决定观看、了解预览或继续关注。他不应先学会读测试日志、PR 和架构文档，也不应把设计稿、示例回放或小说当作当前可玩的能力。

目标是形成“理解玩家位置 → 看懂一次选择 → 判断当前可参与范围 → 找到下一步”的公开发现路径。它承接上位 SC-1/2/3/4/6 的发现与说明部分，不替代后续真实下载安装、进入世界和发行验收。

**已核对事实。** 以 `116c2e6d4f4891c316b305764a1f755a3005d3ae` 为只读基线，[首页源码](https://github.com/eng-cc/oasis7/blob/116c2e6d4f4891c316b305764a1f755a3005d3ae/site/index.html)已有危机场景切换、说明性事件、Viewer 截图、平台下载和文档入口；[小说页](https://github.com/eng-cc/oasis7/blob/116c2e6d4f4891c316b305764a1f755a3005d3ae/site/story/index.html)已有阅读器。源码阅读不是线上浏览器、素材真实性或完整试玩验收。

**设计假设。** 展示具体决策、集中说明参与条件、提供短篇进展，可能比继续增加宏观概念更帮助陌生访客理解。该假设由真实访客观察验证；若访客仍把自己理解为旁观者、认为公共世界已经开放，或找不到下一步，应修改对应展示而非仅美化页面。转化与留存基线未验证，不设虚构提升百分比。

<a id="visitor-value-and-outcomes"></a>
### 1.1 首要受众与值得继续看的理由

首要受众是对 AI Agent 文明模拟、经营取舍和世界后果感兴趣、尚不了解项目的人，而非已经会运行工程脚本的人。宣传主张以“你决定方向与边界，Agent 在资源约束中行动，你根据结果继续判断”为中心。不能仅用“AI 原生”“持久世界”等名词替代玩家收益，也不能把目标态当作当前已验证能力。

| 访问动机 | 本次访问应得到的价值 | 合适的下一步 |
| --- | --- | --- |
| 想知道是否好玩 | 看懂一个具体目标、一次取舍和已经发生或尚未发生的后果，知道自己不是仅观看日志 | 了解玩法，再自主了解技术预览 |
| 想研究或接入 Agent | 在理解同一玩法后找到真实文档、Skill 与复现条件，不必重新浏览一套宣传首页 | 进入既有技术入口；不把接入工具当普通玩家主 CTA |
| 想看世界观或持续关注 | 无需安装即可读懂小说导读或一篇变化说明，并保存有效的后续入口 | 小说、开发日志、RSS 或既有 GitHub 跟进 |

上述动机是编辑与评估分组，不新增账户类型、访客画像、强制角色选择器或游戏模式。对前两类读者都必须说明当前配置和阶段限制；“无需编程”不能推导为“无需配置”。

成功首先是**理解正确、选择合适、离开后有可找回的内容**，不是把更多人推向下载。沿用首屏 30 秒理解目标；完整案例后应能说出一次有意义的取舍；预览说明后应能判断自己是否具备条件。点击、下载、安装、完成游戏目标和回访是不同结果，不互相代签，也不为测量引入追踪服务。

<a id="visual-experience-outcomes"></a>
### 1.2 科技感、沉浸感与视觉冲击是独立产品结果

官网不仅要准确和可读，还应让人愿意停下来看：第一屏应能作为有辨识度的游戏宣传画面，后续阅读应像逐步接近同一座小行星文明，而不是浏览通用工具网站。保持“文明观测站”方向，但“克制”约束的是噪声和干扰，不是世界尺度、画面完成度或视觉张力。

| 维度 | 想让访客获得的感受 | 可检查的表现，而非替代指标 |
| --- | --- | --- |
| 科技感 | 精密、可信、属于这个工业文明的技术系统 | 材质、结构、排版、图标和关系线有一致语法；不是仅深色加绿光，也不是假终端 |
| 沉浸感 | 在接近同一个世界，能自主掌握探索节奏 | 远景、产业近景、一次决策和世界观余韵相互呼应；不是每屏换一张无关壁纸 |
| 视觉冲击 | 第一眼有规模、方向和记忆点，愿意继续看 | 大小尺度反差、主光源、清楚轮廓与留白形成视觉焦点；关闭动效仍成立，不以特效数量计分 |

这三项由 [REQ-SITE-DISCOVERY-007](#req-site-discovery-007)独立承接；006 继续负责可访问与恢复，两者不能互相替代。轻量动效可以增强空间感，非技术人员也应看到完整静态版本；动效开关、真实状态和主动作不成为沉浸效果的牺牲品。

## 2. 范围与 Non-Goals

首期范围：中英文首页、一个分步玩法展示、精选静态观测记录、面向玩家的开发日志入口、技术预览速览与 FAQ、世界观小说导读、对应设计稿及渐进增强/失败恢复；视觉深化覆盖同一批页面的主视觉、连续阅读节奏与可关闭的轻量动效，不另建体验站。

不在范围：实时星图或游戏服务接入、账号/手机号预约、支付、投稿平台、创作者市场、公开网页加入、自动更新客户端、第三方追踪服务、全站框架迁移、游戏规则改造、小说正文重写。媒体资料包、用户主动播放的短片和实时数据是后续候选，不是首期隐藏依赖。不保证固定更新周期、模型费用、邀测时间或新平台支持。

产品文档与生成式设计稿确定目标与验证方法，不宣布持久世界开放。当前静态实现以功能状态中的固定版本为依据；真实素材、完整验收和发行仍消费各自主责证据。

## 3. 体验与决策

| 访客选择 | 可见信息与机会成本 | 结果和下一步 |
| --- | --- | --- |
| 先看玩法 | 标明素材类型的目标、约束、Agent 响应与结果；只花阅读时间 | 可以回看、比较已记录步骤或了解预览；不触发世界动作 |
| 阅读观测/进展 | 日期、适用版本、证据范围与尚存限制 | 深读一条记录或文章，返回列表；不把历史记录当当前服务状态 |
| 尝试技术预览 | 当前可体验范围、平台、必要配置、模型提供方/费用责任和安装风险 | 自主进入受支持下载/文档；不自动安装、连接 provider 或发送凭据 |
| 暂不安装 | 小说、开发进展、现有 GitHub 跟进入口 | 无需注册即可继续了解或离开，不损失权益 |

主阅读顺序为：定位与主展示入口 → 一次具体真实受控过程 → 当前阶段与进展 → 预览已验证范围/下载/FAQ → 世界观小说导读 → 技术与反馈入口。没有条目时，世界记录保留稳定 URL 与次级空态入口，不占主导航。首屏仍保留一个资源压力问题、一个主要按钮、一个次要文字链接和可见技术预览状态，不恢复三列事实条。

适用异常包括素材缺失、来源失效、记录撤回、文章为空、版本信息获取失败、不支持的平台、脚本不可用、语言目标不存在和小说加载失败。页面必须保留真实原因及安全下一步；不自动切换成别的世界、模式、版本或未声明语言，不以“成功”占位隐藏缺失。

### 3.1 在决策点提供下一步，而非不断增加栏目

首屏主动作仍为“了解玩法”，次链接仍为“了解技术预览”。案例结束处让读者选择了解预览或深读同案例；预览区允许继续看开发进展而不安装；文章结尾提供相关内容或现有订阅入口。不要求返回页首或读完整页才能作选择，也不在每段同时摆放一排等权按钮。

官网观看、下载安装包和按源码复现实验是三条不同路径。后两条各自说明版本、条件和证据范围；下载按钮不能承诺自动复现首页案例。持续关注以回答“发生了什么变化、对玩家意味着什么、还有什么没解决”为价值，不用 PR 数量、假在线人数或空路线图制造活跃。

## 4. 产品要求与功能状态

以下功能 ID 同时对应独立 REQ；状态只覆盖本专题定义的适用范围。原六项共同核对基线为 2026-10-14、上述源提交、远程源码阅读环境；实现基线与对应检查范围见各功能点；尚未完成陌生访客理解和完整发行验收。系统设计“已覆盖”仅指条款可逐项定位，不等于已通过独立评审。

<a id="discovery-refinement-scope"></a>
### 本轮细化的范围与状态

下表细化复用原六项要求及其 Trace ID；视觉质量另以新增 007 声明，不将视觉吸引力混入可访问性。原功能点下的“已实现待验证”保留原静态范围与 2026-10-15 固定实现依据；**不覆盖下表新细化或 007**。本轮设计已经定义，代码覆盖待核实，验收未执行；不得用旧实现、旧截图或既有生成稿宣告细化已实现。

| 既有要求 | 新细化的产品结果 | 产品体验承接 | 系统设计承接 |
| --- | --- | --- | --- |
| [001](#req-site-discovery-001) | 说清玩家收益、一次取舍与合适下一步，而非只识别游戏名或技术名词 | [价值与入口](public-site-discovery-and-showcase.design.md#value-and-entry-contract) | [入口与案例](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-value-path) |
| [002](#req-site-discovery-002) | 统一四步顺序；同一案例呈现初始约束、实际调整主体及结果；未执行选项不冒充已发生后果 | [案例叙事](public-site-discovery-and-showcase.design.md#decision-walkthrough) | [入口与案例](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-value-path) |
| [003](#req-site-discovery-003) | 进展讲玩家价值；案例、文章与预览决策点存在真实的继续阅读/关注入口，分享不丢范围 | [回访与分享](public-site-discovery-and-showcase.design.md#editorial-return-path) | [编辑与分享](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-return-sharing) |
| [004](#req-site-discovery-004) | 访客可区分观看、下载候选和源码复现；预览速览帮助判断条件与成本，不制造开箱可玩承诺 | [预览决策](public-site-discovery-and-showcase.design.md#preview-decision) | [预览适配](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-preview-fit) |
| [005](#req-site-discovery-005) | 世界观有吸引人的短导读及可返回路径，页面和分享摘要都保持小说身份与实际语言 | [回访与分享](public-site-discovery-and-showcase.design.md#editorial-return-path) | [编辑与分享](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-return-sharing) |
| [006](#req-site-discovery-006) | 科技感来自可读的工业世界与信息层级，不依赖假遥测、动效或重资源；中宽、手机和降级入口同样成立 | [视觉语言](public-site-discovery-and-showcase.design.md#visual-language) | [视觉交付](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-visual-delivery) |

<a id="req-site-discovery-001"></a>
### REQ-SITE-DISCOVERY-001：访客理解与主路径

Trace ID: O7-REQ-4544302c-40be-4aa3-a557-21fe6f475c19

- 功能点：F-SITE-DISCOVERY-001；首期 CN/EN 首页；验收 [AC-SITE-DISCOVERY-001](#ac-site-discovery-001)。
- 要求：首屏和首个说明段必须让访客辨认游戏类型、玩家与 Agent 的分工、资源压力和当前可做事项；主按钮进入真实存在的展示，次链接进入预览说明，不用“立即进入世界”代替未开放入口。按钮不能直接调用模型或世界写接口。
- 系统设计状态：已覆盖
- 系统设计依据：[DES-SITE-ENTRY](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-entry)，含旧锚点与无脚本入口。
- 实现状态：已实现待验证
- 实现与验证依据：[固定实现](https://github.com/eng-cc/oasis7/tree/cde5a650e0aca7cb444b92699cbe5aff4e9ed17d/site)、[浏览器回归方法](../../../scripts/site-discovery-browser.test.cjs)与[静态派生检查](../../../scripts/site-discovery-check.py)；完整验收和未覆盖项由[实施 PR #4697](https://github.com/eng-cc/oasis7/pull/4697)承接。
- 核对日期与基线：2026-10-15；本专题要求；实现 `cde5a650e0aca7cb444b92699cbe5aff4e9ed17d`；本地静态服务器与外部 Google Chrome；本功能 AC 的静态页面范围。
- 剩余范围：陌生访客理解观察；当前页面变化不证明游戏可玩或线上更新。

<a id="req-site-discovery-002"></a>
### REQ-SITE-DISCOVERY-002：一次选择及证据性质

Trace ID: O7-REQ-4b01d2d4-ec3a-4dbb-90f8-fca0c4b0a822

- 功能点：F-SITE-DISCOVERY-002；首期分步展示与记录详情；验收 [AC-SITE-DISCOVERY-002](#ac-site-discovery-002)。
- 要求：每个展示必须把目标/授权边界、Agent 解释、实际或说明性结果、下一次选择分开；概念图、说明性受控示例、真实运行记录和小说不得混称。真实记录只展示来源能证明的范围；素材缺失/失效时收窄或停止相关展示，不用生成图补成“实机”。浏览、切换和回放均不产生世界效果。
- 系统设计状态：已覆盖
- 系统设计依据：[DES-SITE-EVIDENCE](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-evidence)，含来源校验、失效与竞态隔离。
- 实现状态：已实现待验证
- 实现与验证依据：[固定实现](https://github.com/eng-cc/oasis7/tree/cde5a650e0aca7cb444b92699cbe5aff4e9ed17d/site)、[浏览器回归方法](../../../scripts/site-discovery-browser.test.cjs)与[静态派生检查](../../../scripts/site-discovery-check.py)；完整验收和未覆盖项由[实施 PR #4697](https://github.com/eng-cc/oasis7/pull/4697)承接。
- 核对日期与基线：2026-10-15；本专题要求；实现 `cde5a650e0aca7cb444b92699cbe5aff4e9ed17d`；本地静态服务器与外部 Google Chrome；本功能 AC 的静态页面范围。
- 剩余范围：可公开真实运行材料的选择与来源核对；真实过程按固定来源版本展示受控 CLI 结果，不将其升级为 GUI、发行安装包或公共世界验收。

<a id="req-site-discovery-003"></a>
### REQ-SITE-DISCOVERY-003：值得回访且可纠正的内容

Trace ID: O7-REQ-2c62937a-f257-4112-a04e-4044662d1b04

- 功能点：F-SITE-DISCOVERY-003；首期观测记录与开发日志；验收 [AC-SITE-DISCOVERY-003](#ac-site-discovery-003)。
- 要求：访客必须能免注册阅读有日期、适用范围和稳定链接的内容；观测解释一件事，日志解释玩家问题、变化及限制。无内容时明确为空，不编造新闻、人数、日期、成功故事或更新频率。撤回内容保留安全的纠正入口，不继续推荐错误摘要；敏感内容不以历史保留为由继续公开。
- 系统设计状态：已覆盖
- 系统设计依据：[DES-SITE-PUBLISHING](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-publishing)，含静态来源、排序、RSS 与撤回。
- 实现状态：已实现待验证
- 实现与验证依据：[固定实现](https://github.com/eng-cc/oasis7/tree/cde5a650e0aca7cb444b92699cbe5aff4e9ed17d/site)、[浏览器回归方法](../../../scripts/site-discovery-browser.test.cjs)与[静态派生检查](../../../scripts/site-discovery-check.py)；完整验收和未覆盖项由[实施 PR #4697](https://github.com/eng-cc/oasis7/pull/4697)承接。
- 核对日期与基线：2026-10-15；本专题要求；实现 `cde5a650e0aca7cb444b92699cbe5aff4e9ed17d`；本地静态服务器与外部 Google Chrome；本功能 AC 的静态页面范围。
- 剩余范围：可公开世界运行记录；本候选提供真实空列表及首篇双语站点开发说明，不承诺更新频率或留存。

<a id="req-site-discovery-004"></a>
### REQ-SITE-DISCOVERY-004：预览选择与安全分流

Trace ID: O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3

- 功能点：F-SITE-DISCOVERY-004；首期速览、下载与 FAQ；验收 [AC-SITE-DISCOVERY-004](#ac-site-discovery-004)。
- 要求：下载前必须能读到适用版本/平台、当前可体验范围、必要环境与 provider 配置、费用责任、签名和数据保留/升级限制。未知值明确未知；无对应资产时不能猜下载地址或暗示支持。关键警示留在对应按钮附近，其余详细说明可渐进展开；保留不安装的选择。静态网站不提供凭据输入框。
- 系统设计状态：已覆盖
- 系统设计依据：[DES-SITE-DOWNLOAD](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-download)，消费现行下载与升级 authority。
- 实现状态：已实现待验证
- 实现与验证依据：[固定实现](https://github.com/eng-cc/oasis7/tree/cde5a650e0aca7cb444b92699cbe5aff4e9ed17d/site)、[浏览器回归方法](../../../scripts/site-discovery-browser.test.cjs)与[静态派生检查](../../../scripts/site-discovery-check.py)；完整验收和未覆盖项由[实施 PR #4697](https://github.com/eng-cc/oasis7/pull/4697)承接。
- 核对日期与基线：2026-10-15；本专题要求；实现 `cde5a650e0aca7cb444b92699cbe5aff4e9ed17d`；本地静态服务器与外部 Google Chrome；本功能 AC 的静态页面范围。
- 剩余范围：真人复述参与成本/安装限制；当前浏览器验证不代签发布资产、安装器或平台兼容性。

<a id="req-site-discovery-005"></a>
### REQ-SITE-DISCOVERY-005：小说与世界记录分离

Trace ID: O7-REQ-ee159d08-7998-44e2-bac5-930b7fa8a0ae

- 功能点：F-SITE-DISCOVERY-005；首期首页小说导读与既有阅读器入口；验收 [AC-SITE-DISCOVERY-005](#ac-site-discovery-005)。
- 要求：小说必须明确为世界观创作，不作为当前玩法证据；首次入口优先简介、开始阅读和无剧透背景，版本清单/总纲置于次级入口。故事加载失败保留真实错误、原文与重试；不存在英文正文时明确中文内容，不伪造翻译或英文路由。不更改小说正文。
- 系统设计状态：已覆盖
- 系统设计依据：[DES-SITE-STORY](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-story)，复用阅读器恢复合同。
- 实现状态：已实现待验证
- 实现与验证依据：[固定实现](https://github.com/eng-cc/oasis7/tree/cde5a650e0aca7cb444b92699cbe5aff4e9ed17d/site)、[浏览器回归方法](../../../scripts/site-discovery-browser.test.cjs)与[静态派生检查](../../../scripts/site-discovery-check.py)；完整验收和未覆盖项由[实施 PR #4697](https://github.com/eng-cc/oasis7/pull/4697)承接。
- 核对日期与基线：2026-10-15；本专题要求；实现 `cde5a650e0aca7cb444b92699cbe5aff4e9ed17d`；本地静态服务器与外部 Google Chrome；本功能 AC 的静态页面范围。
- 剩余范围：更多正文内容与小说质量；中文 reader 不提供英文正文，不将具身叙事变成玩法承诺。

<a id="req-site-discovery-006"></a>
### REQ-SITE-DISCOVERY-006：一致且可恢复的阅读

Trace ID: O7-REQ-de38c7a9-3a48-418d-be60-2c527bc1e8eb

- 功能点：F-SITE-DISCOVERY-006；首期全部受影响公开路由；验收 [AC-SITE-DISCOVERY-006](#ac-site-discovery-006)。
- 要求：CN/EN 必须保留同等主路径、素材类型及公开边界；键盘、窄屏、减少动效和脚本失效时仍能读正文并抵达有效下一步。失效语言目标、媒体或网络请求只影响所属内容，不吞掉页面或显示虚假成功。设计稿不能替代这些入口的浏览器证据。
- 系统设计状态：已覆盖
- 系统设计依据：[DES-SITE-ACCESSIBILITY](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-accessibility)，含响应式、语言与降级验证。
- 实现状态：已实现待验证
- 实现与验证依据：[固定实现](https://github.com/eng-cc/oasis7/tree/cde5a650e0aca7cb444b92699cbe5aff4e9ed17d/site)、[浏览器回归方法](../../../scripts/site-discovery-browser.test.cjs)与[静态派生检查](../../../scripts/site-discovery-check.py)；完整验收和未覆盖项由[实施 PR #4697](https://github.com/eng-cc/oasis7/pull/4697)承接。
- 核对日期与基线：2026-10-15；本专题要求；实现 `cde5a650e0aca7cb444b92699cbe5aff4e9ed17d`；本地静态服务器与外部 Google Chrome；本功能 AC 的静态页面范围。
- 剩余范围：完整无障碍与性能测量；既有桌面/窄屏/键盘/无脚本与故障场景的实际结果只覆盖已记录范围。

<a id="req-site-discovery-007"></a>
### REQ-SITE-DISCOVERY-007：有辨识度且可自主探索的视觉世界

Trace ID: O7-REQ-a4f7d9b0-6c28-4e15-a90d-3b62c5e87401

- 功能点：F-SITE-DISCOVERY-007；CN/EN 首页主视觉及到案例、预览、世界观的连续阅读；验收 [AC-SITE-DISCOVERY-007](#ac-site-discovery-007)。
- 要求：科技感、沉浸感和视觉冲击分别满足[视觉体验目标](#visual-experience-outcomes)。首屏有原创世界的尺度与主焦点；后续图文延续同一视觉身份，以由远及近的节奏把氛围接到可理解的决策，不暗示概念镜头是连续实机。静态版必须成立，动效增强可暂停或关闭，不能阻塞正文、导航、阶段提示或主动作。视觉减配不等于手机只剩文字，窄屏仍应保留主体轮廓和品牌识别。
- 系统设计状态：已覆盖
- 系统设计依据：[沉浸交付](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-immersion)。
- 产品体验依据：[多屏分镜与动效](public-site-discovery-and-showcase.design.md#cinematic-experience)。
- 验证计划：[视觉与动效验证](../../site/github-pages/public-site-discovery-and-showcase.design.md#immersion-verification)；PR #4811仅报告静态构图的局部浏览器回归，完整视觉、访客与性能验收未完成。
- 实现状态：部分实现
- 实现与验证依据：静态观测站构图已交付：双语Hero/工业近景/四步/结果/预览/小说顺序，概念身份、窄屏主体及减少动效样式存在；当前设计首期无持续动效或视频，因此不将未显示暂停按钮认作缺陷。 [静态与条件增强设计](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/site/github-pages/public-site-discovery-and-showcase.design.md#des-site-immersion)；[中文首页构图、概念图与固定案例来源](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/site/index.html#L85-L117)；[英文首页对应结构](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/site/en/index.html#L85-L117)；[观测站局部布局、响应式与减少动效](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/site/assets/styles.css#L3940-L4085)；[双语五视口回归定义](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/site-discovery-browser.test.cjs#L17-L60)；[导航滚动/焦点现行实现](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/site/assets/app.js#L1010-L1034)。[交付PR #4811](https://github.com/nil-studio/oasis7/pull/4811)。PR #4811（head 114ae1ed27b3f7a442aada9732e598004bf0a65f，merge b5476a616dd3d4dbc29f332ee42ed848903c1f60）报告external headed Chrome双语360/390/768/1024/1440及33组回归；本次读取报告与测试定义，未独立重跑或取得全部原始产物，不外推当前main、访客感知、Web Vitals、线上部署。
- 核对日期与基线：2026-10-16；原文档基线 `62e1314368752726e85a050794c8f9b0dcebe123`；本次为设计条款核对，无页面视觉验收。
- 剩余范围：系统设计范围与未覆盖边界沿本条原设计依据；实现与验证：逐条视觉结果、真实访客科技感/沉浸感/记忆点及性能证据仍缺；完整导航/恢复体验待核，不能把静态构图交付升级为全部实现或已验证。按条件增强是否适用逐项核对，不强制新增动效/视频。
- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。
- 适用边界说明：导航观察SITE-NAV-OBS-01已在现有核验范围：当前hashchange仍调用无条件scrollIntoView，键盘焦点可选择平台按钮。它关联连续探索/不阻塞导航的核对，但原精确导航义务主要由004/006承接；不是本次新建缺陷，不以此推断整条007运行失败。


## 5. 验收与证据

<a id="ac-site-discovery-001"></a>
### AC-SITE-DISCOVERY-001：理解玩家位置
- 覆盖要求：REQ-SITE-DISCOVERY-001
给定未接触项目的访客，当阅读首屏及第一段展示后，则能用自己的话区分玩家目标、Agent 决策与世界约束，并找到当前允许的下一步；不能认为已开放公共网页加入。记录误解和原话，不用作者自测代签。30 秒是沿用 site SC-8 的理解目标，不是已实测结果。

<a id="ac-site-discovery-002"></a>
### AC-SITE-DISCOVERY-002：展示不伪造事实
- 覆盖要求：REQ-SITE-DISCOVERY-002
给定概念、说明性、真实记录以及来源缺失四种材料，当浏览与快速切换步骤时，则类型标识与当前内容一致；缺失/失效来源不能进入真实记录陈述，旧响应不覆盖新选择，任何步骤均无世界写请求。人能分清“目标”“结果未知”和“已确认结果”。

<a id="ac-site-discovery-003"></a>
### AC-SITE-DISCOVERY-003：空内容与纠正可理解
- 覆盖要求：REQ-SITE-DISCOVERY-003
给定零条、一条、多条、历史和撤回内容，当进入列表、详情、旧链接或订阅源时，则顺序与日期一致，无假填充；撤回理由和当前说明可达，敏感正文及错误推荐被移除。只记录站点内容行为，不推导游戏活跃或留存。

<a id="ac-site-discovery-004"></a>
### AC-SITE-DISCOVERY-004：预览选择不诱导
- 覆盖要求：REQ-SITE-DISCOVERY-004
给定支持/不支持平台、版本请求超时、缺资产和签名未完成，当访客选择预览时，则看见对应限制与文档/重试/退出路径；不会拼接不存在的资产、绕过信任警示或把未知版本标为最新。安装成本、费用责任和保留语义能被正确复述。

<a id="ac-site-discovery-005"></a>
### AC-SITE-DISCOVERY-005：小说不变成玩法承诺
- 覆盖要求：REQ-SITE-DISCOVERY-005
给定英文首页、无英文小说、章节正常/空/失败与快速切换，当进入导读和阅读器时，则读者知道是小说、知道实际语言、可回首页或重试；不错误跳转，不把具身叙事当当前操作模式，不把迟到章节当当前章节。

<a id="ac-site-discovery-006"></a>
### AC-SITE-DISCOVERY-006：降级仍有完整路径
- 覆盖要求：REQ-SITE-DISCOVERY-006
给定 CN/EN、1440×900、390×844、360×800、200% 缩放、键盘、无 JS 与 reduced-motion，当沿主路径浏览时，则文字、限制、导航、CTA 均可达，没有横向溢出或仅靠颜色传意；媒体失效仍有文字与替代入口。真实浏览器结果按路由和环境分别记录。

<a id="ac-site-discovery-007"></a>
### AC-SITE-DISCOVERY-007：静态有冲击、滚动有连续感、增强可控制
- 覆盖要求：REQ-SITE-DISCOVERY-007
以同一候选的完整首屏、多屏顺序与静态/增强两种呈现分别评估科技感、沉浸感和冲击力，不合成一个掩盖短板的总分。首次看完整静态首屏约 5 秒后，无提示询问记住的主体和世界印象；再连续阅读并询问是否像同一世界、哪里失去兴趣或把氛围误认为实机。5 秒是本项目观察方法，不是注意力研究结论或已测指标；原 30 秒角色理解任务另行执行。

评审用 1/3/5 分锚点描述差异：1 为通用模板或混乱且无主焦点；3 为风格统一但尺度、连续感或记忆点仍平；5 为原创世界识别清晰、远近节奏连贯、静态主画面可记忆且不妨碍理解。2/4 表示中间情况；三维逐项记录理由和下一步，不设虚构提升百分比或固定人数审批。对比旧版与候选时使用同设备、语言和任务并交替顺序，记录未参与项目者的原话；设计者打分不代替访客观察。

同时验证静态、暂停、系统减少动效、媒体失败和窄屏下的主体、文案、状态及动作仍成立；被要求看完开场、无法停下动效、误判公共世界已开放均须修复。截图证明构图，录像证明时序，性能记录证明运行代价，三者不互相代签；没有实际观察则主观效果仍标为待验证。

验证方法由[系统设计验证矩阵](../../site/github-pages/public-site-discovery-and-showcase.design.md#verification)、[沉浸验证](../../site/github-pages/public-site-discovery-and-showcase.design.md#immersion-verification)和现有 [testing-manual](../../../testing-manual.md)承接。自动结构检查不证明人类理解，生成稿不证明浏览器结果，页面合入不证明上线或游戏发行。

<a id="refinement-acceptance"></a>
### 5.1 细化验收：从“页面存在”到“访客能够判断”

下表细化原 AC，不增加审批层、固定访客数量门禁或独立任务身份。实施时覆盖非项目成员及技术兴趣者，分别记录语言、设备、是否被提示、原话和阻塞点；小样本用于发现误解，不外推市场转化率。

| 原 AC | 观察或反例 | 应得到的结果 |
| --- | --- | --- |
| 001 | 先看首屏，再问“你会做什么、AI 做什么、为什么想继续看？” | 能区分角色并说出一个具体兴趣点或不适合自己的理由；不能只复述宏观口号 |
| 002 | 完整阅读四步后，问“哪里受阻、谁调整了什么、实际结果是什么？” | 用同一案例解释因果；未执行选项只能被理解为待考虑方向；日志与生成图不能替代答案 |
| 003 | 从文章直接进入、或决定暂不安装，要求找到后续内容/关注方式；再撤回文章 | 不必回首页重新寻找；RSS、分享摘要与页面纠正一致；无内容不生成假推荐 |
| 004 | 分别寻找安装包和源码案例的准备条件，模拟未知平台或未核验新版本 | 能区分两条路径与费用责任，知道下一步该查什么；未知不能被界面升级为通过 |
| 005 | 从中文与英文入口及分享摘要进入小说 | 在进入前知道是小说和实际语言，读后能返回现有发现路径 |
| 006 | 关闭脚本/动效、破坏媒体，覆盖既有三视口及 768–1024px 中宽 | 核心说服信息、阶段限制与动作仍可读；装饰不制造假控制台，不遮挡警示或焦点 |

首轮记录理解错误、任务是否完成及放弃原因作为基线；后续在相近任务和条件下比较。没有实际观察或合法统计来源时，理解率、下载转化和留存均标为未知；不为了填指标新增采集。

## 6. 取舍与未决问题

采用精选静态展示而非实时世界服务，牺牲即时性以减少首期依赖；类型、日期和范围必须显著。采用一个完整案例而非堆叠宏观场景，牺牲题材覆盖以验证因果可读性。采用现有静态技术栈而非 CMS，接受少量编辑同步成本，不引入运行服务。

素材选择仍待当前证据决定：由 `game_visual_interaction_designer` 与 `qa_engineer` 在实施前选定可公开记录；未取得时只保留明确的说明性示例，不声明真实首产物、续接或真人结果。费用/平台信息由 `liveops_community` 对照当前发行文档核实，未知则明确未知。详细排期与执行事实只进入当前 PR 或后续实施 PR。

<a id="site-design-principles"></a>
## 7. 设计原则与实施顺序

首页以玩家角色、一次有意义的取舍和当前参与条件组织内容。观看、尝试、深读和持续关注分别提供有效出口；技术入口服务有需要的读者，不成为普通访客理解游戏的前置条件。

视觉表达采用“英雄主画面建立规模 → 同世界近景连接玩家 → 一次决策形成理解 → 安静阅读区消化 → 远景与小说留下余韵”。这是一条连接既有内容的视觉叙事，不要求新增固定数量的栏目或素材，不把概念镜头接续表达为连续实机。

价值表达、同案例叙事、预览选择与主视觉/多屏节奏在同一轮设计中并行确定，不能把视觉无限推迟为上线后的装饰。静态构图成立后再落实轻量增强；视频、媒体包和实时观测不是前置条件。没有合适素材时改善文字与已存在的入口，不扩张功能来填充版式。

公开素材须原创或具有适用许可，保留来源与素材性质。生成设计稿只表达方案方向，不能替代游戏运行、页面实现或验收证据。

## 专业 owner、authority 与测试层级追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-SITE-DISCOVERY-001](#req-site-discovery-001) / [AC-SITE-DISCOVERY-001](#ac-site-discovery-001) | `viewer_engineer` / `qa_engineer` | [site authority](../../site/design.md#homepage-layout-authority) | [系统验证计划](../../site/github-pages/public-site-discovery-and-showcase.design.md#verification)，实现后取得同候选结果；本 PR 只验证文档 | `test_tier_required`；页面实现另按 S6 选择真实浏览器范围 |
| [REQ-SITE-DISCOVERY-002](#req-site-discovery-002) / [AC-SITE-DISCOVERY-002](#ac-site-discovery-002) | `viewer_engineer` / `qa_engineer` | [site authority](../../site/design.md#homepage-layout-authority) | [系统验证计划](../../site/github-pages/public-site-discovery-and-showcase.design.md#verification)，实现后取得同候选结果；本 PR 只验证文档 | `test_tier_required`；页面实现另按 S6 选择真实浏览器范围 |
| [REQ-SITE-DISCOVERY-003](#req-site-discovery-003) / [AC-SITE-DISCOVERY-003](#ac-site-discovery-003) | `viewer_engineer` / `qa_engineer` | [site authority](../../site/design.md#homepage-layout-authority) | [系统验证计划](../../site/github-pages/public-site-discovery-and-showcase.design.md#verification)，实现后取得同候选结果；本 PR 只验证文档 | `test_tier_required`；页面实现另按 S6 选择真实浏览器范围 |
| [REQ-SITE-DISCOVERY-004](#req-site-discovery-004) / [AC-SITE-DISCOVERY-004](#ac-site-discovery-004) | `viewer_engineer` / `qa_engineer` | [site authority](../../site/design.md#homepage-layout-authority) | [系统验证计划](../../site/github-pages/public-site-discovery-and-showcase.design.md#verification)，实现后取得同候选结果；本 PR 只验证文档 | `test_tier_required`；页面实现另按 S6 选择真实浏览器范围 |
| [REQ-SITE-DISCOVERY-005](#req-site-discovery-005) / [AC-SITE-DISCOVERY-005](#ac-site-discovery-005) | `viewer_engineer` / `qa_engineer` | [site authority](../../site/design.md#homepage-layout-authority) | [系统验证计划](../../site/github-pages/public-site-discovery-and-showcase.design.md#verification)，实现后取得同候选结果；本 PR 只验证文档 | `test_tier_required`；页面实现另按 S6 选择真实浏览器范围 |
| [REQ-SITE-DISCOVERY-006](#req-site-discovery-006) / [AC-SITE-DISCOVERY-006](#ac-site-discovery-006) | `viewer_engineer` / `qa_engineer` | [site authority](../../site/design.md#homepage-layout-authority) | [系统验证计划](../../site/github-pages/public-site-discovery-and-showcase.design.md#verification)，实现后取得同候选结果；本 PR 只验证文档 | `test_tier_required`；页面实现另按 S6 选择真实浏览器范围 |
| [REQ-SITE-DISCOVERY-007](#req-site-discovery-007) / [AC-SITE-DISCOVERY-007](#ac-site-discovery-007) | `game_visual_interaction_designer` / `viewer_engineer` / `qa_engineer` | [沉浸交付](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-immersion) | [视觉与动效验证](../../site/github-pages/public-site-discovery-and-showcase.design.md#immersion-verification)，静态构图已交付，PR #4811报告局部浏览器记录；完整同候选多屏、适用录像、性能与访客记录仍待核实 | `test_tier_required`；页面实现沿现有 S6，不新增工作流 |
