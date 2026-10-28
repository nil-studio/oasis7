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
