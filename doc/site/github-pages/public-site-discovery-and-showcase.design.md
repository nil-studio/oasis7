# 官网发现与展示系统设计

- 状态：active（当前静态实现合同及待核实细化；不表示部署或完整验收）
- Owner role：viewer_engineer
- 文档审读基线：2026-10-16；`a8c7c97984839d32d6dac6013b16b307735482d2`；远程文档与首页源码阅读，无线上浏览器结论
- 视觉补充基线：`62e1314368752726e85a050794c8f9b0dcebe123`；设计规格核对，无页面截图或动效实测
- 历史设计起点：2026-10-14；`116c2e6d4f4891c316b305764a1f755a3005d3ae`
- 上游产品：[`官网发现、玩法展示与持续关注 PRD`](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md)
- 产品体验设计：[`配对产品 design`](../../product/player-entry-distribution/public-site-discovery-and-showcase.design.md)
- 专业入口：[`site PRD`](../prd.md)、[`当前布局与页面覆盖合同`](../design.md#homepage-layout-authority)、[`文件级索引`](../prd.index.md)

<a id="paired-document-navigation"></a>
## 配套文档与阅读顺序

这三份文档属于同一官网专题，按产品与专业实现职责分别维护；以下导航只聚合入口，不复制正文。

| 产品需求 | 产品体验与视觉设计（设计师首读） | 系统实现设计（开发首读） |
| --- | --- | --- |
| [目标、范围与验收](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md) | [文案、布局、素材、分镜与交付物](../../product/player-entry-distribution/public-site-discovery-and-showcase.design.md) | [结构、状态、资源与验证](public-site-discovery-and-showcase.design.md) |

设计师从产品体验与视觉设计开始，再按需核对产品要求；开发者从系统实现设计开始，并回查上游体验。文件位置不改变上述职责，文档存在不表示实现或验收完成。

## 1. 问题、目标与非目标

现有首页有氛围图、说明性危机场景、下载与文档入口。目标是在同一静态技术栈上形成具体决策展示、可公开的观测与开发进展、准确预览分流和可恢复阅读，而不是引入游戏服务依赖。

本设计补充现行 root design，不替代其权限、双语、可访问性、下载、手册和故事 reader 合同。保留首屏一个资源压力问题、一个主动作、一个次要文字链接和可见技术预览状态；通过后续区段承担具体玩法，不恢复三列事实条。后续实现若反转 root 约束，须同步 root，而非以新文件日期静默覆盖。

静态实现沿本设计维护首页、内容列表/详情、RSS、下载增强及小说恢复；实现来源见上游产品功能状态。生成式概念稿继续只承担视觉方向。本文不升级发行状态、游戏规则或运行服务，也不以页面代码存在代替真人理解、完整浏览器验收或线上部署。

本设计承接[官网设计原则](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#site-design-principles)，细化产品传播与静态体验；[第 13 节](#static-experience-refinement)定义具体承接和补充验证。新细化不继承旧实现的验证结论，状态以[PRD 的范围分离](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#discovery-refinement-scope)及[新增视觉要求](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-007)为准。

## 2. 上游约束与相关角色

产品 owner 决定访客价值和范围；视觉 owner 决定信息层级与素材；viewer_engineer 承担静态实现；liveops_community 核对可公开内容与当前口径；qa_engineer 核对来源和验证范围。本文不要求固定 subagent 数量或新增审批流程。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-4544302c-40be-4aa3-a557-21fe6f475c19；[REQ-SITE-DISCOVERY-001](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-001) | 首页解释玩家收益、Agent、资源压力及有效下一步 | [DES-SITE-ENTRY](#des-site-entry) | 产品/视觉；root 布局 | 不证明真人理解或游戏可玩 |
| Trace refs: O7-REQ-4544302c-40be-4aa3-a557-21fe6f475c19；[REQ-SITE-DISCOVERY-001](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-001) | 各决策位置有真实主次动作与适合读者的下一步 | [价值路径](#des-site-value-path) | 产品/视觉；root 布局 | 不证明真人理解或游戏可玩 |
| Trace refs: O7-REQ-4b01d2d4-ec3a-4dbb-90f8-fca0c4b0a822；[REQ-SITE-DISCOVERY-002](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-002) | 同一案例的四步与素材性质一致；来源缺失不补假事实 | [DES-SITE-EVIDENCE](#des-site-evidence) | QA/玩法 owner；可公开原始材料 | 不接实时世界写/读服务 |
| Trace refs: O7-REQ-4b01d2d4-ec3a-4dbb-90f8-fca0c4b0a822；[REQ-SITE-DISCOVERY-002](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-002) | 四步语义、行动主体及结果共用同一案例身份与来源 | [案例语义](#des-site-value-path) | QA/玩法 owner；可公开原始材料 | 不接实时世界写/读服务 |
| Trace refs: O7-REQ-2c62937a-f257-4112-a04e-4044662d1b04；[REQ-SITE-DISCOVERY-003](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-003) | 稳定列表/详情/RSS、回访入口与纠正，分享不丢范围 | [DES-SITE-PUBLISHING](#des-site-publishing) | 内容 owner；公开口径 authority | 不做 CMS、订阅账户或投稿 |
| Trace refs: O7-REQ-2c62937a-f257-4112-a04e-4044662d1b04；[REQ-SITE-DISCOVERY-003](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-003) | 文章直达、继续阅读及分享摘要与撤回状态一致 | [回访与分享](#des-site-return-sharing) | 内容 owner；公开口径 authority | 不做 CMS、订阅账户或投稿 |
| Trace refs: O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3；[REQ-SITE-DISCOVERY-004](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-004) | 版本、平台、成本及限制一致；下载与源码案例分流 | [DES-SITE-DOWNLOAD](#des-site-download) | 发行/Launcher；root 下载合同 | 不改安装器、签名或资产校验机制 |
| Trace refs: O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3；[REQ-SITE-DISCOVERY-004](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-004) | 下载候选与源码案例的条件、来源和已验证范围分离 | [预览适配](#des-site-preview-fit) | 发行/Launcher；root 下载合同 | 不改安装器、签名或资产校验机制 |
| Trace refs: O7-REQ-ee159d08-7998-44e2-bac5-930b7fa8a0ae；[REQ-SITE-DISCOVERY-005](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-005) | 小说导读与真实记录分离，失败/语言目标及分享身份正确 | [DES-SITE-STORY](#des-site-story) | 小说 owner；现有 reader 合同 | 不翻译或重写正文 |
| Trace refs: O7-REQ-ee159d08-7998-44e2-bac5-930b7fa8a0ae；[REQ-SITE-DISCOVERY-005](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-005) | 小说分享保留虚构性质与实际语言，并提供有效回链 | [回访与分享](#des-site-return-sharing) | 小说 owner；现有 reader 合同 | 不翻译或重写正文 |
| Trace refs: O7-REQ-de38c7a9-3a48-418d-be60-2c527bc1e8eb；[REQ-SITE-DISCOVERY-006](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-006) | 双语、无 JS、窄屏/中宽/键盘/动效降级不丢主路径 | [DES-SITE-ACCESSIBILITY](#des-site-accessibility) | 视觉/QA；现有 S6 | 概念稿不代签浏览器/性能证据 |
| Trace refs: O7-REQ-de38c7a9-3a48-418d-be60-2c527bc1e8eb；[REQ-SITE-DISCOVERY-006](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-006) | 页面局部样式、媒体预算及中宽适配保持可读性 | [视觉交付](#des-site-visual-delivery) | 视觉/QA；现有 S6 | 概念稿不代签浏览器/性能证据 |
| Trace refs: O7-REQ-a4f7d9b0-6c28-4e15-a90d-3b62c5e87401；[REQ-SITE-DISCOVERY-007](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-007) | 原创静态主视觉、多屏连续感与可控制的轻量增强；效果及代价分别验证 | [沉浸交付](#des-site-immersion) | 视觉/产品/QA；原创素材及同候选浏览器 | 不保证主观转化、不新增 3D 引擎或游戏控制、不把概念接续当实机 |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[REQ-ENTRY-CANDIDATE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001) | 版本、日期、Release 说明、实际资产与 checksum 以同一候选快照原子更新；失败不混版 | [已有设计](#des-site-download) | 发行/QA/LiveOps 与对应产品 owner | 两 primary mode、跨域 QA/经济/恢复及完整 release gate 不由网站下载片段覆盖 |
| Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff；[REQ-ENTRY-CLAIM-001](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001) | runtime_capture 绑定版本/场景/证据窗口与可公开来源；来源不足不背书运行事实 | [已有设计](#des-site-evidence) | 发行/QA/LiveOps 与对应产品 owner | 全渠道专业事实、QA verdict 和 LiveOps 审核闭包仍外部 |
| Trace refs: O7-REQ-f71ac2e9-1586-44af-89b7-e98ff364e70c；[REQ-ENTRY-CLAIM-002](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-002) | 同 URL 纠正/撤回同步 CN/EN、详情/摘要/RSS，回滚不复活错误正文 | [已有设计](#des-site-publishing) | 发行/QA/LiveOps 与对应产品 owner | 其他 active public surfaces、自动漂移检测与第三方缓存收回不在本设计 |
| Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385；[REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003) | concept/illustrative/runtime_capture 与小说类型分离，目标/概念不冒充真实运行来源 | [已有设计](#des-site-evidence) | 发行/QA/LiveOps 与对应产品 owner | 其他专业领域、非官网渠道及全部公开审核仍外部 |

现行[公开事实四层模型](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#public-claim-four-layer-model)和[下载完整性/升级](../../product/player-entry-distribution/prd.md#download-integrity-and-upgrade)被消费，不在本设计重新定义。

## 3. 历史设计起点与现行状态入口

下表保留 2026-10-14 的问题基线，不是当前待办清单。既有静态实现及其未验证范围读取[产品功能状态](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-001)；本轮细化读取[单独范围](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#discovery-refinement-scope)，不把历史缺口、新设计与已实现能力混列为同一当前状态。

| 对象/能力 | 当前状态（历史基线） | 目标状态 | 差距/假设 | 证据或 owner |
| --- | --- | --- | --- | --- |
| 首页 | 静态 CN/EN HTML、共享 CSS/JS、场景切换与示例日志 | 角色优先、同一案例四步、清楚分流 | 需要内容重排与真实访客验证 | [固定首页源码](https://github.com/eng-cc/oasis7/blob/116c2e6d4f4891c316b305764a1f755a3005d3ae/site/index.html) |
| 记录/日志 | 首页链接 Release/文档；本专题独立内容链未核实 | 静态列表/详情、三条精选、纠正与 RSS | 需要可公开首篇内容，不用占位成功填充 | liveops_community |
| 下载 | HTML 静态回退与 JS 最新 Release 刷新并存 | 速览和版本/资产一致处理 | 不能只凭静态旧日期判断运行显示过期 | [固定 app.js](https://github.com/eng-cc/oasis7/blob/116c2e6d4f4891c316b305764a1f755a3005d3ae/site/assets/app.js) |
| 小说 | 中文阅读器与版本资料导航已存在 | 导读优先，语言/失败遵守现有合同 | 本文不宣布 reader 恢复缺陷已修复 | [reader 设计](../design.md#story-reader-design) |
| 视觉证据 | 既有生成式概念板 | 后续按路由/状态取得真实浏览器证据 | 图稿不证明实现或页面覆盖 | 视觉/QA |

## 4. 边界与结构

<a id="des-site-entry"></a>
### DES-SITE-ENTRY：页面与内容所有权

沿用 `site/index.html`、`site/en/index.html`、`site/assets/styles.css`、`site/assets/app.js`，无必需的 React、CMS、SSG、服务端渲染或新框架构建链。正文、关键状态、链接在 HTML 内，JS 仅增强。

```text
README / 现行产品与专业文档 / 已复核可公开原始材料
    -> 人工编辑的 canonical 内容详情 HTML
    -> 首页精选 / 列表摘要 / RSS（派生摘要，同一变更校对）
    -> GitHub Pages 静态发布
    -> 浏览器阅读与渐进增强（无世界效果）

Release 元数据 -> 现有下载刷新层 -> 同候选版本/资产展示
小说原文 -> 现有 reader -> 真实加载状态与恢复入口
```

箭头表示内容读取/派生，不是世界提交。Pages 不拥有世界时间线、Agent 授权或玩家账户。所有官网交互只改变阅读状态；无模型调用、钱包连接和世界写接口。

| 路由族（目标） | 中文 | 英文 | 内容与兼容 |
| --- | --- | --- | --- |
| 首页 | `/` | `/en/` | 同构主路径；相对 URL 支持项目子路径 |
| 观测列表/详情 | `/observations/`、`/observations/<slug>/` | `/en/observations/`、`/en/observations/<slug>/` | 独立稳定 slug；可只有空列表，不虚构文章 |
| 开发日志列表/详情 | `/devlog/`、`/devlog/<slug>/` | `/en/devlog/`、`/en/devlog/<slug>/` | 玩家视角文章，不自动搬 PR 标题 |
| 订阅 | `/feed.xml` | `/en/feed.xml` | 静态 XML，无登录/数据收集 |
| 小说 | 既有 `/story/` | 不新增虚假英文页 | 英文首页链接明确标为中文小说 |

四个主导航对应玩法、开发进展、世界观、技术预览，按当前已有内容分配；空记录保留 URL 与页尾入口，旧 #world 锚点指向页尾，不为其保留首页区块或详情主导航。世界观链接打开明确标为小说的导读，文档/GitHub 放页尾，遵守 site PRD 的 AC-19。旧锚点 `#proof`、`#world`、`#download`、`#contribute`、`#world-laws` 在新语义区域保留唯一 ID，旧分享链接不失效。详情返回链接不依赖 history 或来源页状态。

## 5. 关键运行流程

<a id="des-site-evidence"></a>
### DES-SITE-EVIDENCE：分步展示与来源

进入页面先显示类型、标题、范围和文字；分步内容预写 HTML，不为切换步骤新增运行服务请求。四步为设定目标、遇到阻塞、调整行动、核对结果，主线直接使用同一真实受控案例。四步在桌面横排、窄屏纵排，始终同时可读；步骤标题为静态文字，不创建仅切换高亮的按钮；无 JS 时同样保留完整顺序及步骤标题。真实测试摘要另行固定来源版本、运行日期和范围，不与说明性 sample 日志或历史截图混为同一次运行。

图片保持独立 alt、caption 和完整原图入口。加载失败只替换该媒体节点的状态，不删文字/标签。若使用异步大图 overlay，以当前 record ID + media ID 校验提交，迟到 load/error 不覆盖新选择；Esc 关闭并将焦点还给触发控件。无此能力时直接打开原图，不新造复杂 lightbox。

来源类型固定为 `concept`（概念）、`illustrative`（说明性）、`runtime_capture`（真实运行）；小说与日志使用内容类型区分，不作为运行来源类型。标签没有“live”。运行来源缺少必要版本/场景/证据关系时不得标 runtime_capture；可以明确收窄为原本就属于说明性的材料，但不能把失效真实记录偷偷改成未说明的“示例”。

原始 Agent 自由文本一律视为不可信内容，只以转义后的文字呈现；不得作为 HTML、脚本、页面跳转或模型指令执行。脱敏后仍需能理解因果，不能把关键失败截掉只留成功。

<a id="des-site-publishing"></a>
### DES-SITE-PUBLISHING：编辑、发布与纠正

每条内容的详情 HTML 是公开正文与元数据的唯一编辑源。发布正文放在唯一 `data-content-body` 区域；撤回必须移除此区域并提供 `data-content-correction`，同 GUID 的 RSS 描述与纠正正文一致；关联的 CN/EN 内容必须在同一变更中撤回并同步状态，不能让另一语言继续发布旧正文。首页/列表/RSS 保留必要摘要，作为派生副本，在同一变更中同步并由检查核对 id、标题、类型、日期、语言、版本、链接及状态；不在 PRD 中另存文章台账。静态派生一致性检查消费 `scripts/site-discovery-check.py`，负例覆盖 `scripts/site-discovery-check.test.py`；当前不引入生成器依赖，未来批量化再单独决定。

发布顺序为核对上游事实与可公开性、编写详情、同步 CN/EN 与摘要、运行适用检查、沿现有 Pages 发布链部署。文档通过、合入和部署是不同事实。

列表按 `published_at` 倒序，同时间以 slug 升序；首页最多三条。编辑更正更新 `updated_at` 但不伪造原发布时间。draft 不进入公开页面/RSS；历史已发布文章保留原版本并明确不代表当前可用性。撤回后同 URL 改为安全说明，移除错误正文和推荐；RSS 同 GUID 更新为纠正说明。客户端已缓存或第三方转发内容无法被承诺收回。

敏感材料必须移除正文、图片和派生摘要；必要仓库历史/缓存处置依现有安全与事件流程，由对应 owner 承担，不用静态 tombstone 假装已经彻底删除泄露。

<a id="des-site-download"></a>
### DES-SITE-DOWNLOAD：速览与同候选展示

预览速览消费 README、对应版本发布说明与现有安装/升级 authority。字段为可体验范围、平台/架构、配置要求、编程要求、模型来源/费用责任、公开网页状态、保存/升级/签名限制。未知字段明确未知，不填写估算 token 花费或未经验证的支持矩阵。

保留每平台一个推荐主包和手动切换。平台检测失败只显示选择器与范围说明，不把未知设备判为已支持。API 刷新必须把版本、日期、发布说明、该版本实际存在的目标资产及 checksum 作为一个候选快照核对并一次性更新；不混用新版 tag 与旧版文件/校验项，不仅按旧 filename 拼接新链接。找不到资产时停止该项下载推荐并给发行说明。

刷新失败可以保留完整的静态已知候选，但必须明确“最后记录版本，未确认最新”；静态无 JS 入口同样不得自称在线已验证最新。无有效候选时只保留发行说明/恢复入口。checksums 不是平台签名，不提供绕过系统策略的步骤；本文不改变 [root 手动升级首写合同](../design.md#manual-upgrade-first-write)。

#### 预览层级与深链定位

承接[产品预览阅读顺序](../../product/player-entry-distribution/public-site-discovery-and-showcase.design.md#preview-choice-hierarchy)及 REQ-SITE-DISCOVERY-004；Trace refs: O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3。复用现有静态 HTML、原生 details、平台选择器与候选快照，不增加第二份版本数据源。

`#download` 定位预览标题和常显摘要，并在增强可用时展开预览说明；`#package-download` 定位平台选择，并先展开其祖先 disclosure。先确定目标和展开状态，再完成定位；布局稳定后目标标题或平台选择须进入可见视口，不能仅凭 URL 已改变或 `open=true` 判成功。若顶栏覆盖内容，使用适用的滚动留白，不能遮住目标。

首次带 hash 加载、页内主次入口、同 hash 重复点击，以及前进/后退到目标 hash，均消费同一目标映射。重复点击应能重新展开并定位，不依赖再次触发 hashchange；普通手动滚动与关闭 disclosure 后，不因迟到事件、图片载入或 Release 刷新再次展开、跳回顶部或抢走阅读位置。历史导航尊重浏览器恢复；仅在目标仍被折叠或遮挡时做必要修正，不新增历史记录。保留修饰键点击和新标签页等原生链接语义。

键盘激活后，后续焦点顺序应能继续进入目标说明和控件；需转移焦点时只落到目标标题或原生 summary，不自动触发下载。无 JS 时保留标题、原生展开入口和有效 href；减少动效时使用即时定位，不以平滑滚动作为可达性前提。

页面回归须在 CN/EN、桌面与窄屏覆盖首次深链、页内点击、同 hash 关闭后重入、前进/后退，以及展开后的图片/候选更新；另核对无 JS 与 reduced-motion。验证展开状态之外，还应在布局稳定后检查目标的视口位置及顶栏遮挡，并实际操作下一控件；不能以自动点击工具先将元素滚入视口后的截图代替链接落点验证。分别记录关键风险可见、候选与案例版本区分及不安装出口；这些页面结果不代签安装或案例复现。

<a id="des-site-story"></a>
### DES-SITE-STORY：小说入口与 reader 复用

首页世界观导读与页尾提供小说名、无剧透摘要、世界观小说标签与开始阅读；不新增正文副本。故事页首层优先简介/开始阅读/背景，已有版本说明、总纲和正文索引保留可达但降为次级。

沿用 [reader 状态机](../design.md#story-reader-design)：loading/ready/empty/error、requestId 防迟到、明确重试与原文入口。当前请求身份核对先于正文或缓存写入，迟到的旧请求不能污染显式重试后的缓存。语言跳转仅消费存在的 allowlist 映射；`/story/` 不能按路径猜为 `/story/en/`。存储不可用时不阻断阅读，不自动改写小说语言。小说中的具身描述不生成任何游戏能力标记。

## 6. 接口与数据合同

这些字段是目标静态内容约定，不是游戏协议、运行回执或新增数据库。可用 HTML `data-*` / `time` / 语义标签承载，不要求浏览器执行数据构建。

| 内容字段 | 生产者 → 消费者 | 身份/顺序 | 成功与错误 | 兼容/大小边界 |
| --- | --- | --- | --- | --- |
| `id, slug, locale, translation_of` | 内容维护者 → 详情/摘要/RSS | slug 发布后不复用；语言映射显式 | 缺译文说明实际语言；非法路径拒绝 | slug 仅小写 ASCII、数字和连字符；禁止路径穿越 |
| `kind, provenance, status` | 内容 owner/QA → 标签/筛选 | kind=observation/devlog/lore；status=draft/published/withdrawn；provenance 仅观测必需 | 未知枚举、互斥组合在发布检查失败 | 不映射成游戏 readiness；无 live 来源 |
| `title, summary, published_at, updated_at` | 详情 → 首页/列表/RSS | ISO 8601 含时区；发布时间排序 | 缺日期不伪造；空正文不发布成文章 | title 建议 ≤80 字符、summary ≤180；超过先编辑，不隐藏事实 |
| `version, scenario, capture_window, evidence_url` | 原始材料/QA → runtime_capture 详情 | 真实运行必须有版本、场景、窗口及可公开来源关联 | 缺失/失效来源不能背书运行事实 | evidence_url 只允许经复核公开 HTTPS 或站内相对路径 |
| `steps, media, alt, caption` | 编辑者 → 分步展示/浏览器 | 四步属于同 record；media 绑定所属步骤 | 媒体失败留文字，快速切换不串条目 | 图片有尺寸、图注、来源；不嵌不可信 HTML |
| 版本/资产快照 | 现有 GitHub Release 读取 → 下载区 | 同一个合法 release 与实际 assets 集合 | 超时/缺资产/不支持均有独立提示 | 沿用现有接口；仅 HTTPS/允许的仓库域名 |

静态内容检查须拒绝错误 id、重复 slug、失效站内链接、语言配对漂移和不完整 runtime_capture 元数据；形式通过不证明原始材料真实。来源复核保留人工判断。

## 7. 状态、事务与持久化

官网没有世界事务。浏览/切步骤不提交 Agent 意图、不消费资源、不连接私有 provider。阅读状态可保留在 URL 锚点；现有语言偏好存储保持可失败，不新增账户或持久个人画像。

| 事件 | 当前视图/状态 | 处理及恢复边界 |
| --- | --- | --- |
| 首次/无 JS | 静态内容可读 | 直接锚点/链接；不存在必需的 loading 遮罩 |
| 若详情存在步骤/媒体切换 | 当前 record/step | 同步设置面板及可访问状态；不加载另一候选来补失败；首页四步始终可读 |
| 媒体加载错误 | 内容仍有效、媒体 error | 显示替代文字与原文；不切换素材类型 |
| 撤回内容 | withdrawn | 安全纠正页、当前入口；不展示旧正文/错误推荐 |
| Release 刷新成功 | verified metadata snapshot | 元数据/资产一起替换；仅代表来源元数据，不证明游戏验收 |
| Release 刷新失败 | last-known / unavailable | 保留完整旧候选并标记未确认最新，或仅说明入口 |
| reader 迟到结果 | 与当前 requestId 不符 | 忽略；不修改当前正文/错误或 busy 状态 |

内容纠正涉及正文、摘要、语言和 RSS 的成套静态变更；静态发布不是分布式事务，部署期间旧缓存可能存在。因此每份内容都自带日期/版本/状态，不依赖首页提示来限制历史文章。

## 8. 部署、安全与运行约束

继续现有 GitHub Pages 发布链，不新增特权 workflow、Secrets、后台服务或审批层。新内容放 `site/**` 对应语言路径，设计资产只放 `doc/site/github-pages/assets/`，不自动复制到公开首页作为游戏素材。

素材在进入公开 Git 历史前脱敏；不得包含 API key、session、私有路径、非公开账号、玩家私信或部署拓扑。外部链接限定 HTTPS 并有明确去向；富文本按站点现有安全策略处理，禁止 `javascript:`、事件属性和用户脚本。第三方视频只在明确点击后加载，并保留 poster/文字；首期优先站内轻量图片，不依赖外部服务完成阅读。

不新增行为追踪 SDK、邮件/手机号表单或隐形采集。浏览器不得获得世界写接口、管理 token 或供应商凭据。公开素材使用原创/有许可资源，并保留适用的来源信息。

## 9. 质量与容量

<a id="des-site-accessibility"></a>
### DES-SITE-ACCESSIBILITY：响应式与可访问性

桌面容器沿用现有最大宽度约 1160px，正文以 16px 为基准、长文控制行宽；首屏标题桌面约 48–64px、手机约 32–40px，以真实中英换行检查为准。手机单列重排，不以缩小整页取代响应式。保持现有深色/薄荷绿视觉，琥珀仅用于风险或资源压力；状态必须有文字。

实现检查目标：普通文字对比度至少 4.5:1，大字/关键非文本控件至少 3:1；触控目标以 44×44 CSS px 为设计目标。键盘焦点明显、跳转正文可用、展开信息可键盘访问；用原生链接和按钮，不做仅 hover 信息。200% 缩放不丢正文或操作。

保留 `1440x900`、`390x844`、`360x800` 的真实浏览器证据；CN/EN 各核对阅读顺序、长标签换行、无横向溢出。无 JS 时菜单/正文/CTA 可用；reduced-motion 下不启动 Hero Canvas、自动计数或强制滚动。语言 allowlist、query/hash 和部署前缀保留，不猜站点根域。

首期容量预算是设计目标而非已测结果：首屏必要图片总传输量尽量 ≤500KB，非首屏图片 lazy-load；不自动下载视频，不新增必需字体或大 JS 库。图片声明宽高、适配尺寸；在现有 S6 性能路径记录请求量/总字节/LCP/布局偏移，未测时不写性能 PASS。若真实截图压缩影响文字可读性，优先缩略图 + 按需原图，不牺牲证据清晰度。

### 设计资产归档

设计图文件：[public-site-discovery-and-showcase.webp](assets/public-site-discovery-and-showcase.webp)。既有概念板由 ChatGPT 原生生图生成；保存压缩展示副本，未以脚本绘图冒充生图。图内全景/控件均属概念表达，原始生成图与提交的展示副本可以不同编码/尺寸。实际文件字节、尺寸、hash 以原资产核对和 Git 对象为准，不填写未知模型版本/seed。本轮文档细化没有重新生成该资产。

图稿负责视觉方向，配对产品 design 负责文案与体验，本系统设计负责实现和验证。图中与正文不一致的细节不自动成为要求；后续 HTML 和浏览器证据独立取得。生图样例不进入 runtime_capture 素材集合。

## 10. 兼容、迁移与回滚

保留既有首页锚点、CN/EN、docs、story 和 download 路由，不移动世界/手册 authority。新增路由相对链接支持自定义域与项目子路径；文章发布后 slug 不重用，迁移保留旧地址到明确新地址的静态说明/链接。

后续实现先维护内容和既有 claim gate 的语义，再重排 HTML，最后补增强脚本与回归。若检查依赖旧精确文案，应同时更新对应测试，使其继续检查公开状态、素材性质、真实 CTA 和下载边界，而不是删检查或用隐藏文本满足旧断言。本设计不要求删减或跳过任何门禁。

站点回滚只回退到已知静态提交，不回滚游戏、世界历史或安装器状态。已撤回/纠正的敏感或错误内容不能随整站回滚复活；必要时在回退提交上前向保留纠正。RSS 和第三方缓存可能保留旧副本，应公开纠正而非声称已全网删除。

## 11. 验证设计与可追溯性

<a id="verification"></a>
下表是验证方法与证据目标，不是执行结果。沿用 [testing-manual](../../../testing-manual.md) 的 Site/S6 和现有站点脚本；操作方法见[官网发现验证手册](public-site-discovery-validation.manual.md)。静态检查与故障回归分别由现有站点门禁和 `scripts/site-discovery-browser.test.cjs` 承担；实际运行结果只在实施 PR 记录。本轮新增细化另按[补充验证](#refinement-verification)覆盖；007 按[沉浸验证](#immersion-verification)覆盖，不以既有脚本存在宣称新断言已实现。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-SITE-DISCOVERY-001](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-001) / AC-SITE-DISCOVERY-001 | [ENTRY](#des-site-entry) | 主路径/旧锚点/CNEN可达，玩家位置可理解 | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-01：同候选静态服务器，点开主次 CTA 和五个旧锚点；另做陌生访客观察并记录原话 | 实施 PR：浏览器/访客记录，绑定 source HEAD、语言和入口 | 自动化不能证明理解或可玩 |
| [REQ-SITE-DISCOVERY-002](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-002) / AC-SITE-DISCOVERY-002 | [EVIDENCE](#des-site-evidence) | 类型/内容同属一案例，来源错误不升级，切换无世界写 | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-02：concept/illustrative/runtime_capture/缺来源样本；破坏图片、快速切步骤、延迟旧媒体；核对网络无世界/模型调用 | 实施 PR：fixture、断言、脱敏来源审查、当前版本截图 | fixture 不证明素材真实或运行结果 |
| [REQ-SITE-DISCOVERY-003](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-003) / AC-SITE-DISCOVERY-003 | [PUBLISHING](#des-site-publishing) | 零/一/多/同时间排序、双语更正、撤回旧 URL/RSS | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-03：静态 fixture 和 XML 解析；核对 id/日期/范围/状态，撤回后详情/首页/列表/RSS同步 | 实施 PR：内容检查输出与页面证据 | 不证明外部缓存清除或活跃留存 |
| [REQ-SITE-DISCOVERY-004](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-004) / AC-SITE-DISCOVERY-004 | [DOWNLOAD](#des-site-download) | 版本与实际资产同候选，不支持/超时有安全出口 | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-04：模拟 Release 成功/超时/缺asset/缺checksum/错误平台；核对无混版链接、last-known不称最新、无凭据框 | 实施 PR：同候选 mock API 与浏览器记录；实际资产另核实 | 不证明安装器安全、签名或发行就绪 |
| [REQ-SITE-DISCOVERY-005](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-005) / AC-SITE-DISCOVERY-005 | [STORY](#des-site-story) | 小说/运行记录区分，语言目标与 reader 恢复有效 | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-05：英文首访到中文小说；正常/空/404/超时/A→B迟到成功与失败；复用 root reader 用例 | 实施 PR：准确路由、请求身份与当前状态截图 | 不证明小说质量或具身玩法 |
| [REQ-SITE-DISCOVERY-006](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-006) / AC-SITE-DISCOVERY-006 | [ACCESSIBILITY](#des-site-accessibility) | CN/EN/三视口/键盘/缩放/无JS/reduced-motion可用 | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-06：同候选按上述矩阵验证首页、两类列表/详情、小说入口；对比度与资源加载单独记录 | 实施 PR：真实浏览器截图、键盘路径、网络/性能结果 | 概念图不证明像素/无障碍/性能达标 |
| [REQ-SITE-DISCOVERY-001](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-001) / AC-SITE-DISCOVERY-001 | [价值路径](#des-site-value-path) | 各决策位置有真实主次动作与适合读者的下一步 | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-01 的细化场景：同候选静态服务器，验证首页/案例/预览/文章直达的主次链接、旧锚点和项目子路径；关闭 JS 后重复任务，另记录访客原话 | 实施 PR：source HEAD、语言、入口、fixture 与实际结果；未执行项如实保留 | 自动化不证明理解或吸引力 |
| [REQ-SITE-DISCOVERY-002](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-002) / AC-SITE-DISCOVERY-002 | [案例语义](#des-site-value-path) | 四步语义、行动主体及结果共用同一案例身份与来源 | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-02 的细化场景：同候选设置跨 record/版本混用、缺结果和未执行分支负例；检查四步次序及主体；网络不出现模型/世界请求 | 实施 PR：source HEAD、语言、入口、fixture 与实际结果；未执行项如实保留 | fixture 不证明原始材料真实 |
| [REQ-SITE-DISCOVERY-003](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-003) / AC-SITE-DISCOVERY-003 | [回访与分享](#des-site-return-sharing) | 文章直达、继续阅读及分享摘要与撤回状态一致 | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-03 的细化场景：同候选测试空列表、文章直达、RSS 与相关回链；撤回后核对正文/OG/封面，模拟分享取消或复制失败 | 实施 PR：source HEAD、语言、入口、fixture 与实际结果；未执行项如实保留 | 不证明第三方缓存清除或回访增长 |
| [REQ-SITE-DISCOVERY-004](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-004) / AC-SITE-DISCOVERY-004 | [预览适配](#des-site-preview-fit) | 下载候选与源码案例的条件、来源和已验证范围分离 | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-04 的细化场景：同候选 mock Release：旧版有验证而新版无验证、案例版本不同、缺资产/未知平台；核对 unknown 与条件分流 | 实施 PR：source HEAD、语言、入口、fixture 与实际结果；未执行项如实保留 | 不代签安装或游戏目标 |
| [REQ-SITE-DISCOVERY-005](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-005) / AC-SITE-DISCOVERY-005 | [回访与分享](#des-site-return-sharing) | 小说分享保留虚构性质与实际语言，并提供有效回链 | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-05 的细化场景：同候选从英文入口进入中文小说；检查正文/分享身份、无译文处理、reader 失败与返回路径 | 实施 PR：source HEAD、语言、入口、fixture 与实际结果；未执行项如实保留 | 不证明小说质量或新增翻译 |
| [REQ-SITE-DISCOVERY-006](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-006) / AC-SITE-DISCOVERY-006 | [视觉交付](#des-site-visual-delivery) | 页面局部样式、媒体预算及中宽适配保持可读性 | [官网发现验证手册](public-site-discovery-validation.manual.md)：S6-DISC-06 的细化场景：同候选核对三视口及 768–1024px、中英/200% 缩放/键盘/无 JS/减少动效/损坏图片；记录实际资源传输 | 实施 PR：source HEAD、语言、入口、fixture 与实际结果；未执行项如实保留 | 概念稿不证明性能或无障碍达标 |
| [REQ-SITE-DISCOVERY-007](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-007) / AC-SITE-DISCOVERY-007 | [IMMERSION](#des-site-immersion) | 静态冲击、连续视觉身份、可控增强与资源代价 | [官网发现验证手册](public-site-discovery-validation.manual.md)：复用外部浏览器环境和真人观察方法，按本设计第 13.3 节扩充同候选静态/增强/暂停/失败、多屏录像与性能对比；不声明现有脚本已覆盖新增场景 | 实施 PR：版本、素材身份、截图、录像、网络/性能和原话分别记录 | 不证明主观转化或运行世界 |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[REQ-ENTRY-CANDIDATE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001) | [已有设计](#des-site-download) | 版本、日期、Release 说明、实际资产与 checksum 以同一候选快照原子更新；失败不混版 | [官网验证手册](public-site-discovery-validation.manual.md)：S6-DISC-04，按原 fixture/同候选浏览器方法核对；仅本行网站子范围 | PR 记录来源审查与适用页面/负例结果，不由链接代替执行 | 两 primary mode、跨域 QA/经济/恢复及完整 release gate 不由网站下载片段覆盖；本次未运行 |
| Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff；[REQ-ENTRY-CLAIM-001](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001) | [已有设计](#des-site-evidence) | runtime_capture 绑定版本/场景/证据窗口与可公开来源；来源不足不背书运行事实 | [官网验证手册](public-site-discovery-validation.manual.md)：S6-DISC-02，按原 fixture/同候选浏览器方法核对；仅本行网站子范围 | PR 记录来源审查与适用页面/负例结果，不由链接代替执行 | 全渠道专业事实、QA verdict 和 LiveOps 审核闭包仍外部；本次未运行 |
| Trace refs: O7-REQ-f71ac2e9-1586-44af-89b7-e98ff364e70c；[REQ-ENTRY-CLAIM-002](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-002) | [已有设计](#des-site-publishing) | 同 URL 纠正/撤回同步 CN/EN、详情/摘要/RSS，回滚不复活错误正文 | [官网验证手册](public-site-discovery-validation.manual.md)：S6-DISC-03，按原 fixture/同候选浏览器方法核对；仅本行网站子范围 | PR 记录来源审查与适用页面/负例结果，不由链接代替执行 | 其他 active public surfaces、自动漂移检测与第三方缓存收回不在本设计；本次未运行 |
| Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385；[REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003) | [已有设计](#des-site-evidence) | concept/illustrative/runtime_capture 与小说类型分离，目标/概念不冒充真实运行来源 | [官网验证手册](public-site-discovery-validation.manual.md)：S6-DISC-02，按原 fixture/同候选浏览器方法核对；仅本行网站子范围 | PR 记录来源审查与适用页面/负例结果，不由链接代替执行 | 其他专业领域、非官网渠道及全部公开审核仍外部；本次未运行 |

文档阶段适用的现有命令为 `./scripts/doc-governance-check.sh`、`./scripts/readme-link-check.sh` 与 `git diff --check`；实际是否运行、范围和结果只写 PR。若环境无法克隆或运行全仓门禁，如实保留未执行，而不是用局部链接检查冒充全量 PASS。

未来页面实现另执行 `./scripts/site-homepage-claim-check.sh`、`./scripts/site-download-check.sh` 及 S6 适用测试，并在同 PR 补相应负例。生成图、结构检查、真人理解和线上部署分别判定。新增内容/媒体不能独立升级 current claim。

## 12. 决策、长期风险与未决问题

采用静态精选而非实时 API：降低服务依赖与敏感数据风险，代价是非实时；以后接实时只读接口时独立设计新鲜度/断线/权限，不能直接删除“非实时”标签。采用原生 HTML 而非 CMS/框架：保留无 JS 路径，代价是少量摘要同步；依靠轻量检查减少漂移，不增加运营平台。

既有单张概念板保留作风格参考；新增视觉要求需要多屏比较面与精确时序合同，不能以旧单板替代每个路由的真实实现证据。选择真实材料而非虚构成功故事可能导致空列表；真实空状态优于假活跃。

材料和验证输入仍需确认：视觉/QA owner 在实现前选定可公开案例与原图，发行 owner 在发布前核对版本/费用/平台，内容 owner 决定首篇文章。没有来源时排除 runtime_capture，没有真实译文时不发布该语言文章；不阻止当前文档 PR 创建，也不强制新 Issue。多屏构图与正文并行设计，动效和短片按下文分级，不新增前置平台建设。

### 概念板构图的实现边界

首页主图作为 hero 内的绝对定位视觉层，以渐变遮罩融入底色；窄屏不再堆叠独立图片块，文字和主操作保留实色对比。四步桌面以方向连接符和明确标为概念氛围的裁切配图形成阅读链，手机使用纵向连接线。案例类型与无世界效果说明始终可见，较长来源说明使用可键盘展开的原生 disclosure。记录、进展、预览与规则入口采用紧凑图文节奏；世界观主导航只引导到标明实际语言和虚构性质的展开区；真实空记录保持次级，不为填满卡片复制案例。预览入口合并到首页图文区，保留本地环境、模型配置与未开放公共加入的预览摘要；平台与安装详情在原生 disclosure 中按需展开，预览 CTA 和 `#download` 深链在增强模式下自动打开；世界规则入口和 `#world-laws` 深链同样展开其正文。延迟 hash 事件不能重新打开用户刚关闭的内容。无 JS 时手动展开仍可访问平台包，完整风险提示与下载按钮同在展开内容中。

### 公开首页概念主图

步骤配图使用原创生成式氛围图 `site/assets/images/hero/asteroid-industrial-cluster-20261015.webp`（1536 × 864），由内置 imagegen 按本概念板的墨绿工业小行星方向生成，再压缩为 WebP。生成工具未提供可核验模型版本或 seed，不作补写。它只表达文明制图的世界氛围，图注与替代文字明确其概念性质；不属于运行截图或世界结果证据。四步中的裁切同属该氛围素材。

首页当前主图 `site/assets/images/hero/asteroid-settlement-planet-20261015.webp`（1535 × 864）由内置 imagegen 参照已采用概念板重建工业聚落、远景行星与逆光，再压缩为 WebP；仅作概念氛围。原图保留，模型版本及 seed 未提供。

四步中的第 02 与第 04 步的概念配图分别使用 `site/assets/images/steps/decision-response-control-bay-20261015.webp` 与 `site/assets/images/steps/decision-choice-horizon-20261015.webp`（均为 864 × 288）。它们由内置 imagegen 参照概念板对应低清小图重建，再压缩为 WebP；前者为薄荷光工业控制空间，后者为带远景聚落与分叉物流线的地平线。配图仅用于氛围与阅读节奏，不构成 Agent 行动、运行回执或具身能力证据；沿用同一可见概念配图说明。未知模型版本与 seed 不补写。

首页入口与剩余步骤使用 `site/assets/images/entries/` 下的 768px WebP 缩略副本（`industrial-cluster-thumb.webp`、`industrial-network-thumb.webp`、`civilization-orbit-thumb.webp`），分别从保留的同名方向原图等比例缩小与压缩；不改构图、不新增运行语义。尺寸覆盖 390px 手机与 1440px 桌面验收视口下入口及步骤横幅的两倍像素密度，避免给小图传输完整主图。

首页可读性使用单一作用域 `body.discovery-page:not(.content-page)` 的字号与间距变量：正文 1rem、辅助 .875rem、图注 .75rem；同一窄屏断点合并首页布局，内容页不消费该组覆盖。主操作为了解玩法，来源、短摘要与关键限制常显，空记录区降低为次级状态，保留旧锚点与栏目 URL。

真实过程可依据保留的受控测试报告整理为玩家文章，但必须分别说明源码版本、预置条件、世界结果与未覆盖项；历史 native Viewer 图仅展示其实际可见界面及抓图来源，不能与 CLI 案例拼接为 GUI 实玩。首次预览指引只链接对应版本说明、现有配置/启动手册与排障，不把当前源码能力当成下载候选已验收能力。

首页静态结果面板根据同次记录展示设施、配方消耗、产物与结束余量；转入量与实际消耗分列，并明确非实机界面。安装包指南与案例条件独立，案例条件由原生 details 和同页锚点打开。详情正文解释过程转折，核验方法另行披露。根路径、index.html 与双语文章的构建标记及资源身份应在部署后一起核对。

主展示使用 runtime_capture 元数据固定测试源码、构建哈希、运行 profile、场景、捕获窗口与保留报告地址；四步默认使用玩家语言，日期、预置世界、调用数、事件/库存核验和未覆盖范围由原生 disclosure 承载。没有当前同次 GUI 图像时，使用结构化 CLI 过程与来源链接，历史 native 图保持独立折叠，不与案例拼接。

预览核验声明绑定发行 tag。只记录已查实的构建发布事实时，明确不能证明包安装或游戏目标；API 切到未知候选后，将其声明切为 unknown，旧候选来源不能代签新版本。起步建议与已验收能力分开。资产/校验和仍按同一 Release 快照选择，不由版本文案推导可用性。

首页不再创建隐藏 Canvas 或初始化其绘图循环；旧说明性三场景切换器没有消费者时一并删除。分享封面使用 `site/assets/images/social/first-iron-batch-{zh,en}.svg` 的代码原稿及其 1200×630 PNG 导出，明确为案例图形摘要、非实机截图；文章使用独立 OG/Twitter 标题、摘要与封面。共享 CSS/JS 的消费者统一发布版本参数，中宽 768–1024px 单独核对两列步骤、入口和首屏。

<a id="static-experience-refinement"></a>
## 13. 宣传体验细化的静态实现合同

本节承接原六项要求的[细化范围](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#discovery-refinement-scope)及新增 [007](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-007)，不新增页面框架、运行 API、后台数据源或独立产品模块。第 2.1 节的 Trace refs 继续适用；字段与界面调整随对应页面实现，不能仅凭本文存在视为已完成。

<a id="des-site-value-path"></a>
### DES-SITE-VALUE-PATH：入口与同案例语义

承接 REQ-SITE-DISCOVERY-001/002 与[价值入口](../../product/player-entry-distribution/public-site-discovery-and-showcase.design.md#value-and-entry-contract)、[案例叙事](../../product/player-entry-distribution/public-site-discovery-and-showcase.design.md#decision-walkthrough)。Hero、案例结尾、预览和内容页分别实现产品设计中的主次动作，不新增强制路由或访客分类。动作是有真实 `href` 的原生链接；站内深链遵守已有 ID、部署前缀和 disclosure 合同，不能只靠 onclick 导航。

四步以同一案例下的有序内容表示，语义统一为 `goal / constraint / adjustment / outcome`，对应设定目标、遇到阻塞、调整行动、核对结果。可使用现有 HTML 结构、语义列表或 `data-*` 承载；不要求新 JSON 清单、浏览器模板引擎或状态机。四步、初始条件与结果共享 record 身份及来源版本；语言镜像不得自行更改阶段含义。结果后的下一步提示不属于第五个运行事件。

每步说明行动主体及已知事实。玩家授权、Agent 解释与世界结果不能合并成一种“成功”状态；目标、限制或结果缺少来源时显示相应未知，不计算或生成缺失的前后值。未执行选择只可作为明确的讨论方向，不渲染伪造的第二条运行结果。结构化 CLI 摘要和概念插图继续分别标识，不能因同处一面板而成为同次 GUI 证据。

主路径不依赖 hover、自动轮播、横向拖动或滚动劫持。无需交互的四步保持同时可读；可选媒体操作仍按 record/media 身份隔离失败与迟到结果。页面浏览不发出模型调用、世界读写请求或连接账户。

<a id="des-site-preview-fit"></a>
### DES-SITE-PREVIEW-FIT：参与条件与候选范围

承接 REQ-SITE-DISCOVERY-004 与[预览决策](../../product/player-entry-distribution/public-site-discovery-and-showcase.design.md#preview-decision)。预览区先用静态短摘要/定义列表说明适合对象、已核实范围与准备条件，再保留原生平台/安装展开区。成本、签名与保留限制跟随对应动作；不新增勾选问卷、设备权限请求或 Key 输入框。

展示上下文分为“官网观看”“下载候选”“源码案例”。这只是说明与来源的分离，不是新游戏模式或发行状态枚举。官网观看不要求账户或 provider；下载候选沿现有 Release 快照展示资产和该候选的已核实范围；源码案例保留固定源码、profile、预置条件与证据链接。不能把案例来源写入下载候选的验收字段，也不能由 assets 存在推导安装或首次进入成功。

候选切换继续沿用现有下载读取层。更新 tag/assets/checksum 时，同步更新适用条件与核验说明；新候选无对应核验来源时降为 unknown。旧来源只允许留在明确标明旧版本的区域，不留在新下载按钮旁充当通过证明。无合法资产时不显示假下载动作；有真实资产但可玩范围未知时明确说明未知，不把“资产可获取”写成“已验证可玩”。

条件与来源可复用现有 HTML 元数据及发布说明，不建立第二份 readiness 数据库。页面只说明缺口并指向实际存在的指南或发行说明，不自动修复客户端、切换 provider 或绕过安装信任警示。不安装的出口到已有开发进展/内容入口，缺内容时保留可读说明和现有 GitHub 链接。

<a id="des-site-return-sharing"></a>
### DES-SITE-RETURN-SHARING：编辑、回访与分享一致性

承接 REQ-SITE-DISCOVERY-003/005 与[回访体验](../../product/player-entry-distribution/public-site-discovery-and-showcase.design.md#editorial-return-path)。复用详情 HTML 这一编辑源；玩家问题、变化、影响与剩余限制以正文/摘要表达，不增加自动新闻生成器或从 PR 数量推导活跃。相关内容和同语言 RSS 链接随详情维护，缺条目时不展示占位推荐。直达详情必须独立说明项目、类型、日期/版本与有效回链。

沿用现有页面 `<title>`、description、OG/Twitter、canonical 与语言映射，不另建分享配置源。记录摘要及封面文字继承真实来源范围；小说在标题/摘要中保留小说身份，英文入口保留正文实际语言。概念图、案例图形摘要与真实截图的性质在 alt/描述中保持一致；分享卡片不提供绕过正文 claim 的第二套文案。

正文纠正或撤回时，在同一变更中核对详情、首页/列表推荐、RSS、相关链接及分享元数据。撤回页不得仍用成功故事作 OG 标题/封面；敏感素材按现有安全流程移除。不能承诺第三方抓取缓存即时更新；当前页面应提供正确解释，回滚仍保留纠正。

canonical 使用已配置的公开站点与真实路由；同语言详情有自己的 URL。只为真实存在的语言对应页声明映射，不把所有详情的 canonical 指向首页，也不凭路径构造英文小说。站内链接继续支持项目子路径；不将任意 query/hash 或用户输入拼成新的外部跳转地址。

复制链接或浏览器原生分享均是可选增强：只在明确点击后调用，取消/拒绝不显示“已分享”或阻断阅读；普通永久链接始终可用。无第三方分享 SDK、邮件收集、持久画像或隐形事件上报。观察测试结果只进入实施 PR 的实际记录，不以打点系统作为上线依赖。

<a id="des-site-visual-delivery"></a>
### DES-SITE-VISUAL-DELIVERY：视觉层级与资源边界

承接 REQ-SITE-DISCOVERY-006 与[视觉语言](../../product/player-entry-distribution/public-site-discovery-and-showcase.design.md#visual-language)。沿用共享 CSS/JS 和首页既有作用域，整理颜色、字号、间距与边框变量；文章/手册/reader 不被首页专用规则意外覆盖。网格、轨道、连接线用非交互装饰表达，不生成假遥测、不恢复隐藏 Canvas 或无消费者动效。状态、主体与操作意义不能只靠颜色。

沿用首屏必要图片总传输量尽量 ≤500KB 的预算目标，统计范围包括实际首屏背景、poster 和其他必要图片，不给每个模块另开 500KB 额度。已存在缩略图优先；按需原图不牺牲证据可读性。图片有尺寸与替代文字，非首屏延后加载；第三方视频只在明确点击后加载且保留文字/海报。无视频、新图或字体不影响完整主路径。

1440×900、390×844、360×800 之外，按现有中宽约束核对 768–1024px；检查中英标签、四步和图文比例，不只验两端宽度。预算与可读性冲突时先调整裁切、加载顺序和装饰，不缩小警示文字或隐藏素材类型。新增素材独立保存来源/许可，遵守素材使用边界；本轮不新增素材资产。

<a id="refinement-verification"></a>
### 13.1 补充验证与实施顺序

以下对应原 REQ/AC 与 S6-DISC-01–06，只细化验证场景，不新增 mandatory workflow 或通用门禁。后续实施在同候选静态服务器上扩充现有 `scripts/site-discovery-check.py`、其负例测试与 `scripts/site-discovery-browser.test.cjs` 的适用覆盖；已有脚本是否覆盖新增场景必须实际检查，不预写通过结论。

| 上游 REQ / AC | 本节条款 | 同候选检查与负例 | 证据与未证明范围 |
| --- | --- | --- | --- |
| [001 / AC-001](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#ac-site-discovery-001) | [VALUE-PATH](#des-site-value-path) | 首页、案例、预览、文章直达的主次链接；旧锚点、无 JS、子路径；重复强主 CTA 或无目标链接应被发现 | 结构/浏览器结果与访客原话分开；链接通不证明吸引力 |
| [002 / AC-002](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#ac-site-discovery-002) | [VALUE-PATH](#des-site-value-path) | 四步语义与顺序；跨 record/版本混用、缺结果、未执行分支、概念图误标；禁止世界/模型请求 | fixture、网络记录及素材审查；自动化不证明事实来源真实 |
| [003 / AC-003](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#ac-site-discovery-003) | [RETURN-SHARING](#des-site-return-sharing) | 零内容、文章直达、RSS、相关回链；撤回后错误 OG/封面、失效翻译、分享取消或复制失败 | 静态 HTML/XML 与浏览器记录；不证明外部缓存清除或回访增长 |
| [004 / AC-004](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#ac-site-discovery-004) | [PREVIEW-FIT](#des-site-preview-fit) | 旧候选有核验而新候选无核验、源码案例与下载版本不同、资产缺失、未知平台/费用 | mock Release、DOM 与人工条件复述；不代签包安装或游戏目标 |
| [005 / AC-005](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#ac-site-discovery-005) | [RETURN-SHARING](#des-site-return-sharing) | 英文入口到中文小说；正文/分享保留小说身份；无对应语言、原有 reader 失败与回链 | 路由、元数据与浏览器记录；不证明小说质量或新增翻译 |
| [006 / AC-006](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#ac-site-discovery-006) | [VISUAL-DELIVERY](#des-site-visual-delivery) | 三视口及 768–1024px、中英、200% 缩放、键盘、无 JS、reduced-motion、损坏图片、视频未点击 | 真实截图、焦点/网络/资源记录；概念板和预算文字不证明性能达标 |

访客观察按[产品细化验收](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#refinement-acceptance)实施，记录入口、语言、设备、提示与原话；理解、选择和回访意愿不由结构断言代替。不新增固定访客数量审批，也不以点击率替代正确理解。

价值文案、四步和主视觉/多屏分镜并行确定，再落实预览/分享条件一致性；静态构图成立后增加轻量动效，视频另行决定。能原子完成时保持单 PR，不为三层分工拆三个 PR。源码页面未修改的文档 PR 只验证文本和关系，不要求伪造浏览器、安装或访客证据。

<a id="des-site-immersion"></a>
### 13.2 DES-SITE-IMMERSION：科技感、沉浸与视觉冲击的渐进交付

承接 [REQ-SITE-DISCOVERY-007](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-007)、[AC-SITE-DISCOVERY-007](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#ac-site-discovery-007)及[多屏与动效规格](../../product/player-entry-distribution/public-site-discovery-and-showcase.design.md#cinematic-experience)。视觉 owner 决定镜头构图、焦点和三维效果目标；实现负责正确交付，不从 CSS 动画数量推导产品质量。

**静态观测站构图。** 首页使用 `observatory` 局部作用域：墨蓝背景、薄荷色主动作、琥珀预览状态；不改变内容详情和手册页面。桌面 Hero 采用非对称文案与世界图、接近一个视口的阅读尺度，手机保留主体裁切与文字暗面并允许内容自然增高。案例用同主题工业近景接续，四步显式标出玩家目标、Agent 尝试、Agent 调整和世界结果；结果图解与概念图分开。日志、预览、FAQ、小说按顺序阅读；预览的范围、准备、费用与升级信息无需展开即可看见，包详情继续使用原生 disclosure。小说名称、中文身份和虚构边界在展开前可见。首期只提供短控件反馈，不引入视差、循环、视频或新的字体请求，因此无需显示无实际增强可控制的暂停按钮。

沿用 `asteroid-settlement-planet-20261015.webp` 与 `asteroid-industrial-cluster-20261015.webp` 的已有 AI 概念素材及来源记录；近景仅表示美术接续。Hero 桌面焦点为 75%/45%，手机为 67%/40%；近景高度在手机压缩，小说尾图延续世界意象。图片失败保留概念图说明、暗色底、全部 HTML 文案和动作；不追加更重替代素材。Hero 使用高优先级加载，后续概念图延迟加载，移除日志/下载/规则重复装饰缩略图。

**结构与资产。** 采用“语义 HTML 内容 + 原创静态世界层 + 可选 CSS/JS 增强”，不新增 WebGL/Three.js、Canvas 粒子引擎或运行服务。Hero 世界层可全幅延伸，正文保持既有容器与阅读行宽；装饰层使用独立层叠上下文、`pointer-events: none` 和适当的 `aria-hidden`，不能拦截 CTA、遮住 focus 或代替必需图注。具有信息价值的图片仍用语义图片/图注，不因装饰层实现丢失来源性质。

以现有媒体记录补充各素材的 `scene/role`、桌面与移动焦点、适配尺寸、来源/许可、provenance 和静态替代；这些是维护所需信息，可放在既有文档、HTML 和 CSS 中，不要求第二份 manifest 或数据库。`picture/srcset/sizes` 或现有等效响应式方案选择实际需要的资源；不同时预载桌面和手机主图。主视觉在 HTML/CSS 中可直接发现，不等待 JS、Reveal 或远端接口才出现；图片预留尺寸，正文不能等字体和素材加载完才可读。

**动效职责与生命周期。** 沿用 `site/assets/app.js` 的页面局部初始化，可独立封装视觉增强职责而不扩成新的全局控制器；检查目标 DOM 与 API 后幂等注册，卸载或 `pagehide` 时移除监听、断开 observer、取消帧回调。视觉错误只影响装饰，不能中断语言切换、版本刷新、四步正文或 reader。CSS 与 DOM 初始态即完整可见；只有装饰允许渐入，不能先给整页 `opacity:0` 再寄希望于初始化成功。

| 条件/输入 | 呈现与处理 | 恢复规则 |
| --- | --- | --- |
| 首次、无 JS、相关 API 不可用 | 完整静态世界与正文，无必要 loading | 缺增强不显示错误成功；不改导航或可玩状态 |
| 系统减少动效或用户关闭增强 | 去除视差、空间位移和循环，保留静态层次及必要状态反馈 | 系统限制优先；取消系统限制后不推翻仍有效的用户关闭选择 |
| 正常且区段可见 | 只运行适用的一次性装饰强调；控件反馈/空间位移沿产品规格 | 一次性效果不因反复进出视口重播；无效应需求不常驻 rAF |
| 区段离开视口或标签页隐藏 | 停止该视觉计算及自启动媒体，不仅改为透明 | 返回后仍检查系统与用户选择；用户暂停的效果不能自动恢复 |
| 用户暂停持续效果 | 停止相关 CSS 动画、帧回调与媒体，保留可读帧/静态替代 | 控件不要求焦点驻留；只有明确恢复且无系统限制时重启 |
| 媒体/增强初始化失败 | 保留文字、身份和本地静态替代；清理半初始化资源 | 不拉取更重的备用媒体，不复活错误素材、不影响其他模块 |

暂停/关闭控件只在确有增强时出现并有可读名称与键盘操作，不占用首屏主 CTA。选择只需页面内记忆，不新增持久画像或远端同步。`IntersectionObserver` 负责区段可见性，`visibilitychange` 负责页面前后台；两者与系统/用户选择共同决定是否运行。不得用鼠标是否悬停或窗口 focus 作为唯一暂停机制。

短过渡优先使用 transform/opacity；避免逐帧布局测量、全屏大模糊和多层混合滤镜。可选 Hero 视差仅在精细指针且无减少动效时注册，事件合并到必要的单次帧更新，最大位移沿产品约 8px 起稿值；不把每次 pointermove 存储、上报或启动永久循环。触屏不提供必须依赖 hover 的信息。默认不增加无限自动动效；任何持续自动效果都要满足暂停合同，不借“纯装饰”豁免。

**主动媒体与安全。** 后续短片只有在素材、来源、许可及设计完成后才增加真实入口；首期不设假播放按钮。用户点击前不挂载第三方 iframe 或请求视频源，保留站内 poster/文字；点击失败、播放拒绝、关闭和离页均释放相关资源。默认不有声、不自动打开全屏。概念片和真实案例的标识在片内持续可见；正文不依赖观看完成。短片时长、码率和传输预算需在实际素材确定后单独记录，本合同不假设已存在视频。

**性能与降级。** 继续采用首屏必要图片总传输量尽量 ≤500KB 的既有目标；不是全部页面或每个模块各有 500KB。新增视觉控制 JS 的压缩传输量以 ≤12KB 为本方案预算目标，不含既有脚本，不构成新的 CI 准入规则；超出时先简化增强并记录理由，不压缩正文或隐藏警示。首屏不新增必要视频、字体和通用动画库。静态→CSS 增强→按点击媒体逐级独立成立，移动端优先裁切/尺寸与更少装饰，而非把品牌画面完全移除。

实施时在同浏览器、设备/CPU/网络、视口及缓存条件下比较静态与增强的 LCP、CLS、请求/传输、长任务和交互响应；记录滚动与静置时是否还存在无必要计算。公开 Web Vitals 的良好阈值为 LCP ≤2.5s、CLS ≤0.1、INP ≤200ms，线上判断需适用的真实访问第 75 百分位数据；本地截图、Lighthouse 分数或一次脚本测试不等于线上 P75/INP 达标。没有真实样本时标未知，不为了测量添加追踪 SDK。帧率和主线程代价按实际目标设备记录，不承诺所有设备固定 60fps。

技术依据：浏览器偏好与生命周期使用 [MDN prefers-reduced-motion](https://developer.mozilla.org/en-US/docs/Web/CSS/@media/prefers-reduced-motion)、[IntersectionObserver](https://developer.mozilla.org/en-US/docs/Web/API/IntersectionObserver)、[Page Visibility API](https://developer.mozilla.org/en-US/docs/Web/API/Page_Visibility_API)；暂停要求参照 [W3C WCAG 2.2 SC 2.2.2](https://www.w3.org/WAI/WCAG22/Understanding/pause-stop-hide.html)，其中间接滚动/hover 触发也不能被当成明确播放同意；性能阈值来源为 [web.dev](https://web.dev/articles/defining-core-web-vitals-thresholds)。资料用于确定实现和验收方式，不表明 oasis7 已通过对应标准。

<a id="immersion-verification"></a>
### 13.3 沉浸体验验证与视觉回退

适用要求为 [007 / AC-007](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#ac-site-discovery-007)，并继续覆盖 002 的素材性质及 006 的可访问性。沿用现有 Site/S6 与浏览器测试入口，实施时补足相应断言，不新增独立 mandatory workflow；下表是新增用例设计，不是已有脚本的覆盖声明。

| 场景 | 方法与同候选证据 | 成功与未证明范围 |
| --- | --- | --- |
| 视觉身份与三维效果 | 同一 source HEAD/素材身份下，CN/EN、1440×900/390×844/360×800 及中宽取得首屏和完整多屏截图；非项目成员按 AC-007 分别记录科技感、连续感与记忆点 | 截图可判断构图，访客原话用于主观效果；不把作者评分或单张概念板当全站通过 |
| 静态与失效 | 关闭 JS/相关 API、初始及运行中减少动效、破坏主图、让增强初始化报错；核对世界主体的适配替代、正文、状态、CTA、语言与下载仍工作 | 无整页透明、无必需视频等待、无跨模块异常；失败截图不证明正常动效时序 |
| 暂停与可见性 | 连续滚动录像包含进入/离开/返回、用户暂停/恢复、标签页隐藏/显示及系统偏好切换；同时检查 CSS/帧回调和媒体状态 | 暂停后不因返回视口重启，隐藏时无本视觉无用计算；静态截图不能代签暂停或帧率 |
| 指针与移动端 | 精细指针、触屏、键盘、200% 缩放下测试视差与焦点；检查装饰层不拦截，位移不改变点击目标 | 无 hover-only 信息或光标劫持；手机仍有辨识主体，不暗示手机可运行游戏 |
| 来源与动态暗示 | 检查同世界美术接续、关系线及结果图解；模拟缺来源、未发生分支 | 氛围仍标概念，四步不靠动画生成结果，网络无模型/世界请求；连续视觉不代表连续实机 |
| 资源与性能 | 冷/暖缓存条件分开，同环境对比静态与增强的传输/LCP/CLS/长任务、交互和滚动记录；检查未点击时无视频请求 | 符合预算或明确修正；实验室结果不冒称真实访问 P75/全部设备帧率 |

每份证据绑定候选、路由、语言、视口、素材版本、浏览器及动效状态；文档阶段只核对关系与规格，未取得项仍为待验证。若增强造成闪烁、布局漂移或理解干扰，先关闭该增强并保留静态主视觉、来源与链接，不整站退回旧错误口径；素材替换和版本参数沿现有发布链同步。视觉验收不改变游戏 readiness、发行和隐私边界。
