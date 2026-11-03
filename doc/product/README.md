# oasis7 产品模块入口

本页是产品信息架构的唯一总入口，按玩家价值组织产品，不完全替代 `doc/README.md` 的工程模块矩阵。四大模块的产品真值与设计将逐步迁入本目录的文档树；外部各专业模块中散落的相关产品文档也将随语义归拢逐步清理、删除。

本目录的文档树以 PRD 为主，并可按需包含同名配对的 design：PRD 承载产品真值、产品承诺、组合关系和跨域验收；design 承载产品设计。迁移与交付事实由 Git、PR、实际 CI 与评审记录承载；Issue/Project 按需使用，不创建本地 project ledger。

产品文档规范与可复用模板：[`product-documentation-standard.prd.md`](../engineering/doc-governance/product-documentation-standard.prd.md)、[`product-documentation-standard.design.md`](../engineering/doc-governance/product-documentation-standard.design.md)、[`product-documentation-standard.templates.md`](../engineering/doc-governance/product-documentation-standard.templates.md)。

## C1 生效：四域最终标签与职责边界

本节承接 P0 冻结的产品决策；C0 的旧名/新名映射现仅作为历史兼容与迁移 provenance 保留，C1 现将 active 产品入口与模块根身份统一收紧为最终中文标签；active 专题保留各自的局部产品题名，但其模块归属与产品身份声明使用所属模块的最终中文标签。旧标签不再作为 active identity；目录 slug 和 `Product PRD-ID` 仍是稳定身份，不因本次展示名收敛改变。

| 最终中文名称 | 稳定身份（不可变） | 当前 active 展示名 | 核心问题与产品职责 |
| --- | --- | --- | --- |
| **世界规则与玩法系统** | `world-rules-core-gameplay` / `PRD-PRODUCT-001` | `世界规则与玩法系统` | 玩家在世界中能做什么、付出什么、获得什么；定义世界规则的产品约束、玩家目标、核心循环、成长、资源/工业/经济、组织/区域/冲突/治理等长期玩法。 |
| **权威世界基础设施** | `world-infrastructure` / `PRD-PRODUCT-002` | `权威世界基础设施` | 世界如何保持唯一、确定、持久、可验证、可恢复；定义最终性、权威历史、状态可用性、确定性执行、版本兼容与恢复的产品保证，不拥有工业规则、经济语义、市场、区域或玩家循环；技术执行保证见 [`权威世界基础设施 PRD`](world-infrastructure/prd.md)。 |
| **智能体、世界模拟与交互** | `agents-world-simulation` / `PRD-PRODUCT-003` | `智能体、世界模拟与交互` | Agent 如何理解和行动、玩家如何观察和干预；定义 Agent/provider、目标与委托、记忆/学习、世界观测、模拟反馈，以及进入世界后的 Viewer、世界舞台和玩家交互体验。 |
| **玩家接入与发行** | `player-entry-distribution` / `PRD-PRODUCT-004` | `玩家接入与发行` | 玩家如何了解、进入并持续使用受支持产品路径；定义发现、访问模式、账户/会话、安装升级、发行沟通、可选服务与世界外参与反馈，不拥有世界内成长或权力。 |

四域按职责划分，不要求规模相等，也不新增第五模块；`core`、`game`、`world-runtime`、`world-simulator`、`p2p`、`testing`、`engineering` 与 `site` 继续作为专业或治理域。产品语义可以跨域组合，但每一条规范性要求必须有一个主责条款；“主题出现在哪个模块”不等于“该模块拥有主题的全部语义”。

### 条款级唯一主责与跨模块引用

Trace refs: O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6；新增或实质修改的跨层要求按[全局要求追踪](../engineering/doc-governance/cross-layer-requirements-traceability.design.md#global-requirement-trace)在主责原文声明一次，设计和任务复用同一 ID。既有 Product PRD-ID、局部 REQ/AC/SC 与真实路径/锚点保留；未触达存量和历史证据不批量回填。

以下拆分是长期归属规则。每一行的“主责条款”拥有规范性条件、成本/权利/失败含义；其他模块只能保留必要摘要、消费主责结果，或提出自己的组合验收条件，不得再次立法：

| 语义簇 | 主责条款 | 跨模块引用与组合验收 |
| --- | --- | --- |
| Agent 资产与授权 | Agent 的取得、维护、容量、转让条件与经济权利 → 世界规则与玩法系统；自治、委托、撤销、异议、override 与行为归因 → 智能体、世界模拟与交互；权威身份/状态执行 → 权威世界基础设施；账户登录/会话恢复 → 玩家接入与发行。 | 组合验收必须同时证明经济条件、有效授权、权威状态与会话边界；任一主责条款未成立，其他模块不得把局部结果表述为完整转让或控制权生效。 |
| 游戏设施与系统基础设施 | 工厂、仓库、物流、能源设施的作用、投入与收益 → 世界规则与玩法系统；共识、网络、存储、runtime 的产品保证 → 权威世界基础设施。 | 工业结果须由玩法语义与同一权威世界历史共同验收；基础设施能力本身不产生设施收益、市场权利或玩家循环进展。 |
| Viewer 与 Launcher | 进入世界后的观察、操作、信息层级与因果反馈 → 智能体、世界模拟与交互；下载、启动、模式选择、真实后端核验、升级与重新进入 → 玩家接入与发行。 | 跨入口验收组合玩家可读因果、模式语义一致性、真实后端到达和版本/恢复边界；不可按同一可执行程序把两域合并。 |
| 恢复与连续性 | 损失后的重建与经营恢复 → 世界规则与玩法系统；Agent 计划/委托续接与交互上下文恢复 → 智能体、世界模拟与交互；原世界历史重建 → 权威世界基础设施；安装、登录与会话恢复 → 玩家接入与发行。 | 恢复验收必须区分世界历史、Agent 授权、玩家界面上下文和本地会话；任何一层恢复都不能代签另一层。 |
| 成长、声誉与认可 | 世界内成长、合同信誉与区域资格 → 世界规则与玩法系统；测试反馈、文档贡献、社区参与等外部认可 → 玩家接入与发行。 | 组合验收可以证明两者并存，但外部认可不自动兑换世界资产、资格或治理权。 |
| WASM 与创作者扩展 | 扩展给玩家带来的能力、成本与制度后果 → 世界规则与玩法系统；安全执行、版本化与权威提交保证 → 权威世界基础设施；SDK、ABI、编译和部署细节 → 专业域。 | 验收将玩法准入/成本与安全执行/版本兼容分别取证，专业实现文档不改写产品权利。 |
| 治理 | 世界内组织、区域政策、公共资源与玩家权利 → 世界规则与玩法系统；协议升级与验证者转换的技术保证 → 权威世界基础设施；仓库流程、文档规范和评审规则 → 工程治理。 | 组合验收必须注明治理事项所属轨道；技术可行性、世界内授权和工程流程互不自动授予对方权力。 |

跨模块文档必须链接到主责条款（根 PRD 或其专题的稳定标题/锚点），并把摘要标为“消费/组合说明”；不得复制另一模块的条件、阈值、权利或状态机。组合验收须分别通过所有引用的主责条款及本模块新增的消费条件，不能由任一模块的局部 green 推导整体完成。

### P1/P2/C1 迁移阶段、兼容前置与非目标

历史实施与验收记录见 [C0 名称兼容 Task #3758](https://github.com/eng-cc/oasis7/issues/3758)、[P1 正式名称切换 Task #3763](https://github.com/eng-cc/oasis7/issues/3763)；总体治理背景见 [Task #3680](https://github.com/eng-cc/oasis7/issues/3680)，不将其作为所有迁移批次的接收方。本页保留条款归属、稳定身份和迁移条件，不镜像各批任务进度。

- **稳定身份与变更边界**：四个 slug、四个 `Product PRD-ID`、根 SC/REQ/AC 与 checker 合同继续保持；名称与分类调整不改变 runtime、WASM、Agent、Viewer、玩法数值或公开 claim。C0 的旧名/新名映射只供历史引用、迁移 provenance 和兼容窗口识别，不作为 active identity，也不允许任意名称或任意模块数。
- **P2（产品，按专题分片）**：按“源条款 → 目标条款/锚点 → 未接收语义 → 接收 owner → 删除条件”逐项迁移：Agent 资产/自治和免费进入/世界成长已按各自专题收敛；原区域 charter/tenure/公共融资 AC-1～5 已由[玩法区域专题](world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-004)的 GR-004～008 接收；工业/市场/保供旧源 AC-1～5 的产品语义现由[区域能力与扩展](world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)、[工业需求与生产交付](world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009)和[常态市场与有界紧急保供](world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004)的稳定锚点接收；普通治理 GG-1..5 的历史条款映射见[普通共同决策与宪制边界](world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md) §6.3。连续性旧源 CR-1/2 由基础设施模块 DC-1/DC-2 与 DE-1/DE-3 接收；CR-3/4/5 由 GCB-001/002/003/006、ES-003/004、根 [`SC-34`](world-rules-core-gameplay/prd.md#sc-34)（其中 CR-5 由成熟世界 `MW-12` 的 [`REQ-WR-MW-005`](world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)/[`AC-WR-MW-005`](world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005) 接收）与[同候选门禁 REQ/AC](player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001)追踪至成熟世界/可玩性/入口/公开 claim 专题，CR-6 由 CC-003/AC-6/AC-7 接收。原始 CR-1..6 条款与完整映射保留在 Git 历史提交 `a83c57893de2ec4217fb7c1a04d30575ebd783e6`，仅作 provenance，不作为 active authority、实现、QA 或发行状态证据。这些接收仅定义产品边界，不声明实现或发行状态；保留现有 REQ/AC 可追溯性，过程记录继续写入 Git、PR 与实际 CI 记录，不在仓库新增 migration ledger。
- **C1（当前 active-final 阶段）**：active 入口与模块根身份只使用四个最终中文名称；active 专题的模块归属与产品身份声明使用所属模块的最终中文标签，但专题标题仍是局部产品主题名称。C0 旧名仅可在历史兼容、迁移 provenance、`retired`/`superseded` 说明中作为别名保留，不得成为 active authority、稳定身份或 active 模块标签。后续迁移任务仍必须验证稳定 slug/ID、根身份、checker 规则、跨模块引用闭合、重复 authority 消除以及源/目标与失败恢复语义对账；`superseded` 不等于已迁移完成，也不等于可直接删除。
- **本阶段非目标**：不新增第五模块，不改目录 slug 或 Product PRD-ID，不批量重构 crate/工程目录；C1 当时的 checker 仅覆盖产品文档的结构边界，包括 active 入口/模块根身份的最终标签、专题归属与可达性、产品树内部真实 fragment 及配对/退役结构引用，不替代语义评审，也不证明语义完整性；允许同步 fixture 测试以最终名称为默认，并保留历史别名 prose。不改变世界规则实现、数值平衡、视觉交互、WASM/Agent/runtime 合同；C1 active 名称收紧、checker/link 通过或文档归属变化，均不等于语义迁移完成、源文件可删除、能力已实现、测试已通过或发布状态升级。

## 四大产品模块

| 产品模块 | 唯一入口 | 产品职责 |
| --- | --- | --- |
| 世界规则与玩法系统 | [`doc/product/world-rules-core-gameplay/prd.md`](world-rules-core-gameplay/prd.md) | 定义玩家目标、核心循环、成长、资源压力与世界规则体验。 |
| 权威世界基础设施 | [`doc/product/world-infrastructure/prd.md`](world-infrastructure/prd.md) | 区块链/分布式系统与确定性世界运行时底座：最终性、权威状态、复制、存储、网络、恢复和版本化执行边界。 |
| 智能体、世界模拟与交互 | [`doc/product/agents-world-simulation/prd.md`](agents-world-simulation/prd.md) | 把场景、Agent/LLM 决策、世界状态与可交互模拟体验连接起来。 |
| 玩家接入与发行 | [`doc/product/player-entry-distribution/prd.md`](player-entry-distribution/prd.md) | 统一玩家如何了解、进入、安装和验证当前有证据支持的技术预览及其公开边界。 |

<a id="game-design-coverage-map"></a>
## 游戏设计领域覆盖导航

本节按[领域覆盖与阶段证据规范](../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)检查全游戏范围，继续使用上表四个产品模块。模块列用于分流，具体主责按原条款；本表不重新定义玩法、技术合同或完成状态。

内容与表现设计、实际素材和制作资料的职责及落位见[非代码设计与交付物组织规则](../engineering/doc-governance/doc-structure-standard.design.md#non-code-design-deliverables)；详细目录复用原专业/制作入口，本表仅导航。

既有来源核查基线为 `b442bf9350832825f6ba98eea2b9865a669cd22b`（2026-11-03）；首局内容与交付规格按其自身来源和适用范围读取。本轮阅读下列关键章节及直接相关的产品条款、专业设计、静态素材样本和验证方法，未逐篇审读全仓、逐项检查素材或执行游戏。下列“未核实/未专审”仅表示该阅读范围的边界；原条款中的目标、实现状态与适用条件继续有效，静态文件和方法存在不证明当前候选通过。

| 领域 | 所属产品模块 | 现有权威条款或素材入口 | 已核对覆盖边界 |
| --- | --- | --- | --- |
| GD-01 定位与体验目标 | 世界规则与玩法系统 | [产品承诺与玩家位置](world-rules-core-gameplay/prd.md#1-产品承诺)、[当前体验切线](../game/gameplay/gameplay-top-level-design.prd.md#当前交付切线) | 已明确题材、间接控制身份、渐进理解与受控首局目标；其他入口组合及总体体验须按实际候选分别核验 |
| GD-02 循环与玩家旅程 | 世界规则与玩法系统 | [首局具体样本](world-rules-core-gameplay/first-session-and-continuation.prd.md#first-session-concrete-sample)、[首局体验设计](world-rules-core-gameplay/first-session-and-continuation.design.md#des-first-concrete-sample)、[首局内容交付](../world-simulator/viewer/first-session-content-delivery.design.md#content-units) | 首产物、后继候选、自然离场与回访已有具体设计；经营分支、Agent 主体感和持续能力仍须各自成立的方案与体验证据 |
| GD-03 玩法与系统组合 | 世界规则与玩法系统 | [首局适用范围](world-rules-core-gameplay/first-session-and-continuation.prd.md#starter-profile-applicability)、[生产与交付边界](world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-004)、[换向与待决结果](world-rules-core-gameplay/first-session-and-continuation.prd.md#goal-redirection-pending-and-settled-boundary) | 已明确工业首局的适用前置、完成层级和换向处置；全系统组合及全部入口、取消、中断与恢复条件未在本范围逐项核实 |
| GD-04 数值、经济与成长 | 世界规则与玩法系统 | [资源来源与消耗](world-rules-core-gameplay/prd.md#resource-model-and-cross-module-provenance)、[Agent 制造目标政策](../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#agent-manufacturing-v1)、[首局参数来源](../world-simulator/viewer/first-session-content-delivery.design.md#content-source-boundary) | 已有资源来源、消耗与所有权边界；制造目标参数和首局参数来源须分别标识，不能代签整体经济平衡或长期成长实测 |
| GD-05 世界观与叙事 | 世界规则与玩法系统 | [游戏前提](world-rules-core-gameplay/prd.md#1-产品承诺)、[V50 小说与配套资料](../../site/story/README.md#配套资料)、[小说背景使用边界](../../site/story/background/world-background.md#7-使用边界) | 已有产品题材与小说、人物和背景资料；可玩事件、任务、对话的适用范围及世界事实映射未核实，小说不代签可玩叙事 |
| GD-06 地图、空间与生成 | 世界规则与玩法系统；智能体、世界模拟与交互 | [四个场景的具体内容](../world-simulator/scenario/asteroid-scenario-content-delivery.design.md#scenario-content-catalog)、[实际消费者差异](../world-simulator/scenario/asteroid-scenario-content-delivery.design.md#consumer-content-boundary)、[生成与消费者合同](../world-simulator/scenario/unified-world-seed-fragment-runtime.prd.md#uws-consumer-boundaries)、[正式生成未决范围](../world-simulator/scenario/unified-world-seed-fragment-runtime.design.md#unified-decision-profile) | 已补四个现有碎片场景的有效输入、组成、出生与交付；Simulator 初态、Runtime bootstrap 和正式世界分别核验。正式 seed、starter、业务 record 与消费者绑定，以及分布和不可用开局的质量验证仍须分别核清 |
| GD-07 内容目录与初始配置 | 世界规则与玩法系统 | [native 工业内容与初态](../game/gameplay/industrial-native-content-delivery.design.md#native-content-catalog)、[内置配方来源](../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)、[内置工厂来源](../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs) | 已补 11 个 native 配方、2 类工厂的内容关系与 starter/有效初态来源；目录不代表世界实例、已安装或已获资格，其他 WASM 与完整游戏内容仍按各自范围核对 |
| GD-08 操作、UI 与反馈 | 智能体、世界模拟与交互；玩家接入与发行 | [UI/UX 交付](../world-simulator/viewer/viewer-page-module-design-2026-06-18.design.md#detailed-uiux-handoff)、[首局内容与反馈](../world-simulator/viewer/first-session-content-delivery.design.md#content-units)、[静态稿来源目录](../world-simulator/viewer/assets/ui-ux/full-function/coverage-catalog.json) | 已有流程、状态、文案与可编辑静态稿；目录保留其固定来源版本，静态设计不能代签当前运行、真实交互或各平台可用性 |
| GD-09 美术、动画与特效 | 智能体、世界模拟与交互 | [视觉语言](../world-simulator/viewer/viewer-visual-design-spec-2026-06-05.design.md#7-visual-language)、[世界分层](../world-simulator/viewer/viewer-visual-design-spec-2026-06-05.design.md#9-pixel-world-layering)、[世界视觉内容与交付](../world-simulator/viewer/viewer-world-visual-content-delivery.design.md#visual-content-catalog)、[制作验收方法](../world-simulator/viewer/viewer-world-visual-content-delivery.manual.md) | 已补当前程序化材质、热点、模块标记与 Micro Depot 的具体变体及交接；复用原图层、LOD、动效和场景。实际候选品质、完整模型、动画、特效与正式六视图资产消费仍分别核验 |
| GD-10 音乐、音效与语音 | 智能体、世界模拟与交互 | [Viewer 音频覆盖边界](../world-simulator/viewer/viewer-page-module-design-2026-06-18.design.md#uiux-module-coverage) | 已审读的 Viewer authority 未确认音量/音效设置；各表面的声音适用范围、事件与混音、设置和替代表达待核，不据此推断没有实现 |
| GD-11 本地化 | 智能体、世界模拟与交互；玩家接入与发行 | [语言与帮助内容](../world-simulator/viewer/viewer-localization-and-context-help.design.md)、[品牌短标签目标](../world-simulator/viewer/viewer-brand-system-2026-06-05.design.md#25-bilingual-terminal-copy-matrix-target)、[逐语言验证](../world-simulator/viewer/viewer-localization-and-context-help.manual.md) | 已补 Viewer 中英术语、上下文帮助、字符串来源及翻译/字体交接；各入口沿自己的语言规则，运行采用、逐语言体验与全游戏其他内容仍分别核验 |
| GD-12 无障碍与帮助 | 智能体、世界模拟与交互；玩家接入与发行 | [可访问性](../world-simulator/viewer/viewer-visual-design-spec-2026-06-05.design.md#12-accessibility-and-robustness)、[上下文帮助](../world-simulator/viewer/viewer-localization-and-context-help.design.md)、[操作验证](../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility) | 已有文字、非颜色反馈、键盘/触控、IME 与减弱动效要求；Viewer 六类帮助补具体正文及验收，实际入口、焦点/缺字形与真人理解另取证，完整游戏帮助仍未全量核定 |
| GD-13 多人、社交与冲突 | 世界规则与玩法系统 | [沟通与合同](world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#2-范围与玩家边界)、[组织连续性](world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#1-产品承诺)、[冲突范围](world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#2-宣战参战范围与离线防御) | 已核非绑定沟通、合同/争端、组织保护与退出、参战和离线防御边界；战损恢复比较、占领/提取仍有专业设计未核实项，骚扰举报与处置未专审 |
| GD-14 NPC 与 AI Agent | 智能体、世界模拟与交互；世界规则与玩法系统 | [Runtime 角色与范围](agents-world-simulation/external-agent-runtime-play.prd.md)、[目标与干预](world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)、[预算边界](agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-009) | 已核自有/受托关系、观察与决策、目标干预、预算及停止恢复要求；反馈进入认知、观战独立与私有上下文恢复仍有部分设计，独立 NPC 范围未专审，各 Runtime 分别取证 |
| GD-15 持久世界与规则演化 | 权威世界基础设施；世界规则与玩法系统；智能体、世界模拟与交互 | [同世界恢复](world-infrastructure/prd.md#world-infrastructure-current-stage)、[组织休眠](world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md)、[规则与历史版本](world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) | 已核身份/历史保留、只读到可服务恢复、组织逻辑时间与保护、升级重验及原版本回放；激活证明、跨版本替代和跨域恢复仍按原条款保留部分覆盖 |
| GD-16 UGC、Mod 与规则扩展 | 世界规则与玩法系统；权威世界基础设施 | [受治理扩展](world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)、[玩家发布范围](../world-runtime/module/player-published-entities.prd.md)、[有限回滚](../world-runtime/module/player-published-entities.design.md#ppe-rollback-disposition) | 已核 WASM 与产品/配方/工厂 profile 发布、权限重验及有限回滚；该发布专题明确排除游戏内 IDE 与美术分发管线，一般期限/收缩/原子替代及退出尚未完整覆盖 |
| GD-17 技术与体验预算 | 权威世界基础设施；智能体、世界模拟与交互；玩家接入与发行 | [基础设施合同](world-infrastructure/prd.md)、[Viewer 质量与容量](../world-simulator/viewer/external-agent-world-view.design.md#view-quality)、[入口响应](player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001) | 已核权威/恢复边界与容量、冷暖启动、响应、缓存、长稳目标和测法；各入口/设备与跨域容量按适用范围验证，本轮无当前候选性能结论 |
| GD-18 资产生产、来源与权利 | 智能体、世界模拟与交互；玩家接入与发行 | [静态素材目录](../world-simulator/viewer/assets/ui-ux/full-function/coverage-catalog.json)、[体素资产合同](../../tools/voxel_asset_pipeline/DESIGN.md#asset-contract)、[首局交付规格](../world-simulator/viewer/first-session-content-delivery.design.md#delivery-index)、[程序化内容源与交付](../world-simulator/viewer/viewer-world-visual-content-delivery.design.md#content-source-delivery) | 已补四类程序化内容的编辑源、WASM 构建与加载、候选绑定和替换交接，区分仓库许可、字体依赖与未来外部素材；全资产源文件、版本、许可、署名、复用权利及规模化产能仍未逐项核定 |
| GD-19 试玩与质量验证 | 世界规则与玩法系统；权威世界基础设施；智能体、世界模拟与交互；玩家接入与发行 | [证据层级与比较单位](world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002)、[独立试玩方法](../testing/manual/agent-runtime-self-playtest.manual.md)、[首局内容验收](../world-simulator/viewer/first-session-content-delivery.manual.md)、[首局真人研究](../testing/manual/first-session-human-playtest.manual.md) | 已补受控首局的筛选、双会话任务与主持、提示规则、原始记录和分项分析方法；自动化、Agent 与真人证据保持各自范围，实际研究、自然继续行为、真实 Runtime 与留存仍分别取证 |
| GD-20 制作范围与成本 | 世界规则与玩法系统；权威世界基础设施；智能体、世界模拟与交互；玩家接入与发行 | [首局内容制作范围](../world-simulator/viewer/first-session-content-delivery.design.md#delivery-handoff)、[工业内容变更与测量](../game/gameplay/industrial-native-content-delivery.design.md#production-measurement) | 首局和 native 工业内容已有可计数范围、制作依赖与变更测量方法；整游戏规模、工时、人员/外包与预算仍未核定，测量方法不代替实测产能，排期与分派仍归 Task |
| GD-21 商业与发行 | 玩家接入与发行 | [免费进入与可选服务](player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-free-001)、[访问与候选发行](player-entry-distribution/access-modes-and-release-readiness.prd.md)、[公开口径](player-entry-distribution/release-communications-and-public-claims.prd.md#public-claim-four-layer-model) | 已核基础进入目标、便利服务不购买世界权力、正式入口与同候选发行边界；具体渠道/地区的分级、协议与隐私材料未专审，不据目标推导服务上线或发行就绪 |
| GD-22 运营、社区与支持 | 玩家接入与发行 | [Preview 反馈要求](player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-preview-feedback-001)、[玩家支持与服务生命周期](../readme/governance/player-support-and-service-lifecycle.design.md)、[执行手册](../readme/governance/player-support-and-service-lifecycle.manual.md)、[事故与纠正](player-entry-distribution/release-communications-and-public-claims.prd.md#ac-entry-claim-002) | 已补受控测试反馈、分流复核、数据告知与误传处置、事故/暂停/结束文案及验收方法；实际入口采用与执行仍需取证。完整私密客服、社区治理、统一保留/删除政策、持续支持成本和正式全服务结束决定仍未核实，不承诺逐条人工答复 |

上述行是来源导航与有界核查结果，不是全游戏完成清单。领域要求、入口、素材或适用范围变化时，只更新受影响行及原权威；具体设计/实现状态继续按功能条款维护，阶段评审须核对当时实际范围与证据。

每个产品模块以主 PRD 为权威入口，但不限于单个文件；可以按长期稳定的产品主题建立专题分册，形成“模块入口 → 主 PRD → 专题分册”的文档树。专题分册必须由模块入口可达并回链主 PRD，不得按日期或短期小功能拆成设计碎片。

外部 Runtime 游玩从[智能体模块根 PRD](agents-world-simulation/prd.md#external-runtime-play)进入[外部 Agent Runtime 接入与持续游玩](agents-world-simulation/external-agent-runtime-play.prd.md)：产品分册拥有 OpenClaw、Codex 等 Runtime 的目标、首期范围与组合验收，[配对设计](agents-world-simulation/external-agent-runtime-play.design.md)承接用户流程。具体协议、适配和能力证据下钻到 [LLM/provider 专业合同](../world-simulator/llm/README.md)，文档目标不提升当前发行或支持状态。

迁移按文件逐个进行：先判定其中哪些内容属于产品承诺、产品设计、跨域组合或端到端验收，并将这些语义回填到对应产品模块；专业规则、实现合同、技术 How、工程实现、测试/运维和任务证据仍留在专业域并由产品文档链接。迁移治理应以“语义完整归位并删除源文件”为默认目标，而不是只新增产品文档、继续保留重复或日期化的旧入口。迁移前后均须保留模块入口与专题回链，修复所有活跃引用；仅当产品语义已完整回填、专业域权威未丢失且活跃引用已修复时，才可删除原文件。若源文件仍承载尚未迁移的专业真值而必须暂时保留，须明确记录剩余语义、目标权威与后续删除条件，将其作为迁移债务继续治理。完成条件是读者可从四大模块入口到达产品真值，并能继续下钻到对应专业域权威和验证证据。

## 产品树完整性与迁移验收

本节是产品层自身的结构合同；它约束产品语义如何归位，不替代专业域规则、实现、测试或任务生命周期。

### 结构规则与边界

- `doc/product/README.md` 是唯一产品入口，且只承认上表四个模块。`core`、`game`、`world-runtime`、`world-simulator`、`p2p`、`testing`、`engineering` 与 `site` 是专业或治理域，不得通过新增产品入口变成第五个产品模块。
- 每个模块根 PRD 负责该模块的产品承诺、组合关系、Non-Goals 与跨域验收；长期专题必须由模块根 PRD 的“活跃产品专题”可达并回链根 PRD。专题不得声明模块根的保留身份元数据或另立产品总入口。
- 稳定的玩家/产品语义进入对应产品模块；专业规则、实现合同、技术 How、工程实现、测试/运维和任务证据留在其专业权威，产品文档只链接并说明组合边界。产品文档不得复制可变任务状态、review ledger 或发布证据。
- `superseded` / `retired` 文档是迁移债务或历史引用，不是 active authority、路线图或验收入口。源文件只有在语义完整回填、专业权威仍可达、活跃引用修复后才能删除；暂存文件必须记录剩余语义、接收 owner 与删除条件，不能以重复入口长期替代迁移。

### Done：可验证验收

- **PD-1 四模块闭合**：入口表恰好列出四个模块根 PRD，四个根 PRD 均能回链本页；不存在由别的目录或日期化文件承载的第五个产品入口。
- **PD-2 专题可达**：每个 `active` 专题都能从所属根 PRD 的“活跃产品专题”到达并回链；专题只声明自身生命周期与专业权威，不伪造模块根身份。
- **PD-3 迁移可判定**：每个被吸收的来源语义都能在接收模块或其明确链接的专业权威中定位；尚未完成的来源保留 `superseded` / `retired`、剩余语义、目标 owner 和删除条件，不能继续作为 active authority。
- **PD-4 权威边界可追踪**：模块根 PRD 的每条成功标准都能追踪到 owner、专业 PRD-ID、权威文档、验证证据和测试层级；产品层不把局部专业 green 或任务状态当作产品完成。
- **PD-5 检查边界诚实**：对产品树运行 `./scripts/doc-governance-check.sh`、`python3 scripts/product-doc-governance-check.test.py` 必须通过，且 `git diff --check` 无输出；这些自动检查只证明已枚举的结构合同（四行入口清单、根 PRD 身份/生命周期/声明的 authority backlink、专题声明、成功标准—追踪行、保留元数据/ledger/path/行数约束）成立，不证明语义迁移完整、所有通用专业 authority 链接或根 Markdown backlink 完整，也不证明 PD-1–PD-4 的语义内容和 traceability 值正确；后者必须由人工/对应专业角色复核。

### Non-Goals

- 不在本页定义玩法数值、runtime/WASM/Agent/Viewer 实现、测试步骤、运维 runbook、渠道文案或发布 verdict。
- 不要求一次性删除所有历史或专业来源；未完成迁移的语义应按上述迁移债务规则保留，直到接收与引用修复可被验证。
- 不把本页的结构检查当作产品可玩、发布就绪或线上安全证明；这些结论仍由对应专业 authority、QA 与公开 claim 边界共同决定。

如果需要从产品承诺继续下钻到规则、实现契约或验证证据，按下表进入专业域权威：

- 世界规则与玩法系统 → [`doc/game/prd.md`](../game/prd.md)
- 权威世界基础设施 → [`doc/p2p/prd.md`](../p2p/prd.md)、[`doc/world-runtime/prd.md`](../world-runtime/prd.md)、[`doc/testing/prd.md`](../testing/prd.md)
- 智能体、世界模拟与交互 → [`doc/world-simulator/prd.md`](../world-simulator/prd.md)
- 玩家接入与发行 → [根 `README.md`](../../README.md)、[`doc/world-simulator/prd.md`](../world-simulator/prd.md)

专业规则、实现合同、专业 PRD-ID 和测试机制由对应专业模块的 PRD 承载；技术 How 与工程实现由 design 承载；测试/运维由对应专业文档承载；任务说明可以直接使用 PR；实际版本、状态与过程证据由 Git、PR、实际 CI 与评审记录承载，Issue/Project 按需使用。
