# 外部 Agent Runtime 接入与持续游玩

## 文档身份

- 所属产品模块：智能体、世界模拟与交互
- 上位产品 PRD：[prd.md](prd.md)
- 配对产品 design：[external-agent-runtime-play.design.md](external-agent-runtime-play.design.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- 专业域权威：[Decision Provider](../../world-simulator/llm/decision-provider-contract.prd.md)、[Continuous Agent Harness](../../world-simulator/llm/continuous-agent-harness.prd.md)、[Runtime cognition lifecycle](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md)
- Last reviewed：2026-10-10
- 适用入口：外部 Runtime 接入后的 Agent 决策与持续游玩；玩家通过现行 `viewer` / `pure_api` 入口观察和间接引导。

本文拥有“用户带现有 Agent Runtime 来玩 oasis7”的端到端产品目标、首期范围与组合验收。`active` 表示本文是有效产品要求，不表示 OpenClaw、Codex 或任一适配组合已可用、通过验收或获准公开发行。协议、适配实现、世界结算和具体支持证据仍由专业域拥有。

## 设计适用性与生命周期闭合

- 设计判定：`paired-design`。
- 配对关系：[同名产品设计](external-agent-runtime-play.design.md)承接首次接入、委托运行、结果阅读、目标调整和恢复的体验设计；本 PRD 拥有产品要求和验收。
- 设计适用性理由：本主题跨越 Runtime、游戏 Agent、玩家入口和世界结果，需要共同的信息层级与失败解释；具体 API、CLI 参数和存储算法留在专业设计。

## 1. 产品目标

### 1.1 目标与代表性情境

用户可以把自己使用的 OpenClaw、Codex 等 Agent Runtime 接入 oasis7，在高层目标、有效委托和已说明的预算范围内，驱动合法绑定的游戏 Agent 自主、持续地推进游戏任务。玩家还需能通过**网页和桌面客户端可交互地看大世界**：浏览缩放、搜索定位 Agent/设施、查看活动与权威成果，并在现场合法地指导自己的 Agent。oasis7 提供受约束的世界观察、可理解的合法能力、权威行动结果和继续游玩的依据。**首期默认体验是：为外部 Runtime 提供可验证来源的游戏 Skill，并由 Runtime 主动通过标准游戏网络 API 获取观察、发现能力、提交意图和追读权威结果**；不要求世界反过来轮询某个专有 Provider Bridge 才能让 Agent 玩游戏。

代表性情境：玩家已有一个 Runtime，希望让自己的 Agent 建立第一项工业成果。他确认连接目标、Agent 资格、可玩范围与开销，给出生产目标并开始委托；Runtime 自行理解处境、规划和执行。玩家能看到已经发生的世界成果、主要阻塞和下一步，可以调整高层目标，也可以离开观战界面后再回来继续。

本主题的成功由“接入、理解、行动、反馈、继续”这一完整经历判断。接通模型、返回合法 JSON、出现一次动作或通过低频 NPC 测试，分别只证明其自身范围。

### 1.2 角色与职责

| 对象 | 拥有的职责 | 约束与组合关系 |
| --- | --- | --- |
| 人类玩家 | 选择 Runtime、确认身份与委托、提供高层目标和提示词、观察与纠正策略 | 间接控制与严重后果授权沿用[Agent 自治与委托](agent-authority-ownership-and-accountability.prd.md#agent-delegation-boundary)。 |
| 外部 Agent Runtime | 运用自身推理、规划、会话记忆和工具编排能力，形成候选行动并利用反馈继续决策 | OpenClaw、Codex 是首批适配与验收目标。内部工具或子任务不自动成为游戏实体，也不获得额外世界权限。 |
| 游戏 Skill（引导层） | 教会支持的外部 Runtime 如何识别正确世界、核对身份、发现能力和安全使用游戏 API | Skill 是有来源、版本和适用范围的说明与可选适配资产，不拥有游戏权限、不保存凭据、不另造行动规范。 |
| 游戏网络 API（对外能力层） | 向获授权的外部 Runtime 提供游戏观察、能力发现、读写调用、待决查询、权威回执及恢复能力 | 统一使用世界 Runtime 的认证授权与裁决结果；这是外部自主游玩的首期主入口，不是 Launcher/operator 管理接口。 |
| oasis7 的 Agent 接入层与 Harness | 将合法观察、目标、能力和反馈接入同一 Agent 会话，维持世界相关认知上下文的边界与连续性 | [Harness 合同](../../world-simulator/llm/continuous-agent-harness.prd.md#3-权威边界)拥有生命周期与策略约束；Runtime 内部会话能力和 Harness 职责须能配合，不重复宣称世界权威。 |
| 游戏 Agent | 作为世界中被认领、维护和授权的实体承担行动及后果 | 取得与维护消费[Agent 所有权与持续经营](../world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#1-产品承诺)；接入软件不赠送、复制或重新认领实体。 |
| 世界 Runtime | 校验权限、规则与前置条件，裁决和提交行动，提供权威结果与历史 | 外部 Runtime、模型输出、工具日志和玩家界面均不能自行确认世界状态已改变。 |
| 模型服务 / Token Bridge | 提供或转接推理能力及其计量、账户服务 | 模型账户、连接认证和游戏 Agent 委托分别成立；更换模型端点不自动证明已支持某个 Agent Runtime。 |

### 1.3 依据、目标与现状

- 上位产品承诺由[模块根 PRD 的外部 Runtime 目标与 SC-14](prd.md#external-runtime-play)承载；首局成果消费[首局与持续游玩](../world-rules-core-gameplay/first-session-and-continuation.prd.md#1-产品目标)。
- 已有专业合同包括 Decision Provider、Local Provider HTTP、持续 Harness、执行 lane 和 parity；它们各自的 `current` / `partial` / `target` 与证据范围继续由原文维护。
- 本文冻结首批 Runtime 与体验验收目标，不据此上调当前支持矩阵、默认选择或公开状态。当前公开口径仍从[根 README](../../../README.md)进入。
- 设计假设：用户希望保留熟悉的 Runtime 使用方式，并以世界成果和可理解的恢复判断可玩性。体验有效性需要真实使用者验证，不能由结构检查或 Agent 自报完成推出。

## 2. 范围与阶段

### 2.1 首批 Runtime 与支持单位

OpenClaw 和 Codex 分别作为首批目标验收。每个支持结论都绑定 Runtime 及版本、适配方式、模型/profile、世界候选、玩家入口、执行 lane、能力范围与验证窗口。一个 Runtime 的通过不会自动覆盖另一个；通用协议存在或可配置可执行文件名，也不能代替适配与真实会话证据。

接入文档应给出每个受支持组合的一条可复现推荐路径，说明凭据归属、连接与启动方式、就绪判定、恢复方式及限制。首期**推荐且必须可独立验收**的是“官方 Skill → 受授权的、版本化的游戏 HTTP(S)/JSON API → 权威世界结果”。Skill 是外部 Runtime 的使用说明和安全入口，Game API 是可由普通网络客户端直接调用的稳定能力通道；MCP、CLI 或其他包装层可以消费同一 API，却不是必需的第二套世界规则或首局前置。网络路由形状、认证机制和状态 schema 由后续专业系统设计决定。

### 2.2 阶段目标

| 阶段 | 要证明的结果 | 适用边界 |
| --- | --- | --- |
| 契约与适配验证 | 官方 Skill 可被目标 Runtime 消费，标准游戏 API 完成受权观察、能力发现、提交、回执与错误恢复 | 既有 P0 低频 NPC profile、mock、loopback 与 smoke 仅保留其协议及局部行为证据；不作为新首期产品默认接入路径。 |
| 外部 Runtime 首局与大世界可视化 | 两个真实 Runtime 分别完成首产物、目标更新与恢复；玩家另在网页和受支持桌面客户端各自可浏览大世界、定位 Agent、查看世界结果并合法指导 | 组合验收：Runtime 任务与可视化两入口分别留证，见 [AC-EXT-010](#ac-ext-010)、[AC-EXT-015](#ac-ext-015)–[AC-EXT-017](#ac-ext-017)。 |
| 持续游玩与扩面 | 更长任务、多 Agent、更多受治理玩法、更多 Runtime 及经声明的切换组合 | 按能力逐项取证；模型训练、认证制度和情报机制沿用[长期专题](provider-learning-intelligence-and-cadence.prd.md#2-范围与玩家边界)。 |

首期复用玩法主责当前选定的 `starter-industrial-smelter-to-assembler-v1` / `production_only` 代表链，成果边界消费[首局主责](../world-rules-core-gameplay/first-session-and-continuation.prd.md)与[工业结算合同](../../game/gameplay/gameplay-industrial-starter-completion-contract.prd.md)。适配需要覆盖这条链实际所需的合法能力；具体资源、配方、建造条件、结算和后续候选均不在本文另行定义。首产物、经营恢复、Runtime 恢复和玩家回访分别判定，首产物不自动证明稳定生产、交付或需求满足。

### 2.2.1 产品功能清单（Feature Catalog）

清单各功能的设计/实现状态及适用 Trace 由本行链接的 REQ 状态摘要拥有；同一要求下的功能共同消费该范围，不以某个子功能完成代签其余义务。后续候选仍不是首期承诺。

本清单是 §1 产品目标与 §4 产品要求之间的**稳定功能范围索引**，按[产品文档功能清单维护原则](../../engineering/doc-governance/product-documentation-standard.design.md#product-feature-catalog-principles)更新。**阶段不是实现状态**：标为“首期闭环”的功能是本文承诺要验收的组成部分，不代表当前已交付；每项由链接的 REQ/AC 判定，版本、支持矩阵和实际进度沿专业 authority 与 GitHub 记录查询。当前仅列用户或外部 Runtime 能实际感知的能力，不将 HTTP/MCP/CLI、API 字段、辅助脚本或测试步骤定为产品功能。

首期范围共 **62 项，分成十一组**（原有 53 项 + 新增大世界可视化/操作 9 项）。不同条目可共同满足一组 REQ/AC；单个 REQ/AC 下的负例和恢复边界仍由其完整正文约束，不能只检查功能名称。

#### 接入与 Runtime 身份

| 功能 ID | 功能项 | 玩家或外部 Runtime 可观察的结果 | 阶段 | 验收主责 |
| --- | --- | --- | --- | --- |
| EXT-F-001 | 选择可声明的 Runtime 组合 | 识别 OpenClaw/Codex 对应的适配方式、版本与适用范围，不把模型连通等同 Runtime 接入 | 首期闭环 | [REQ-EXT-001](#req-ext-001) / [AC-EXT-001](#ac-ext-001) |
| EXT-F-002 | 可复现的安装与连接引导 | 了解必要依赖、连接对象、启动条件、凭据归属及未就绪时的修复路径 | 首期闭环 | [REQ-EXT-001](#req-ext-001) / [AC-EXT-001](#ac-ext-001) |
| EXT-F-003 | 连接和会话就绪确认 | 在委托前核对实际承担推理/编排的 Runtime 会话，而非仅确认程序存在或模型响应 | 首期闭环 | [REQ-EXT-001](#req-ext-001) / [AC-EXT-001](#ac-ext-001) |
| EXT-F-004 | 能力/版本不兼容诊断 | 得到缺失能力或不兼容原因、可选修复与明确停止路径，不静默换用其他执行方 | 首期闭环 | [REQ-EXT-001](#req-ext-001) / [AC-EXT-001](#ac-ext-001) |
| EXT-F-005 | 实际 Runtime 与配置可核对 | 在适用诊断层核对当前 Runtime、版本、适配与模型/profile，避免误认为使用了别的执行方 | 首期闭环 | [REQ-EXT-001](#req-ext-001) / [AC-EXT-001](#ac-ext-001) |

#### 游戏身份、绑定与委托

| 功能 ID | 功能项 | 玩家或外部 Runtime 可观察的结果 | 阶段 | 验收主责 |
| --- | --- | --- | --- | --- |
| EXT-F-006 | 合法 Agent 选择与绑定 | 只选择当前身份已绑定、认领或获有效委托的游戏 Agent | 首期闭环 | [REQ-EXT-002](#req-ext-002) / [AC-EXT-002](#ac-ext-002) |
| EXT-F-007 | 认领与维护前置指引 | 未持有可控制 Agent 时转向现有认领、比较候选或观察路径；接入不赠送游戏资产 | 首期闭环 | [REQ-EXT-002](#req-ext-002) / [AC-EXT-002](#ac-ext-002) |
| EXT-F-008 | 启动前的高层委托确认 | 展示目标 Agent、授权范围、持续条件与需玩家决定的高后果承诺 | 首期闭环 | [REQ-EXT-002](#req-ext-002) / [AC-EXT-002](#ac-ext-002) |
| EXT-F-009 | 资格变化与撤销生效 | 权限到期、转让、撤销或主体变化后停止产生新的越权行动，不抹去已发生结果 | 首期闭环 | [REQ-EXT-002](#req-ext-002) / [AC-EXT-002](#ac-ext-002) |

#### 世界观察与能力发现

| 功能 ID | 功能项 | 玩家或外部 Runtime 可观察的结果 | 阶段 | 验收主责 |
| --- | --- | --- | --- | --- |
| EXT-F-010 | 受授权约束的世界观察 | 获取 Agent 自身、相关环境、任务进展和资源约束，不泄露其他 lane 的私有信息 | 首期闭环 | [REQ-EXT-003](#req-ext-003) / [AC-EXT-003](#ac-ext-003) |
| EXT-F-011 | 观察的新鲜度与不确定性 | 区分可信当前事实、陈旧/不完整情报及下一次观察或复核 | 首期闭环 | [REQ-EXT-003](#req-ext-003) / [AC-EXT-003](#ac-ext-003) |
| EXT-F-012 | 可用行动及输入条件发现 | 在选择行动前知道当前可调用的动作、必要输入及不可用原因；公布与实际解析保持一致 | 首期闭环 | [REQ-EXT-003](#req-ext-003) / [AC-EXT-003](#ac-ext-003) |
| EXT-F-013 | 只读查询与目标检查 | 使用合法查询理解对象、条件和状态，而不把查询/计划视为已改变世界 | 首期闭环 | [REQ-EXT-003](#req-ext-003) / [AC-EXT-003](#ac-ext-003) |
| EXT-F-014 | 状态变化后的能力重判 | 前置条件、资格或资源漂移时得到可解释的限制、拒绝及安全重新观察路径 | 首期闭环 | [REQ-EXT-003](#req-ext-003) / [AC-EXT-003](#ac-ext-003) |

#### 自主决策与权威反馈

| 功能 ID | 功能项 | 玩家或外部 Runtime 可观察的结果 | 阶段 | 验收主责 |
| --- | --- | --- | --- | --- |
| EXT-F-015 | 首局必要动作的真实执行路径 | Runtime 能调用现行首局工业链实际所需的合法动作并交给世界权威裁决 | 首期闭环 | [REQ-EXT-003](#req-ext-003) / [AC-EXT-003](#ac-ext-003) |
| EXT-F-016 | 多轮自主规划与执行 | 依据目标和前轮结果自行选择后续行动，玩家无需每一步重新下达指令 | 首期闭环 | [REQ-EXT-004](#req-ext-004) / [AC-EXT-004](#ac-ext-004) |
| EXT-F-017 | 等待与后续继续 | 没有合法进展动作或等待世界条件时采用有界等待/恢复，避免盲目重试 | 首期闭环 | [REQ-EXT-004](#req-ext-004) / [AC-EXT-004](#ac-ext-004) |
| EXT-F-018 | 已结算成功反馈 | 接收与任务和 Agent 正确关联的权威成功结果，并用其决定下一步 | 首期闭环 | [REQ-EXT-005](#req-ext-005) / [AC-EXT-005](#ac-ext-005) |
| EXT-F-019 | 拒绝与无进展反馈 | 理解合法拒绝、失败或未取得进展的原因及可行的替代决策 | 首期闭环 | [REQ-EXT-005](#req-ext-005) / [AC-EXT-005](#ac-ext-005) |
| EXT-F-020 | 待决、超时与重复请求核对 | 未知状态先核对原提交；延迟回复、重复投递或重连不制造第二份世界效果 | 首期闭环 | [REQ-EXT-005](#req-ext-005) / [AC-EXT-005](#ac-ext-005) |

#### 玩家高层指导与任务连续性

| 功能 ID | 功能项 | 玩家或外部 Runtime 可观察的结果 | 阶段 | 验收主责 |
| --- | --- | --- | --- | --- |
| EXT-F-021 | 创建并查看持续目标 | 玩家能为合法 Agent 设定当前高层任务，并理解目标完成边界与已有进展 | 首期闭环 | [REQ-EXT-006](#req-ext-006) / [AC-EXT-006](#ac-ext-006) |
| EXT-F-022 | 调整提示词或目标 | 玩家修改策略指导，保留 Agent 在有效授权内自主选择动作的空间 | 首期闭环 | [REQ-EXT-006](#req-ext-006) / [AC-EXT-006](#ac-ext-006) |
| EXT-F-023 | 目标接受与应用状态 | 明确区分草稿、已接受、真正应用和阻塞，不以聊天回复冒充生效 | 首期闭环 | [REQ-EXT-006](#req-ext-006) / [AC-EXT-006](#ac-ext-006) |
| EXT-F-024 | 新旧目标与在途任务衔接 | 后续决策消费有效新目标，旧待决请求仍按原世界合同处理 | 首期闭环 | [REQ-EXT-006](#req-ext-006) / [AC-EXT-006](#ac-ext-006) |

#### 无界面执行与恢复

| 功能 ID | 功能项 | 玩家或外部 Runtime 可观察的结果 | 阶段 | 验收主责 |
| --- | --- | --- | --- | --- |
| EXT-F-025 | 无 GUI 执行路径 | 不依赖打开 Viewer 才能让外部 Runtime 持续决策 | 首期闭环 | [REQ-EXT-007](#req-ext-007) / [AC-EXT-007](#ac-ext-007) |
| EXT-F-026 | 离开观战后继续 | 在 Runtime、连接、授权及预算有效时继续；回访可查看离开期间的真实世界成果 | 首期闭环 | [REQ-EXT-007](#req-ext-007) / [AC-EXT-007](#ac-ext-007) |
| EXT-F-027 | 分层在线/暂停状态 | 分别显示玩家观战连接、Runtime 执行、Agent 委托和世界任务结果状态 | 首期闭环 | [REQ-EXT-007](#req-ext-007) / [AC-EXT-007](#ac-ext-007) |
| EXT-F-028 | 连接中断后的安全恢复 | 重新核对 Agent、有效目标、世界事实和未决请求，而非重放旧提交 | 首期闭环 | [REQ-EXT-008](#req-ext-008) / [AC-EXT-008](#ac-ext-008) |
| EXT-F-029 | Runtime 进程重启后的续接 | 重启后在同一世界与游戏 Agent 历史上继续，已结算结果不重复 | 首期闭环 | [REQ-EXT-008](#req-ext-008) / [AC-EXT-008](#ac-ext-008) |

#### 成本、安全与预算

| 功能 ID | 功能项 | 玩家或外部 Runtime 可观察的结果 | 阶段 | 验收主责 |
| --- | --- | --- | --- | --- |
| EXT-F-030 | 推理与世界资源费用分离 | 清楚区分外部模型/工具费用与游戏内真实资源/维护消耗 | 首期闭环 | [REQ-EXT-009](#req-ext-009) / [AC-EXT-009](#ac-ext-009) |
| EXT-F-031 | 可执行预算与未知开销说明 | 展示当前能强制限制的额度、估计或未知的下游开销，不虚构硬上限 | 首期闭环 | [REQ-EXT-009](#req-ext-009) / [AC-EXT-009](#ac-ext-009) |
| EXT-F-032 | 预算耗尽后的安全收口 | 限制生效或委托失效时停止新的受限请求，同时允许核对已有待决结果 | 首期闭环 | [REQ-EXT-009](#req-ext-009) / [AC-EXT-009](#ac-ext-009) |
| EXT-F-033 | 连接身份与费用承担保护 | 支付路由、公共标识与实际认证/Agent 授权分开，不能借可猜测路由冒用权限 | 首期闭环 | [REQ-EXT-002](#req-ext-002) / [AC-EXT-002](#ac-ext-002) |

#### 代表性可玩闭环与两种入口

| 功能 ID | 功能项 | 玩家或外部 Runtime 可观察的结果 | 阶段 | 验收主责 |
| --- | --- | --- | --- | --- |
| EXT-F-034 | 外部 Runtime 首项工业成果 | 沿既有 starter chain 完成合法生产，以匹配权威 production receipt 作为首产物边界 | 首期闭环 | [REQ-EXT-010](#req-ext-010) / [AC-EXT-010](#ac-ext-010) |
| EXT-F-035 | 真实阻塞后的自主续玩 | 生产或资源受阻时理解主 blocker，并在合法选项中修复、等待、减量或重新规划 | 首期闭环 | [REQ-EXT-010](#req-ext-010) / [AC-EXT-010](#ac-ext-010) |
| EXT-F-036 | Viewer 与 pure API 的同义反馈 | 按声明的玩家入口分别呈现或返回同一权威目标、结果、阻塞与下一步，不相互代签 | 首期闭环 | [REQ-EXT-007](#req-ext-007) / [AC-EXT-007](#ac-ext-007) |

#### 游戏 Skill 的发现、安装与使用

| 功能 ID | 功能项 | 玩家或外部 Runtime 可观察的结果 | 阶段 | 验收主责 |
| --- | --- | --- | --- | --- |
| EXT-F-043 | 可信的游戏 Skill 获取与来源核验 | 外部 Runtime 能取得有版本、适用范围与可验证发布来源的官方 Skill，不依赖搜索未知脚本 | 首期闭环 | [REQ-EXT-011](#req-ext-011) / [AC-EXT-011](#ac-ext-011) |
| EXT-F-044 | OpenClaw/Codex 分别有可复现的 Skill 接入 | 每个首批 Runtime 有被实际测试的安装、加载、连接与首次调用路径 | 首期闭环 | [REQ-EXT-011](#req-ext-011) / [AC-EXT-011](#ac-ext-011) |
| EXT-F-045 | Skill 引导发现游戏能力 | Skill 指导先确认身份和能力目录，再依据实际授权视角规划游戏，而非硬编码不存在的指令 | 首期闭环 | [REQ-EXT-011](#req-ext-011) / [AC-EXT-011](#ac-ext-011) |
| EXT-F-046 | Skill 不越权使用环境和秘密 | 不要求将长期密码、模型密钥或签名材料写进 Skill/Prompt；避免自动执行未审核高权限安装命令 | 首期闭环 | [REQ-EXT-013](#req-ext-013) / [AC-EXT-013](#ac-ext-013) |
| EXT-F-047 | Skill 与游戏版本兼容提示 | Skill/运行世界/API 版本不兼容时明确失败或升级，不把旧描述伪装成合法能力 | 首期闭环 | [REQ-EXT-011](#req-ext-011) / [AC-EXT-011](#ac-ext-011) |
| EXT-F-048 | Skill 级故障诊断与修复 | 解释连接、授权、能力、限流或世界阻塞的区别，并引导读取真实 API 结果 | 首期闭环 | [REQ-EXT-011](#req-ext-011) / [AC-EXT-011](#ac-ext-011) |

#### 可由外部 Runtime 主动调用的游戏网络 API

| 功能 ID | 功能项 | 玩家或外部 Runtime 可观察的结果 | 阶段 | 验收主责 |
| --- | --- | --- | --- | --- |
| EXT-F-049 | 游戏 API 发现与版本协商 | 普通网络客户端可确认 API 协议版本、目标世界及当前支持能力，版本不符明确失败 | 首期闭环 | [REQ-EXT-012](#req-ext-012) / [AC-EXT-012](#ac-ext-012) |
| EXT-F-050 | 身份认证与 Agent 范围授权 | 连接必须证明客户端身份，并仅可在显式委托的 Agent 与读写范围内使用 | 首期闭环 | [REQ-EXT-013](#req-ext-013) / [AC-EXT-013](#ac-ext-013) |
| EXT-F-051 | 游戏世界观察的网络读取 | 按目标 Agent 授权和执行 lane 读取真实世界状态、目标与最新可信事实 | 首期闭环 | [REQ-EXT-012](#req-ext-012) / [AC-EXT-012](#ac-ext-012) |
| EXT-F-052 | 只读查询与对象检查接口 | 外部 Runtime 可查询支持的对象及条件且不产生世界副作用 | 首期闭环 | [REQ-EXT-012](#req-ext-012) / [AC-EXT-012](#ac-ext-012) |
| EXT-F-053 | 状态感知的能力目录与输入语义 | 外部 Runtime 经统一版本化目录取得可用动作、参数要求、不可用原因和能力变动 | 首期闭环 | [REQ-EXT-012](#req-ext-012) / [AC-EXT-012](#ac-ext-012) |
| EXT-F-054 | 异步行动提交与受理反馈 | 合法 Agent 意图通过网络提交；返回受理/拒绝/待决，不把受理当结算 | 首期闭环 | [REQ-EXT-012](#req-ext-012) / [AC-EXT-012](#ac-ext-012) |
| EXT-F-055 | 权威回执与原请求状态查询 | 可按原请求/任务关系查询 pending、committed、rejected、failed 等实际结果和世界因果 | 首期闭环 | [REQ-EXT-012](#req-ext-012) / [AC-EXT-012](#ac-ext-012) |
| EXT-F-056 | 有界事件增量读取与继续 | 通过可恢复游标/增量查询（轮询为最低保证）得到后续结果和变化，不强迫永远打开 GUI | 首期闭环 | [REQ-EXT-012](#req-ext-012) / [AC-EXT-012](#ac-ext-012) |
| EXT-F-057 | 重试幂等与跨连接结果关联 | 网络超时后用同一意图身份核对/重试，防止多次世界效果或隐式重新授权 | 首期闭环 | [REQ-EXT-013](#req-ext-013) / [AC-EXT-013](#ac-ext-013) |
| EXT-F-058 | 结构化错误、限流及预算反馈 | 无权、过期、未发现能力、配额、过载和未知结算可判别，并给出安全重试/恢复建议 | 首期闭环 | [REQ-EXT-013](#req-ext-013) / [AC-EXT-013](#ac-ext-013) |
| EXT-F-059 | 本地和远程网络接入语义一致 | 按部署选择本地或受保护远程通道；均不能借 public identifier、路由标签或模型 Token 冒充游戏身份 | 首期闭环 | [REQ-EXT-013](#req-ext-013) / [AC-EXT-013](#ac-ext-013) |

#### 网页与桌面客户端大世界可视化

| 功能 ID | 功能项 | 玩家或外部 Runtime 可观察的结果 | 阶段 | 验收主责 |
| --- | --- | --- | --- | --- |
| EXT-F-062 | 网页世界舞台 | 浏览器内可交互地浏览真实大世界而不只是日志 | 首期闭环 | [REQ-EXT-015](#req-ext-015) / [AC-EXT-015](#ac-ext-015) |
| EXT-F-063 | 桌面客户端世界舞台 | 原生程序内可操作世界，可嵌入 Web 但不能只打开系统浏览器 | 首期闭环 | [REQ-EXT-015](#req-ext-015) / [AC-EXT-015](#ac-ext-015) |
| EXT-F-064 | 世界/区域概览 | 了解世界/区域/Fragment、当前活动和未知/未加载范围 | 首期闭环 | [REQ-EXT-016](#req-ext-016) / [AC-EXT-016](#ac-ext-016) |
| EXT-F-065 | 平移缩放与层级导航 | 从世界浏览至局部，返回选中 Agent 和主目标 | 首期闭环 | [REQ-EXT-016](#req-ext-016) / [AC-EXT-016](#ac-ext-016) |
| EXT-F-066 | Agent、地点与路线定位 | 搜索、定位或适用时跟随合法可见的 Agent/设施/路线 | 首期闭环 | [REQ-EXT-016](#req-ext-016) / [AC-EXT-016](#ac-ext-016) |
| EXT-F-067 | 对象选择与详情 | 查看有权对象的身份、位置/关系、状态和信息时效 | 首期闭环 | [REQ-EXT-016](#req-ext-016) / [AC-EXT-016](#ac-ext-016) |
| EXT-F-068 | 事件与工业活动观察 | 结合空间关系理解生产、Agent 活动与世界事件，不以动画代签成功 | 首期闭环 | [REQ-EXT-016](#req-ext-016) / [AC-EXT-016](#ac-ext-016) |
| EXT-F-069 | Agent 目标/回执与现场联动 | 选中 Agent 可读任务、blocker、pending/committed 与下一步 | 首期闭环 | [REQ-EXT-017](#req-ext-017) / [AC-EXT-017](#ac-ext-017) |
| EXT-F-070 | 地图中的合法高层指导 | 从自己 Agent 上下文进入目标/Prompt/委托，选择地图不直接执行行动 | 首期闭环 | [REQ-EXT-017](#req-ext-017) / [AC-EXT-017](#ac-ext-017) |

#### 后续扩面与长期候选（非首期承诺）

以下各项是方向性功能候选，不等于已确定的交付承诺、兼容性或支持声明；升入确定范围时必须按维护原则补足适用条件、正式 REQ/AC 与相应专业验证范围。

| 功能 ID | 候选功能 | 预期玩家价值与边界 | 阶段 | 上位方向或已有主责 |
| --- | --- | --- | --- | --- |
| EXT-F-037 | 更多 Agent Runtime 适配 | 更多受支持的 Runtime/版本组合，各自可复现与验证 | 后续扩面 | §2.2 扩面目标；新增组合必须单独取证 |
| EXT-F-038 | 更长周期的自主经营任务 | 多轮任务跨更多游戏阶段，保留可归因进展、修复与成本 | 后续扩面 | §2.2 持续游玩；需独立长期场景 AC |
| EXT-F-039 | 多个游戏 Agent 协同任务 | 多个被合法取得与委托的游戏 Agent 组织合作，不突破资产与授权边界 | 长期候选 | [Agent 自治与委托](agent-authority-ownership-and-accountability.prd.md) |
| EXT-F-040 | Runtime/provider 切换的任务连续性 | 切换只影响新意图，保证旧候选、待决和结算结果的边界 | 后续扩面 | [切换连续性](provider-agent-experience-continuity.prd.md) |
| EXT-F-041 | 长期记忆与可迁移任务知识 | 持久化已证实世界知识并在允许的范围内跨会话复用；不预设跨 Runtime 私有内存无损迁移 | 长期候选 | [Continuous Harness](../../world-simulator/llm/continuous-agent-harness.prd.md) |
| EXT-F-042 | 扩展工业之外的开放世界玩法 | 在专业玩法、世界能力和安全授权成立后参与协作、市场、WASM 等新体验 | 长期候选 | [世界规则与玩法系统](../world-rules-core-gameplay/prd.md) |
| EXT-F-060 | MCP 游戏工具适配 | 让支持 MCP 的 Runtime 通过同一游戏 API 使用动态能力与权威结果，不建立另一份权限或动作语义 | 后续扩面 | 本文 §2.5 与 [REQ-EXT-012](#req-ext-012)；独立适配验收 |
| EXT-F-061 | 轻量游戏 CLI/SDK 包装 | 为脚本/Codex 等提供便利包装，最终仍使用统一 API、授权、回执与错误语义 | 后续扩面 | 本文 §2.5 与 [REQ-EXT-012](#req-ext-012)；独立适配验收 |

清单的**完成口径**：首期每一功能（包括 Skill 与 Game API）的行为与失败边界必须能追踪到 REQ/AC，且 [AC-EXT-010](#ac-ext-010) 用真实 OpenClaw、Codex **分别**证明代表性首局。功能条目勾选、接口存在、模拟调用、一次成功或低频 NPC smoke 均不能单独代替结果与玩家体验证据。清单新增、合并、拆分、改名、变更阶段或退役时，在同一变更中同步其 REQ/AC、配对设计与专业引用；ID 不重用，功能实现任务和当前完成状态不写入此目录。

### 2.3 入口与运行边界

正式玩家访问模式沿用[玩家接入与发行](../player-entry-distribution/prd.md#玩家访问模式与证据边界)的 `viewer` / `pure_api`。网页与原生桌面客户端都是 `viewer` 的视觉使用表面，并非新玩家模式；各自需要实际交互证据。空间世界的产品主责是[玩家可读世界舞台](player-readable-world-stage.prd.md#world-exploration-surfaces)。`headless_agent` / `player_parity` 是执行或观察 lane；Runtime 类型、部署位置和连接方式不增加玩家访问模式。各 lane 的信息可见性沿用[双轨执行合同](../../world-simulator/llm/provider-agent-dual-mode.prd.md)。

无 GUI 是首期执行能力。关闭 Viewer 后能否继续，取决于实际运行的 Runtime、连接和独立委托仍有效；世界在玩家离开或 Runtime 停止后仍按自身规则推进。远程托管 Runtime 是一种部署选择，本目标不承诺替已关闭的本机 Runtime 提供后台推理服务。

### 2.4 Non-Goals

- 首期不交付任意 Runtime 即插即用、所有原生插件自动可用或跨 Runtime 私有记忆无损迁移；MCP/CLI 便利适配和流式推送可以后续独立交付，不能成为基础网络 API 首局可用性的隐藏前置。
- 首期不以未知 WASM 制度自动开发、完整市场/战争、多 Agent 协作或模型训练体系作为首局完成前置；对应长期要求继续由各自主责文档维护。
- 不重新定义世界规则、认领成本、固定权威节奏、资源计费公式或治理权限；不把外部推理更快解释成更多世界行动权。
- 不创建人类逐动作遥控游戏 Agent 的入口，不把观战、对话、目标草稿或 Runtime 的内部子任务当作已授权世界行动。
- 不在本产品文档中冻结协议字段、CLI 参数、认证算法、持久化格式、重试算法或发布 verdict。

### 2.5 Skill、网络 API 与旧 Provider Bridge 的产品路径决策

<a id="external-runtime-canonical-interface"></a>
**首期产品主路径**是由外部 Agent Runtime 主动调用游戏：`获取官方 Skill/加载适配说明 → 认证并绑定合法游戏 Agent → 发现世界观察及可用能力 → 只读查询/提交合法意图 → 查询权威状态或增量事件 → 在外部 Runtime 内继续规划`。可直接使用的版本化游戏 HTTP(S)/JSON API 必须独立于任何特定 LLM 服务、OpenClaw CLI、进程内模型和本地 Bridge；Skill、可选 MCP/CLI 包装均不拥有另一套游戏权威。外部 Runtime 的自循环、调度和可恢复会话由自身承担，oasis7 只承诺其可使用的世界能力、真实反馈及已定义的 Agent 授权连续性。

**Skill 的产品边界**：发行来源/版本、适用 Runtime 和游戏 API 范围可核对；说明只引导已授权的网络能力，不以静态描述覆盖当前能力目录。首次接入能在无需阅读仓库内部桥接脚本的情况下知道世界地址、认证要求、游戏 Agent、目标、支持能力和继续方式。Skill 中不能携带长期访问凭据、静默改变机器高权限环境、将用户模型密钥代作游戏认证，或诱导越过玩家授权。现有 [公开 Skill](../../../site/skills/oasis7.md) 主要描述旧 Local Provider Bridge 的操作路径，只是现状材料，不代表已经满足本目标。

**游戏网络 API 的产品边界**：必须具备可供普通 HTTP(S) 客户端独立调用的版本/能力发现、有效身份/Agent 授权下的只读观察与查询、状态感知合法动作、异步提交、原请求/receipt 查询、增量结果读取及结构化错误。至少支持可恢复的增量轮询；流式推送、WebSocket、MCP 和 CLI 不作为基础接口可玩的必要条件。网络重试、撤销、状态漂移、过载和预算耗尽均须安全且可诊断。业务认证不能从任意客户端填写的公开身份标签或模型服务 token 路由推出。具体 URL、方法、schema、限流数值、加密/签名与安全技术选型由专业系统设计负责。

**旧路径的命运**：已存在的 `provider_loopback_http` / `oasis7_provider_local_bridge` 是游戏向 Provider 发起决策调用的适配/实验路线，不是首期外部 Runtime 主动游戏 API；`crates/oasis7_agent_api` 的共享 DTO 也不等于可玩的网络服务。它们可以被复用、替换、停用或删除，不因为旧接口已存在就强制保留兼容层。若新路径端到端满足本文要求和世界运行时边界，则停止把旧 Provider Bridge 当作默认外部 Runtime 产品路径，移除旧公开引导和不再必要的桥接/脚本；删除前按实际依赖核对仍在使用的内置 Agent、历史世界成果、授权、其他专业合同与公开声明，不能顺手破坏这些独立能力。技术迁移方式与删除单元由后续系统设计裁定，不在本产品 PR 中执行代码废弃。

该选择不把直接 API 接通、官方 Skill 存在、某个 MCP 工具能调用或 P0 mock green 当成外部 Runtime 完整首局通过；必须按 [AC-EXT-010](#ac-ext-010)、[AC-EXT-011](#ac-ext-011) 至 [AC-EXT-014](#ac-ext-014) 独立验证。

## 3. 用户流程与关键决策

### 3.1 正常路径

| 阶段 | 玩家知道什么 | 可以选择什么 | 代价 / 承诺 | 可观察结果与下一步 |
| --- | --- | --- | --- | --- |
| 选择接入 | 官方 Skill 的来源/版本、目标世界 API 与适用 Runtime | 通过 Skill 完成必要配置、核对网络 API/身份或暂不开始 | 本地/远程连接与模型开销、授权范围可读 | Runtime 能主动读取获授权的真实世界能力，或返回具体缺项；尚未开始游戏委托。 |
| 绑定与委托 | 当前身份、可控制 Agent、目标及授权范围 | 绑定已取得的 Agent，按既有规则认领，或先观察 | 认领/维护由玩法主责；执行预算和委托范围分别说明 | 目标 Agent 与有效委托明确，才能开始对应任务。 |
| 自主推进 | 当前高层目标、阶段进展和主要阻塞 | 在有效授权内让 Runtime 自主推进；需要时调整高层策略 | 已发生与预计开销分开 | Runtime 持续读取观察、调用能力、接收结果并规划下一步。 |
| 阅读后果 | 哪些行动已结算、仍待决、被拒绝或没有进展 | 等待、补足条件、调整目标或使用受支持恢复路径 | 世界资源变化与推理费用分别归因 | 首局成果由权威结果确认，失败有适用下一步。 |
| 浏览大世界 | 当前已知世界、Agent、工厂与事件 | 通过 Web 或桌面客户端缩放、定位、选中检查、返回目标 | 观察不自动取得控制权 | 把 Agent 自主行动、位置和世界结果关联成可理解的体验。 |
| 调整与离开 | 新目标是否已应用；观战和执行是否分别在线 | 修改目标、停止后续委托，或保持授权离开 Viewer | 已提交行动按原权威规则处理 | 新决策消费有效目标；离开观战界面不被误报为停止执行。 |
| 恢复与继续 | 原世界成果、未完成义务、当前权限和恢复缺项 | 核对待决结果、恢复受支持会话、重新规划或结束 | 不重复结算旧动作，不复活失效委托 | 在同一世界历史上继续，或明确说明不能继续的原因。 |

### 3.2 主要失败与恢复

| 情境 | 产品必须说明 | 适用下一步 |
| --- | --- | --- |
| Skill/API 版本不兼容、认证失败或 Agent 不属于当前委托 | 分清 Skill/接口版本、网络连通、账户身份、游戏 Agent 授权和模型账单；指出缺少的资格 | 更正/更新 Skill 或 API 配置、重新认证、选择合法 Agent；不能通过静默切回旧 Bridge 或更换模型掩盖失败。 |
| 目标所需能力不在当前范围 | 缺失能力如何阻断当前目标 | 选择适用目标或等待能力补齐；不持续要求 Runtime 猜测不可用命令。 |
| 资源不足、观察过期、世界拒绝行动 | 当前事实与拒绝原因，已发生和未发生的后果 | 更新观察、补足、等待或重排；适用路径由世界与玩法主责决定。 |
| 超时或提交结果未知 | 未确认的是推理、投递还是世界结果 | 保留关联并核对原结果；未知不等于取消或确定失败。 |
| 委托/预算失效、Runtime 停止或重连 | 观战连接、执行连接、授权和世界运行分别处于何种状态 | 停止新的未授权决策或提交，处理已有结果；恢复前重新核对资格。 |

流程呈现、状态优先级与可访问性由[配对产品设计](external-agent-runtime-play.design.md#3-状态与恢复)细化。

## 4. 产品要求

以下均为目标要求，具体支持与完成状态须按 §5 取证。每条要求上承根 SC-14，并消费所列相邻条款；本文只新增外部 Runtime 的组合义务。

<a id="req-ext-001"></a>
### REQ-EXT-001：Runtime 支持必须有独立、可复现的使用路径

Trace ID: O7-REQ-b3e367c3-c72d-4959-b599-613ac652dab8

- 功能点：[REQ-EXT-001](#req-ext-001)；适用范围沿本条原条件与 [AC-EXT-001](#ac-ext-001)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/external-agent-runtime-adapter.design.md#adapter-drivers)覆盖独立driver、版本manifest、resume/cancel与未知运行禁止重启；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：未核实
- 实现与验证依据：已沿[第一方执行入口](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_model_client/execution.rs#L1-L136)→[SessionModelProvider身份与检查](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_model_provider/src/harness_adapter.rs#L26-L73)核清当前生产切片选择部署preset并运行NativeTaskRunner，[玩家表面](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/model_connection_panel.jsx#L146-L169)明确为“实验模型连接·第一方执行方”，区分模型连接与Agent授权/运行；这不是两个外部Runtime driver的可复现接入证明。[adapter当前/目标边界](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/world-simulator/llm/external-agent-runtime-adapter.design.md#L33-L38)仍把两driver独立manifest、探测、运行和恢复列为目标。
- 核对日期与基线：2026-10-17；需求/设计/静态实现固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，源码逐文件 Git blob 校验；本轮未运行模型、Runtime、浏览器或产品验收。既有PR运行记录仅按各自候选/范围引用。
- 剩余范围：尚须核清每个声明外部Runtime组合的真实driver入口、manifest/版本限制、就绪/启动/resume与失败呈现。当前第一方BYOK链与native协议fixture不证明外部Runtime组合；不凭搜索无命中判未实现。

- 要求：每个声明受支持的 Runtime 组合必须提供可复现的接入、就绪、启动和恢复路径；实际承担决策的 Runtime 与所声明对象一致，能力限制在开始委托前可知。
- 理由：用户能够使用已有 Runtime，并明确区分 Runtime 适配与模型服务接通。
- 上位承诺：[根 SC-14](prd.md#external-runtime-play)。专业权威：[Local Provider](../../world-simulator/llm/provider-loopback-http-contract.prd.md#local-provider-user-flows)。
- 验收：[AC-EXT-001](#ac-ext-001)。

<a id="req-ext-002"></a>
### REQ-EXT-002：连接身份、Agent 资格与委托必须分别成立

Trace ID: O7-REQ-d30925fe-d798-4056-9641-062255931a61

- 功能点：[REQ-EXT-002](#req-ext-002)；适用范围沿本条原条件与 [AC-EXT-002](#ac-ext-002)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/external-agent-game-api.design.md#api-auth)覆盖连接认证、设备配对、有效委托与撤销隔离；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[实现任务 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定实现源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L295)：原生设备签名、当前task fence与完整context验证；[历史prepared-world真实模型记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)仅证明其固定候选的targeted切片，包含原cleanup失败。剩余Game API配对/token、全部费用路由及入口组合，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：本条范围及主要失败边界已有专业承接；实现/验证：Game API配对/token、全部费用路由及入口组合。

- 要求：开始任务前必须核对连接身份、目标 Agent 与有效委托；更换 Runtime、模型或支付路由不得取得别人的 Agent 权限或费用承担资格。接入不得创建额外游戏实体或绕过既有认领与维护条件。
- 理由：用户知道谁在替哪个 Agent 执行、谁承担哪类成本。
- 上位承诺：[根 SC-8 / SC-11 / SC-14](prd.md#external-runtime-play)。消费主责：[Agent 委托](agent-authority-ownership-and-accountability.prd.md#agent-delegation-boundary)、[认领与维护](../world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#1-产品承诺)。
- 验收：[AC-EXT-002](#ac-ext-002)。

<a id="req-ext-003"></a>
### REQ-EXT-003：观察和公布能力必须足以支持声明的玩法

Trace ID: O7-REQ-aad7b341-83b1-4402-8ebc-8540db36ccfb

- 功能点：[REQ-EXT-003](#req-ext-003)；适用范围沿本条原条件与 [AC-EXT-003](#ac-ext-003)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/external-agent-game-api.design.md#api-context)覆盖授权观察、完整目标、合法目录与过期上下文拒绝；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[实现任务 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定实现源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L114)：原生capabilities、industry有界观察与完整目标context；[历史prepared-world真实模型记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)仅证明其固定候选的targeted切片，包含原cleanup失败。剩余完整首局catalog、所有声明能力和失败表面，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：本条范围及主要失败边界已有专业承接；实现/验证：完整首局catalog、所有声明能力和失败表面。

- 要求：Runtime 必须能理解授权视角下的观察、当前目标、可用能力、必要输入和失败含义；声明支持的玩法所需能力必须可经同一权威路径执行。能力或前置条件变化时，接入层必须明确反映限制，不把无法解析或不允许的能力公布成可用。
- 理由：Runtime 能自主选择有意义的行动，并在世界变化后纠正决策。
- 上位承诺：[根 SC-6 / SC-7 / SC-14](prd.md#external-runtime-play)。专业权威：[Decision Provider](../../world-simulator/llm/decision-provider-contract.prd.md#4-technical-specifications)、[双轨观察](../../world-simulator/llm/provider-agent-dual-mode.prd.md#provider-execution-user-flows)。
- 验收：[AC-EXT-003](#ac-ext-003)。

<a id="req-ext-004"></a>
### REQ-EXT-004：接入必须支持 Runtime 自主推进多轮任务

Trace ID: O7-REQ-bd792456-4b98-4891-b5e3-e24b1de4f1a1

- 功能点：[REQ-EXT-004](#req-ext-004)；适用范围沿本条原条件与 [AC-EXT-004](#ac-ext-004)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/external-agent-runtime-adapter.design.md#adapter-loop)覆盖实际Runtime控制/决策循环、上下文与旧目标失效；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[交付 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定静态源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L505)核实原生完整Context读取及客户端行动/结果接口。历史prepared-world固定候选 `b59d010556e39beaf1928eb1e95aa9aad099e180` 的两次真实目标决策、正式Skill使用及原结果回放只由[历史运行记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)支持；其中原cleanup失败保持不变，不能外推当前main或完整首局。剩余双Runtime自主首局、长期会话与工具编排，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：本条范围及主要失败边界已有专业承接；实现/验证：双Runtime自主首局、长期会话与工具编排。

- 要求：在有效授权内，Runtime 必须能够利用其声明保留的规划、会话上下文和工具编排能力自主形成后续决策；接入方式及能力裁剪必须明示，不能要求用户逐动作重新输入任务。世界事实与 Runtime 的计划、推测和私有记忆必须可区分。
- 理由：接入保留已有 Runtime 的使用价值，任务能跨多轮连贯推进。
- 上位承诺：[根 SC-2 / SC-14](prd.md#external-runtime-play)。专业权威：[Harness 权威边界](../../world-simulator/llm/continuous-agent-harness.prd.md#3-权威边界)。
- 验收：[AC-EXT-004](#ac-ext-004)。

<a id="req-ext-005"></a>
### REQ-EXT-005：权威结果必须进入后续认知

Trace ID: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7

- 功能点：[REQ-EXT-005](#req-ext-005)；适用范围沿本条原条件与 [AC-EXT-005](#ac-ext-005)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/external-agent-game-api.design.md#api-actions)已承接原操作查询、受理/结算、同键冲突和读取权限；同文件需求承接/验证表反链本条AC。剩余：结果进入后续认知与玩家反馈的跨专业精确承接仍须核对；api-actions已覆盖原操作/结算/幂等。 不代表实现或发布。[loopback反馈关联](../../world-simulator/llm/provider-loopback-http-contract.design.md#41-harness-contract-alignment)补充target/legacy路由隔离与Runtime receipt关联边界，未覆盖各真实Runtime结果进入后续认知和玩家反馈的全链路。
- 实现状态：部分实现
- 实现与验证依据：[实现任务 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定实现源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L389)：原签名缓存、lookup/replay及同回执恢复；历史两次实际移动；[历史prepared-world真实模型记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)仅证明其固定候选的targeted切片，包含原cleanup失败。剩余全部拒绝/失败组合及玩家反馈端到端，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。 新增loopback局部关系审读基线为2026-10-15的`4e94163a3dff9ba69f6959b406ca3fcedf8aa097`及本次文档候选；未新增行为验证。
- 剩余范围：设计：结果进入后续认知与玩家反馈的跨专业精确承接仍须核对；api-actions已覆盖原操作/结算/幂等。；实现/验证：全部拒绝/失败组合及玩家反馈端到端。

- 要求：行动结果必须关联到正确 Agent 与任务，已结算成功、拒绝、失败和待决的真实含义必须进入后续决策及玩家反馈；模型自报、请求接受或超时不能代替世界结算，也不能隐式触发另一轮重复执行。
- 理由：Runtime 能根据真实后果继续，玩家能确认成果和安全下一步。
- 上位承诺：[根 SC-2 / SC-5 / SC-14](prd.md#external-runtime-play)。专业权威：[反馈隔离](../../world-simulator/llm/continuous-agent-harness.prd.md#7-feedback-correlation-与-isolation)、[世界恢复规则](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md#42-recovery-rules)。
- 验收：[AC-EXT-005](#ac-ext-005)。

<a id="req-ext-006"></a>
### REQ-EXT-006：高层目标调整必须可确认生效

Trace ID: O7-REQ-091d6c22-fca5-4284-91dd-2edce9ef440b

- 功能点：[REQ-EXT-006](#req-ext-006)；适用范围沿本条原条件与 [AC-EXT-006](#ac-ext-006)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-goal)覆盖目标保存/激活/采纳/用于决策分层及旧版本阻断；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[交付 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定静态源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L505)核实当前目标关联Context读取与验证。历史prepared-world固定候选 `b59d010556e39beaf1928eb1e95aa9aad099e180` 的两次真实目标决策、正式Skill使用及原结果回放只由[历史运行记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)支持；其中原cleanup失败保持不变，不能外推当前main或完整首局。剩余完整UI草稿/受理/应用、delivery/adoption与停止交错，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：本条范围及主要失败边界已有专业承接；实现/验证：完整UI草稿/受理/应用、delivery/adoption与停止交错。

- 要求：玩家必须能够对合法绑定的 Agent 调整持续目标或提示词，并区分草稿、已接受、已应用和被阻塞；后续决策必须消费当时有效的目标与委托。停止后续委托不自动取消或抹去已有待决行动。
- 理由：玩家能持续引导自主 Agent，并知道系统何时采用了新的方向。
- 上位承诺：[根 SC-9 / SC-11 / SC-14](prd.md#external-runtime-play)。消费主责：[Prompt 与目标](agent-conversation-and-prompt-control.prd.md#23-agent-prompt-与目标调整)、[在途意图](provider-agent-experience-continuity.prd.md#21-切换窗口与在途意图)。
- 验收：[AC-EXT-006](#ac-ext-006)。

<a id="req-ext-007"></a>
### REQ-EXT-007：执行与观战的在线状态必须分开

Trace ID: O7-REQ-c5c90e09-ae74-4d77-9858-a4769562b01e

- 功能点：[REQ-EXT-007](#req-ext-007)；适用范围沿本条原条件与 [AC-EXT-007](#ac-ext-007)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[运行诊断与生命周期](../../world-simulator/llm/external-agent-runtime-adapter.design.md#adapter-liveness)承接 active run、最近同步/结果、过期unknown/offline及自有进程/本机权限边界；同文需求与验证映射回链AC-EXT-007。各声明viewer/pure_api入口的观战、执行、委托及结果四层状态及独立运行条件的完整组合仍待逐项核对；维持部分覆盖。
- 实现状态：未核实
- 实现与验证依据：[真实启动者](https://github.com/nil-studio/oasis7/blob/bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_model_client/execution.rs#L85-L136)将SessionModelProvider交给后台NativeTaskRunner；[页面续期](https://github.com/nil-studio/oasis7/blob/bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5/crates/oasis7_viewer/software_safe_src/model_connection_panel.jsx#L116-L134)与[服务端过期/资格重验](https://github.com/nil-studio/oasis7/blob/bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5/crates/oasis7_model_provider/src/connection.rs#L604-L636)说明这是第一方BYOK会话执行切片，不能用线程不随HTTP返回退出证明外部Runtime独立委托。已核清native的ownership、预算、fence及停止原因，但OpenClaw/Codex实际driver、observer断连后的持续执行与分层反馈生产链仍未核清；本轮仅静态审读，未执行AC-EXT-007。
- 核对日期与基线：2026-10-17；本条需求、设计及本轮新增静态源码固定`bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5`；只读GitHub与本地文本。历史native prepared-world记录仍限定其原候选/入口，不作为本候选观战断连或Runtime停止验收；未调用外部Runtime、模型、浏览器、部署或用户电脑。
- 剩余范围：核清每个声明外部Runtime/profile的启动者与进程所有者、独立委托及预算续存条件、observer连接关闭/重连与executor控制的实际关系、心跳生产/过期诊断和四层状态在viewer/pure_api的消费者；分别取得断开观战仍产生真实后果、停止/失权阻止新决策或受限提交而世界推进与既有行动仍可查的同案证据。BYOK会话到期是资格边界，不能误报为外部Runtime缺陷或已证明独立执行；全部实现义务仍未闭合。

- 要求：无 GUI 的 Runtime 执行路径必须可用；关闭或断开观战界面时，只要 Runtime、所需连接、预算和独立委托仍有效，任务应继续。Runtime 停止或执行资格失效时必须如实表达，不能承诺继续产生新决策，也不能暂停世界或伪造既有行动取消。
- 理由：用户能够离开界面，并准确理解持续运行所需条件。
- 上位承诺：[根 SC-4 / SC-14](prd.md#external-runtime-play)。专业权威：[双轨执行](../../world-simulator/llm/provider-agent-dual-mode.prd.md#1-executive-summary)；消费主责：[玩家接入与发行中的会话与委托连续性](../player-entry-distribution/prd.md)。
- 验收：[AC-EXT-007](#ac-ext-007)。

<a id="req-ext-008"></a>
### REQ-EXT-008：断连和重启必须在原世界历史上恢复

Trace ID: O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330

- 功能点：[REQ-EXT-008](#req-ext-008)；适用范围沿本条原条件与 [AC-EXT-008](#ac-ext-008)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-recovery)已承接协作原子控制、恢复切点、游标缺口和旧fence拒绝；同文件需求承接/验证表反链本条AC。剩余：Runtime私有上下文不可恢复时的明示和可信重规划跨专业关系仍须核对；collab-recovery已覆盖世界任务恢复。 不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[实现任务 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定实现源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L322)：持久原prepare/行动与未知结果只查询原请求；[历史prepared-world真实模型记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)仅证明其固定候选的targeted切片，包含原cleanup失败。剩余完整断连/重启、私有记忆丢失提示和旧授权负例，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：Runtime私有上下文不可恢复时的明示和可信重规划跨专业关系仍须核对；collab-recovery已覆盖世界任务恢复。；实现/验证：完整断连/重启、私有记忆丢失提示和旧授权负例。

- 要求：受支持的断连/Runtime 重启恢复必须重新核对世界、Agent、权限、有效目标与未决结果；已结算结果不得重做，未知提交先核对，失效授权不得自动复活。无法恢复的 Runtime 私有上下文必须明示，并提供从可信世界事实重新规划的路径。
- 理由：玩家可以继续已有任务，恢复不会复制成果或悄悄丢失重要义务。
- 上位承诺：[根 SC-5 / SC-14](prd.md#external-runtime-play)。专业权威：[Runtime recovery](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md#42-recovery-rules)；消费主责：[Provider 切换窗口](provider-agent-experience-continuity.prd.md#21-切换窗口与在途意图)。
- 验收：[AC-EXT-008](#ac-ext-008)。

<a id="req-ext-009"></a>
### REQ-EXT-009：持续执行的成本和预算边界必须可理解

Trace ID: O7-REQ-cb69c2a7-f13a-42ea-a929-214fa7ff9d43

- 功能点：[REQ-EXT-009](#req-ext-009)；适用范围沿本条原条件与 [AC-EXT-009](#ac-ext-009)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/external-agent-runtime-adapter.design.md#adapter-budget)已承接模型/工具共享预算、有限deadline与中断能力；同文件需求承接/验证表反链本条AC。剩余：世界资源成本区分归因及失去授权后原结果查询尚需组合承接；不代表实现或发布。
- 实现状态：未核实
- 实现与验证依据：[实际预算生产接线](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_agent_harness/src/native_task_sdk.rs#L65-L99)将config/world/agent/task_fence绑定到[持久物理调用预算](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_client_storage/src/native_run_budget.rs#L28-L190)；每次provider调用前持久预留，重启不恢复余额，缺失/损坏/改限额拒绝，非Unix明确unsupported。5d503f→e27f的native_run_budget逐字节相同；operation_store仅公开feedback存取函数并注明调用者validator责任。实际上游仍Hosted BYOK→SessionModelProvider→NativeTaskRunner；该run物理调用数不是Runtime内部子调用UsageMeasurement计量，更不是CapabilityExecution授权额度或世界资源账。第一方UI仍明确未知费用≠0、预算占用及已受理世界作业独立。
- 核对日期与基线：2026-10-17；状态卡按 A `0901927131b3e7c919fc6b092e77ae8895579989`，静态生产源码固定 main `e27f4d990bc4d481097c003ff75afc582fd8f65d`，22个引用/闭包源逐 Git blob 校验；另比较 `5d503f0ca79bcc68708a32672a48a2786ad51395` 迁移前源码。未运行模型、Runtime、浏览器或行为验收，未把测试源码当本轮PASS。
- 剩余范围：已查清新增crate的预算/原操作恢复语义与真实第一方消费者；仍缺Codex/OpenClaw各自manifest计量范围、稳定meter/sequence去重与恢复、partial enforcement/unknown保留、世界资源成本分项，以及失权停止新受限请求后原结果可读的两Runtime+玩家表面闭包。不得把本地调用数当费用硬上限或将缺完整合同抹成已审子链不存在。

- 要求：开始和持续执行时必须区分外部推理/工具开销与世界资源成本，说明可执行的预算限制、估计值和未知值；达到可执行限制或授权失效后停止新的受限请求或提交，并保留已有结果的处理路径。无法保证的下游硬上限不得被显示为已强制生效，未知开销不得按零处理。
- 理由：玩家能够判断是否继续委托，并理解 Runtime 内部调用对开销的影响。
- 上位承诺：[根 SC-7 / SC-12 / SC-14](prd.md#external-runtime-play)。专业权威：[Harness 有界调用预算](../../world-simulator/llm/continuous-agent-harness.prd.md#request-bound-call-budget)；世界经济与固定节奏消费[长期主责](provider-learning-intelligence-and-cadence.prd.md#21-认证-provider-与固定权威-cadence)。
- 验收：[AC-EXT-009](#ac-ext-009)。

<a id="req-ext-010"></a>
### REQ-EXT-010：首期完成必须由真实 Runtime 的完整游戏任务证明

Trace ID: O7-REQ-7283b901-ea06-49e3-956d-2bad46b6c89d

- 功能点：[REQ-EXT-010](#req-ext-010)；适用范围沿本条原条件与 [AC-EXT-010](#ac-ext-010)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/external-agent-game-api.design.md#api-composition)已承接Game API部署及模块分工；同文件需求承接/验证表反链本条AC。剩余：两Runtime分别完成产品§2.2首局及目标调整/阻塞/恢复的组合设计仍须核对；api-composition仅覆盖部署及模块分工。 不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[交付 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定静态源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L428)核实原生签名行动提交与原结果查询接口。历史prepared-world固定候选 `b59d010556e39beaf1928eb1e95aa9aad099e180` 的两次真实目标决策、正式Skill使用及原结果回放只由[历史运行记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)支持；其中原cleanup失败保持不变，不能外推当前main或完整首局。剩余OpenClaw及Codex各自完整首局、工业链与组合恢复，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：两Runtime分别完成产品§2.2首局及目标调整/阻塞/恢复的组合设计仍须核对；api-composition仅覆盖部署及模块分工。；实现/验证：OpenClaw及Codex各自完整首局、工业链与组合恢复。

- 要求：OpenClaw 与 Codex 必须分别在声明组合内完成 §2.2 的首局闭环，并提供可追溯的权威结果、目标调整、阻塞与恢复证据。旧 P0 smoke、单次调用、累计动作数量或另一 Runtime 的成功均不能代签。
- 理由：产品完成对应用户能持续玩起来，而支持范围可以准确评估。
- 上位承诺：[根 SC-7 / SC-14](prd.md#external-runtime-play)。消费主责：[首局工业结果](../world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-004)；专业权威：[parity](../../world-simulator/llm/provider-agent-experience-parity.prd.md#1-executive-summary)。
- 验收：[AC-EXT-010](#ac-ext-010)。

<a id="req-ext-011"></a>
### REQ-EXT-011：官方 Skill 必须让真实外部 Runtime 独立上手

Trace ID: O7-REQ-a5bbf6e4-3e71-4d48-9eba-68c50867c95c

- 功能点：[REQ-EXT-011](#req-ext-011)；适用范围沿本条原条件与 [AC-EXT-011](#ac-ext-011)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/external-agent-runtime-adapter.design.md#adapter-skill)覆盖可信Skill分发、适用版本、安装和不兼容失败；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[交付 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定静态源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L46)核实普通签名客户端配置加载和受控连接入口。历史prepared-world固定候选 `b59d010556e39beaf1928eb1e95aa9aad099e180` 的两次真实目标决策、正式Skill使用及原结果回放只由[历史运行记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)支持；其中原cleanup失败保持不变，不能外推当前main或完整首局。剩余两个Runtime的公开安装、版本兼容和冷启动接入，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：本条范围及主要失败边界已有专业承接；实现/验证：两个Runtime的公开安装、版本兼容和冷启动接入。

- 要求：首批 OpenClaw、Codex 均必须有来源和适用版本可核对的官方 Skill 使用路径，能指导 Runtime 发现受授权游戏 API、完成接入和首个合法游戏动作，并提供版本不兼容、安装/调用失败的修复说明；不能将仓库内部 Bridge、静态假能力或原始脚本维护经验当作用户必要前置。
- 理由：用户可以带来已有 Runtime，不需要先理解 oasis7 内部 Provider 工程。
- 上位承诺：[根 SC-14](prd.md#external-runtime-play)。消费主责：[现有公开 Skill](../../../site/skills/oasis7.md)、[Runtime 支持范围](../../world-simulator/llm/decision-provider-contract.prd.md)。
- 验收：[AC-EXT-011](#ac-ext-011)。

<a id="req-ext-012"></a>
### REQ-EXT-012：对外游戏网络 API 必须独立形成完整行动闭环

Trace ID: O7-REQ-8120dffe-17cc-458a-9c70-8121217e6f7b

- 功能点：[REQ-EXT-012](#req-ext-012)；适用范围沿本条原条件与 [AC-EXT-012](#ac-ext-012)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/external-agent-game-api.design.md#api-wire)已承接Game API目标路由和DTO；同文件需求承接/验证表反链本条AC。剩余：首局合法行动的可发现、可调用与玩法目录的组合承接尚需核对；不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[实现任务 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定实现源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L271)：独立原生签名HTTP客户端的context、行动、lookup和工业观察；[历史prepared-world真实模型记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)仅证明其固定候选的targeted切片，包含原cleanup失败。剩余完整/v1/game写路由、配对及可恢复增量闭环，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：首局合法行动的可发现、可调用与玩法目录的组合承接尚需核对；实现/验证：完整/v1/game写路由、配对及可恢复增量闭环。

- 要求：无需特定 Provider Bridge 或内置模型进程，具有有效游戏权限的普通网络客户端必须能够协商版本、读取授权世界观察、查询对象和合法能力、提交意图、区分受理与结算、追读权威 receipt/状态并以可恢复增量方式继续。首局工业链需要的合法行动必须在此通道上可发现且实际可调用；MCP/CLI 包装不得改变业务语义。
- 理由：Agent Runtime 可以主动使用游戏能力，而非被迫伪装成被调用的模型 Provider。
- 上位承诺：[根 SC-2 / SC-7 / SC-14](prd.md#external-runtime-play)。专业权威：[Decision Provider 与能力边界](../../world-simulator/llm/decision-provider-contract.prd.md)、[Runtime 行动生命周期](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md)；网络游戏入口的接口合同由后续对应专业设计承接。
- 验收：[AC-EXT-012](#ac-ext-012)。

<a id="req-ext-013"></a>
### REQ-EXT-013：直接网络调用必须守住认证、幂等与安全恢复

Trace ID: O7-REQ-7d6cef7f-9419-49ed-b445-663382383458

- 功能点：[REQ-EXT-013](#req-ext-013)；适用范围沿本条原条件与 [AC-EXT-013](#ac-ext-013)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/external-agent-game-api.design.md#api-auth)已承接认证、有效委托及当前权限校验；同文件需求承接/验证表反链本条AC。剩余：幂等、并发、未知提交及限流的多条设计关系仍须核对；api-auth已覆盖认证/有效委托和当前权限。 不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[实现任务 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定实现源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L389)：原请求持久化、同Context异动作冲突及签名context验证；[历史prepared-world真实模型记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)仅证明其固定候选的targeted切片，包含原cleanup失败。剩余完整Game API token/限流/远程部署与权限竞态验收，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：幂等、并发、未知提交及限流的多条设计关系仍须核对；api-auth已覆盖认证/有效委托和当前权限。；实现/验证：完整Game API token/限流/远程部署与权限竞态验收。

- 要求：本地与远程 Game API 都必须基于可信身份、明确的游戏 Agent 委托和适用读写范围授予能力；不接受仅凭公开身份/路由字符串、模型密钥或 Skill 内容取得其他主体的权限。延迟、重试、并发、预算/限流、权限变化及未知提交必须有结构化可恢复结果；已结算行动只能被核对，不能通过新连接产生第二次世界效果。
- 理由：开放网络入口不能以方便为名制造越权或重复世界效果。
- 上位承诺：[根 SC-5 / SC-11 / SC-14](prd.md#external-runtime-play)。专业权威：[Agent 自治委托](agent-authority-ownership-and-accountability.prd.md)、[Runtime 最终性](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md)、[玩家身份与会话](../player-entry-distribution/prd.md)。
- 验收：[AC-EXT-013](#ac-ext-013)。

<a id="req-ext-014"></a>
### REQ-EXT-014：外部 Runtime 产品完成不得依赖旧 Provider Bridge 的兼容

Trace ID: O7-REQ-68ffc843-8679-4ad9-8cef-10a239a29c73

- 功能点：[REQ-EXT-014](#req-ext-014)；适用范围沿本条原条件与 [AC-EXT-014](#ac-ext-014)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/external-agent-game-api.design.md#api-cutover)覆盖新主路径独立、旧Bridge迁移和历史保留；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[实现任务 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定实现源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L428)：原生Game客户端经World Service而非旧Provider callback提交；[历史prepared-world真实模型记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)仅证明其固定候选的targeted切片，包含原cleanup失败。剩余完整Skill首局及关闭旧Bridge后的独立组合验收，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：本条范围及主要失败边界已有专业承接；实现/验证：完整Skill首局及关闭旧Bridge后的独立组合验收。

- 要求：首期默认体验和代表性首局的实际运行证据必须由 Skill 引导并经独立 Game API 完成；旧 Provider callback、mock/直接模型调用、旧 CLI 兼容名或共享 DTO 存在不能替代。若新路径满足功能与权威前置，可废弃旧 Bridge 的非必要业务/文档/脚本，不把兼容负担强加给新产品；必须保留仍有真实依赖的世界历史、委托与其他主责能力。
- 理由：目标是让外部 Runtime 自己玩游戏，不是永久维护多套难理解的代理通道。
- 上位承诺：[根 SC-14](prd.md#external-runtime-play)。专业权威：[旧 Local Provider 专题](../../world-simulator/llm/provider-loopback-http-contract.prd.md)、[Continuous Harness](../../world-simulator/llm/continuous-agent-harness.prd.md)和 [Runtime lifecycle](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md)。
- 验收：[AC-EXT-014](#ac-ext-014)。

<a id="req-ext-015"></a>
### REQ-EXT-015：网页和桌面客户端必须真实可视化大世界

Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；本条在外部 Runtime 首期范围消费[REQ-AGENT-STAGE-004](player-readable-world-stage.prd.md#req-agent-stage-004)，不重复声明世界舞台要求。

- 功能点：[REQ-EXT-015](#req-ext-015)；适用范围沿本条原条件与 [AC-EXT-015](#ac-ext-015)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[逐端容器与共同交互合同](../../world-simulator/viewer/external-agent-world-view.design.md#view-native)明确 Web/native 各自在内嵌世界窗口交互，并直接共同执行探索、原结果与指导、图形及连接恢复状态机；同文需求承接与验证映射回链 AC-EXT-015。受限 WebView、安全来源、平台独立取证与未验证组合不得宣称支持均有明确边界；此为设计覆盖，不代表 native 实现或实际平台验收。
- 实现状态：部分实现
- 实现与验证依据：[Web世界host与图形失败文本/恢复](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L1255-L1325)及[授权对象/路线只读检查](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_world_view_inspector.jsx#L4-L37)直接交付Web世界视图子范围，图形失败仍保留获准目标/原回执并提供显式重试。[对应实现摘要](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/product/agents-world-simulation/player-readable-world-stage.prd.md#L263-L277)关联PR #4825/#4834，明确Web切片与原生容器尚未实现边界；本轮仅静态核对，不新增浏览器或真实Runtime验收。
- 核对日期与基线：2026-10-17；需求/设计/静态实现固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，源码逐文件 Git blob 校验；本轮未运行模型、Runtime、浏览器或产品验收。既有PR运行记录仅按各自候选/范围引用。
- 剩余范围：仍需实现并验证受支持桌面客户端内嵌世界容器；外部Runtime首局与Web/native各自交互、重连/图形恢复及真实世界结果的组合验收未闭合。现有Web直接子范围不能代签native或外部Runtime整条。

- 要求：外部 Runtime 首期必须有 Web 浏览器和受支持桌面客户端各自可交互的世界视图；客户端可嵌入 Web 技术，但只有 API/日志、静态截图或外部浏览器跳转不能替代客户端内世界体验。
- 上位承诺：[根 SC-13/14](prd.md)。消费主责：[世界舞台双表面](player-readable-world-stage.prd.md#req-agent-stage-004)。
- 验收：[AC-EXT-015](#ac-ext-015)。

<a id="req-ext-016"></a>
### REQ-EXT-016：用户可在大世界中浏览定位和检查

Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；本条在外部 Runtime 首期范围消费[REQ-AGENT-STAGE-003](player-readable-world-stage.prd.md#req-agent-stage-003)，不重复声明世界舞台要求。

- 功能点：[REQ-EXT-016](#req-ext-016)；适用范围沿本条原条件与 [AC-EXT-016](#ac-ext-016)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/viewer/external-agent-world-view.design.md#view-query)覆盖多尺度浏览、授权搜索选择和回到主目标；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：Web Agent/Location 探索切片已实现，待完整测试；[PR #4825](https://github.com/nil-studio/oasis7/pull/4825)，实现基线 `bb27f5c033674419d24575872bd64e8a0c44a4d0`。已完成局部自动化和本地无 LLM 浏览器检查，完整试玩与范围见[世界舞台实现摘要](player-readable-world-stage.prd.md#stage-implementation-status)。
- 核对日期与基线：历史审读为 2026-10-15、`ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad` 仅对应原需求/设计与静态实现审读；当前 Web 切片实现及局部浏览器证据以[世界舞台实现摘要](player-readable-world-stage.prd.md#stage-implementation-status)所列候选和范围为准。文档审读本身不作运行验收，历史运行候选依各记录独立限定。
- 剩余范围：设施/路线/事件及完整空间投影尚未实现；真实指导与世界结果、私有账号切换、容量和完整适用入口验收尚未闭合。

- 要求：用户应能从有权观看的世界总览进入区域/Fragment，平移/缩放、搜索定位 Agent/设施/路线，选中对象或事件并查看身份、来源时效及下一步；未知/权限外的世界不能伪装全图实时情报。
- 上位承诺：[根 SC-13/14](prd.md)。消费主责：[世界探索](player-readable-world-stage.prd.md#req-agent-stage-003)。
- 验收：[AC-EXT-016](#ac-ext-016)。

<a id="req-ext-017"></a>
### REQ-EXT-017：可视化关联 Agent 指导和权威结果

Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；本条在外部 Runtime 首期范围消费[REQ-AGENT-STAGE-005](player-readable-world-stage.prd.md#req-agent-stage-005)，不重复声明世界舞台要求。

- 功能点：[REQ-EXT-017](#req-ext-017)；适用范围沿本条原条件与 [AC-EXT-017](#ac-ext-017)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[合法高层指导与原回执](../../world-simulator/viewer/external-agent-world-view.design.md#view-guidance)承接只读选择、当前 owner 高层指导、accepted/active 与原世界结果分离，并明确消费权威重验；同文需求承接与验证映射回链 AC-EXT-017。图形失败保留获准文本、原操作及安全恢复已有同文恢复合同正文，但该恢复条款与本功能的精确反向承接尚未核清，不能据此宣称全覆盖。
- 实现状态：部分实现
- 实现与验证依据：[只读对象/路线选择](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_world_view_inspector.jsx#L4-L37)不提交世界动作；[权限约束指导入口](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L3530-L3568)沿当前可控Agent能力启用Prompt；[原回执主体读取](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L231-L273)显示receipt状态/置信度/target_agent_id，[图形失效停止旧会话](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L967-L985)与[Web世界host与图形失败文本/恢复](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L1255-L1325)保留获准文本和回执后显式重试。[主体绑定与恢复交付摘要](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/product/agents-world-simulation/player-readable-world-stage.prd.md#L263-L277)列PR #4825及独立验证缺口。
- 核对日期与基线：2026-10-17；需求/设计/静态实现固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，源码逐文件 Git blob 校验；本轮未运行模型、Runtime、浏览器或产品验收。既有PR运行记录仅按各自候选/范围引用。
- 剩余范围：设计：核清 renderer 故障可信文本与安全恢复的本条精确承接关系。实现/验证：Web 只读选择、权限约束指导表面、回执主体及 renderer 失效恢复为直接子范围；仍须核清两个外部 Runtime 与 native 实际指导→执行→权威结果链，及真实双主体延迟结果、撤权、图形失效组合验收。不能把 guidance accepted 当世界效果。

- 要求：选中自己有权的 Agent 可查看目标、行为、主要 blocker、pending/committed 并进入合法高层 Prompt/目标/委托；选中其他 Agent、点击工厂/地形或拖移地图不直接执行世界动作。Renderer 失败时提供可信文本与安全恢复。
- 上位承诺：[根 SC-9/11/14](prd.md)。消费主责：[世界舞台间接指导](player-readable-world-stage.prd.md#req-agent-stage-005)。
- 验收：[AC-EXT-017](#ac-ext-017)。

## 5. 验收与证据

### 5.1 可独立判定的场景

<a id="ac-ext-001"></a>
### AC-EXT-001：从已有 Runtime 完成接入

Trace refs: O7-REQ-b3e367c3-c72d-4959-b599-613ac652dab8；承接[REQ-EXT-001](#req-ext-001)。

- 覆盖要求：[REQ-EXT-001](#req-ext-001)。
- 给定：一份声明版本和适配方式的 OpenClaw 或 Codex 环境及其使用说明。
- 当：用户按推荐路径接入并启动任务。
- 则：实际决策由声明的 Runtime 会话承担；用户可核对目标世界、适用能力和就绪结果。缺少依赖、不兼容或无法认证时给出明确缺项，不静默换成其他 Runtime 或直接模型调用。

<a id="ac-ext-002"></a>
### AC-EXT-002：错误绑定和越权成本路由被拒绝

Trace refs: O7-REQ-d30925fe-d798-4056-9641-062255931a61；承接[REQ-EXT-002](#req-ext-002)。

- 覆盖要求：[REQ-EXT-002](#req-ext-002)。
- 给定：一个合法委托，以及他人 Agent、失效委托或不属于当前身份的费用路由作为负例。
- 当：执行方尝试绑定、开始任务或在资格改变后继续。
- 则：只有合法组合取得相应能力；公开标识或路由选择不能代替认证和授权，非法组合不产生新世界动作或未经授权的费用。接入不增加游戏实体、认领补贴或维护豁免。

<a id="ac-ext-003"></a>
### AC-EXT-003：公布能力与首局执行一致

Trace refs: O7-REQ-aad7b341-83b1-4402-8ebc-8540db36ccfb；承接[REQ-EXT-003](#req-ext-003)。

- 覆盖要求：[REQ-EXT-003](#req-ext-003)。
- 给定：首局代表链所需的合法观察和能力，以及前置条件已变化的负例。
- 当：Runtime 根据公布的语义和输入要求选择行动。
- 则：合法行动能走到世界裁决并产生对应结果；不支持的能力在选择前可知，前置变化形成可理解的拒绝或重新观察路径。受限 lane 不泄露其他 lane 才能观察的信息。

<a id="ac-ext-004"></a>
### AC-EXT-004：目标驱动的多轮自主推进

Trace refs: O7-REQ-bd792456-4b98-4891-b5e3-e24b1de4f1a1；承接[REQ-EXT-004](#req-ext-004)。

- 覆盖要求：[REQ-EXT-004](#req-ext-004)。
- 给定：有效目标、委托与声明的 Runtime 会话能力。
- 当：任务需要多次观察、规划和行动才能推进。
- 则：Runtime 使用前轮上下文和反馈继续，用户无需逐动作重复任务；可观察到计划变化与工具/行动结果的关联，且私有推测不会被呈现为世界事实。不要求披露模型内部思维过程。

<a id="ac-ext-005"></a>
### AC-EXT-005：成功、拒绝和未知结果均形成闭环

Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7；承接[REQ-EXT-005](#req-ext-005)。

- 覆盖要求：[REQ-EXT-005](#req-ext-005)。
- 给定：分别成功结算、被拒绝、仍待决以及投递后超时的关联请求。
- 当：世界返回结果，或执行方恢复查询原结果。
- 则：真实反馈进入正确 Agent 的后续决策，成功不会被反馈通道丢弃；拒绝可用于修正，未知保留关联并等待核对。客户端重试或响应迟到不产生第二次世界效果。

<a id="ac-ext-006"></a>
### AC-EXT-006：目标调整与停止后续委托可读

Trace refs: O7-REQ-091d6c22-fca5-4284-91dd-2edce9ef440b；承接[REQ-EXT-006](#req-ext-006)。

- 覆盖要求：[REQ-EXT-006](#req-ext-006)。
- 给定：正在执行的任务、一个有效新目标，以及一个尚未结算的旧请求。
- 当：玩家修改持续目标或停止后续委托。
- 则：可以区分接受与实际应用；应用后新决策依据有效目标和授权，旧请求仍按原权威合同处理。对话回复、草稿更新或“已停止”的界面提示不能伪造应用、取消或结算。

<a id="ac-ext-007"></a>
### AC-EXT-007：断开观战与停止 Runtime 产生不同结果

Trace refs: O7-REQ-c5c90e09-ae74-4d77-9858-a4769562b01e；承接[REQ-EXT-007](#req-ext-007)。

- 覆盖要求：[REQ-EXT-007](#req-ext-007)。
- 给定：一个可无 GUI 运行、独立委托有效的 Runtime 会话及其声明的 primary mode。
- 当：先按声明入口断开观战并恢复，再停止 Runtime 或使执行资格失效；`viewer` 验证关闭/回访，`pure_api` 验证断开观察客户端/重新读取观察。
- 则：第一种情形下任务可以继续且回访可看到真实后果；第二种情形如实停止新决策或受限提交，世界继续推进，已受理行动仍可核对。所声明入口提供可理解或可消费的对应状态；一个入口通过不代签另一个。

<a id="ac-ext-008"></a>
### AC-EXT-008：断连和进程重启后安全继续

Trace refs: O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330；承接[REQ-EXT-008](#req-ext-008)。

- 覆盖要求：[REQ-EXT-008](#req-ext-008)。
- 给定：同时存在已结算成果和未决请求的任务。
- 当：分别发生连接中断和受支持 Runtime 进程重启，再尝试恢复。
- 则：在原世界与 Agent 身份上核对结果，不重放已结算动作，不把旧会话恢复视为新授权；上下文缺失或资格失效时说明限制和重新规划/授权路径。两类故障分别留下证据。

<a id="ac-ext-009"></a>
### AC-EXT-009：预算耗尽与未知开销如实表达

Trace refs: O7-REQ-cb69c2a7-f13a-42ea-a929-214fa7ff9d43；承接[REQ-EXT-009](#req-ext-009)。

- 覆盖要求：[REQ-EXT-009](#req-ext-009)。
- 给定：一组可执行预算限制和一组下游内部开销不可完全观测的组合。
- 当：持续执行达到限制，或成本数据缺失。
- 则：受限新请求停止；待决结果仍可查询与处理；已知实际值、估计与未知分开，世界消耗与模型/工具费用分别归因。受限观测不能被表述为下游全链路硬预算保证。

<a id="ac-ext-010"></a>
### AC-EXT-010：两个 Runtime 分别完成首局闭环

Trace refs: O7-REQ-7283b901-ea06-49e3-956d-2bad46b6c89d；承接[REQ-EXT-010](#req-ext-010)。

- 覆盖要求：[REQ-EXT-010](#req-ext-010)。
- 给定：对 OpenClaw、Codex 分别声明的实际运行组合，以及同一玩法版本下可比较、满足既有首局可行性条件的初始场景。
- 当：用户按推荐路径接入，绑定一个合法 Agent，提供高层目标，并让其自主完成代表性工业任务。
- 则：每个 Runtime 各自取得首局主责要求的权威生产成果；在同一世界、Agent 与任务历史上，分别证明一次真实资源/生产阻塞及继续、一次持续目标应用、按声明入口离开后的回访，以及断连与 Runtime 重启后的原任务续接。`viewer` 验证关闭/回访，`pure_api` 验证观察客户端断开/重新读取；不要求进程重启后保留同一 Runtime 私有会话身份。世界成果、反馈、目标和主要开销可追溯；各项分别判定，失败不得被累计动作数掩盖。
- 完成边界：首产物只证明 `production_only` 成果；稳定生产、交付、需求满足、节点灾备、另一个玩家入口和更广 Runtime 支持均需其自身证据。首局通过也不自动替代专业 parity 或公开发行准入。

<a id="ac-ext-011"></a>
### AC-EXT-011：Skill 在两个 Runtime 中分别完成自助引导

Trace refs: O7-REQ-a5bbf6e4-3e71-4d48-9eba-68c50867c95c；承接[REQ-EXT-011](#req-ext-011)。

- 覆盖要求：[REQ-EXT-011](#req-ext-011)。
- 给定：一个新的受支持 OpenClaw 环境和一个独立的 Codex 环境，以及对应世界 API 和合法游戏 Agent。
- 当：两者分别通过官方 Skill 阅读接入说明、完成认证/能力发现并选择第一项合法动作；另以 Skill 版本错误或未授权为负例。
- 则：均可确认 Skill 发布来源、兼容范围、实际目标世界与调用结果；不必先手工研究 Local Provider Bridge 或向 Skill/Prompt 粘贴长期密钥；负例提供具体修复步骤，不静默执行高权限脚本或返回假成功。该场景只证明 Skill 引导与首项合法调用，不代替完整首局。

<a id="ac-ext-012"></a>
### AC-EXT-012：普通网络客户端可独立完成可恢复游戏 API 闭环

Trace refs: O7-REQ-8120dffe-17cc-458a-9c70-8121217e6f7b；承接[REQ-EXT-012](#req-ext-012)。

- 覆盖要求：[REQ-EXT-012](#req-ext-012)。
- 给定：有适用凭据和合法游戏 Agent 委托的普通 HTTP(S)/JSON 客户端（不启动 OpenClaw、Codex 或 Provider Bridge），以及可达首局所需动作的世界。
- 当：客户端协商版本、读取观察/能力目录、查询条件、提交合法意图、追读权威 receipt，再通过有界增量轮询获得变化。
- 则：能力目录与实际允许的输入/动作一致；提交接受不误报结算，世界结果由权威回执给出；只读请求没有世界效果，游标/轮询恢复可继续。缺失能力和过期状态返回可消费的原因，不要求为完成该流程部署特殊 Provider。

<a id="ac-ext-013"></a>
### AC-EXT-013：API 授权、远程访问及重试负例

Trace refs: O7-REQ-7d6cef7f-9419-49ed-b445-663382383458；承接[REQ-EXT-013](#req-ext-013)。

- 覆盖要求：[REQ-EXT-013](#req-ext-013)。
- 给定：两个不同身份/Agent 的委托，以及本地和受保护远程 API 路径，另有错误身份标签、伪造费用路由、过期委托、重复提交、请求超时和限流/预算负例。
- 当：请求读取私有状态、提交行动、重连后查询原意图或在资格失效后重试。
- 则：未授权主体不能因公开字符串、Skill 文本或模型 Token 获得其他用户的能力/额度；合法请求在同一权威世界历史中至多执行一次，待决/未知可重查；错误按认证、权限、版本、容量、预算或未知结果分类并给出安全下一步。不能把 TLS 连接成功直接视为身份授权。

<a id="ac-ext-014"></a>
### AC-EXT-014：主路径不依赖旧桥且可独立裁撤

Trace refs: O7-REQ-68ffc843-8679-4ad9-8cef-10a239a29c73；承接[REQ-EXT-014](#req-ext-014)。

- 覆盖要求：[REQ-EXT-014](#req-ext-014)。
- 给定：已声明的两个外部 Runtime 组合和新的 Skill + Game API，另将旧 `provider_loopback_http` / `oasis7_provider_local_bridge` 视为不可用。
- 当：用户按新推荐路径完成 [AC-EXT-010](#ac-ext-010) 的代表性首局与恢复样例，并评估旧通道剩余真实使用者。
- 则：新首局、合法世界提交、权限、回执与恢复不依赖旧桥；可以在完成独立依赖与公开口径核查后退役不必要的兼容脚本/协议，不丢失世界历史或破坏内置 Agent。若新路径未通过，不因宣称弃用就视作已交付。本 AC 是目标性淘汰判据，本 PR 不执行部署/代码删除。

<a id="ac-ext-015"></a>
### AC-EXT-015：真实 Web 与桌面客户端分别能看和操作世界

Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；承接[REQ-EXT-015](#req-ext-015)。

- 覆盖要求：[REQ-EXT-015](#req-ext-015)。
- 给定：真实世界、合法 Agent 及权威工业结果、一个浏览器和一个受支持原生客户端。
- 当：在各自界面进入世界、缩放定位、选择查看并离开回访。
- 则：两者都实际交互同一世界并读到同义结果；Web、API、Launcher 或外部浏览器跳转不代签客户端内可视化。

<a id="ac-ext-016"></a>
### AC-EXT-016：世界总览到局部再回到任务

Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；承接[REQ-EXT-016](#req-ext-016)。

- 覆盖要求：[REQ-EXT-016](#req-ext-016)。
- 给定：跨区域/Fragment 的世界、若干合法可见对象与未知/最近已知对象。
- 当：玩家缩放平移、搜索 Agent、选中工厂/事件并返回主任务。
- 则：对象身份、来源时效、空间层级与 blocker/下一步连续，未知/无权位置不变成精确实时事实。

<a id="ac-ext-017"></a>
### AC-EXT-017：地图选择不成为直接世界行动

Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；承接[REQ-EXT-017](#req-ext-017)。

- 覆盖要求：[REQ-EXT-017](#req-ext-017)。
- 给定：自己与其他人的可见 Agent、pending 与 committed 行动及一次视图故障。
- 当：玩家选择对象、对自己 Agent 更新目标、点击地形/设施。
- 则：合法目标的 accepted/applied 与权威回执分开，越权 Agent 与地图点击不产生移动/采集/建造，故障能安全恢复可信信息。

### 5.2 叶级追踪

| REQ / AC 关系 | 专业 owner | 专业权威 | 验证证据（应提供） | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-EXT-001](#req-ext-001) / [AC-EXT-001](#ac-ext-001) | agent_engineer / qa_engineer | [Local Provider 使用路径](../../world-simulator/llm/provider-loopback-http-contract.prd.md#local-provider-user-flows) | Runtime 版本、适配方式、实际会话与接入正负例 | test_tier_full |
| [REQ-EXT-002](#req-ext-002) / [AC-EXT-002](#ac-ext-002) | agent_engineer / runtime_engineer / qa_engineer | [Runtime 权威边界](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md#6-agentintentv2-与-authoritycapability-边界) | 合法委托、错误身份/Agent/费用路由与资格变化负例 | test_tier_required |
| [REQ-EXT-003](#req-ext-003) / [AC-EXT-003](#ac-ext-003) | agent_engineer / gameplay_designer / runtime_engineer | [Decision Provider 能力合同](../../world-simulator/llm/decision-provider-contract.prd.md#4-technical-specifications) | 首局能力的公布、解析、裁决与可见性一致性 | test_tier_required |
| [REQ-EXT-004](#req-ext-004) / [AC-EXT-004](#ac-ext-004) | agent_engineer / qa_engineer | [Harness 生命周期](../../world-simulator/llm/continuous-agent-harness.prd.md#3-权威边界) | 真实 Runtime 的多轮目标、上下文和行动结果关联 | test_tier_full |
| [REQ-EXT-005](#req-ext-005) / [AC-EXT-005](#ac-ext-005) | agent_engineer / runtime_engineer / qa_engineer | [反馈关联](../../world-simulator/llm/continuous-agent-harness.prd.md#7-feedback-correlation-与-isolation) | 真实成功反馈及拒绝、未知、迟到和重复反馈对账 | test_tier_full |
| [REQ-EXT-006](#req-ext-006) / [AC-EXT-006](#ac-ext-006) | agent_engineer / viewer_engineer / qa_engineer | [目标与 continuation](../../world-simulator/llm/continuous-agent-harness.prd.md#9-goal-与-continuation-边界) | 目标接受/应用、新决策及旧待决请求的各自结果 | test_tier_full |
| [REQ-EXT-007](#req-ext-007) / [AC-EXT-007](#ac-ext-007) | agent_engineer / viewer_engineer / qa_engineer | [无 GUI 执行](../../world-simulator/llm/provider-agent-dual-mode.prd.md#1-executive-summary) | Viewer 关闭/回访、执行停止和授权失效的区别 | test_tier_full |
| [REQ-EXT-008](#req-ext-008) / [AC-EXT-008](#ac-ext-008) | agent_engineer / runtime_engineer / qa_engineer | [Runtime 恢复](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md#42-recovery-rules) | 连接与进程故障、旧结果不重做和权限重验 | test_tier_full |
| [REQ-EXT-009](#req-ext-009) / [AC-EXT-009](#ac-ext-009) | agent_engineer / runtime_engineer / qa_engineer | [有界调用预算](../../world-simulator/llm/continuous-agent-harness.prd.md#request-bound-call-budget) | 限制生效、未知开销及两类成本归因 | test_tier_required |
| [REQ-EXT-010](#req-ext-010) / [AC-EXT-010](#ac-ext-010) | producer_system_designer / agent_engineer / gameplay_designer / qa_engineer | [parity 证据范围](../../world-simulator/llm/provider-agent-experience-parity.prd.md#1-executive-summary) | 两个 Runtime 各自的代表性首局与恢复完整记录 | test_tier_full |
| [REQ-EXT-011](#req-ext-011) / [AC-EXT-011](#ac-ext-011) | agent_engineer / qa_engineer / liveops_community | [Local Provider 现行操作文档](../../world-simulator/llm/provider-loopback-http-contract.prd.md#local-provider-user-flows); [公开 Skill](../../../site/skills/oasis7.md) | OpenClaw/Codex 分别安装加载、可信来源与真实 Game API 初次使用及失败修复 | test_tier_full |
| [REQ-EXT-012](#req-ext-012) / [AC-EXT-012](#ac-ext-012) | agent_engineer / runtime_engineer / qa_engineer | [Decision Provider](../../world-simulator/llm/decision-provider-contract.prd.md#4-technical-specifications); [Runtime lifecycle](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md) | 不依赖桥的普通 HTTP(S)/JSON 客户端观察、查询、提交、回执与增量恢复契约 | test_tier_required + test_tier_full |
| [REQ-EXT-013](#req-ext-013) / [AC-EXT-013](#ac-ext-013) | runtime_engineer / agent_engineer / qa_engineer | [Agent 委托](agent-authority-ownership-and-accountability.prd.md#agent-delegation-boundary); [Runtime lifecycle](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md); [玩家接入](../player-entry-distribution/prd.md) | 本地/远程认证与授权、路由冒用、幂等、预算限流、重连和世界单次效果负例 | test_tier_full |
| [REQ-EXT-014](#req-ext-014) / [AC-EXT-014](#ac-ext-014) | producer_system_designer / agent_engineer / runtime_engineer / qa_engineer | [Local Provider 专题](../../world-simulator/llm/provider-loopback-http-contract.prd.md#local-provider-user-flows); [Runtime lifecycle](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md) | 新路径在无旧桥时完成首局和恢复、仍用旧实现的独立依赖清点、旧公开指引切换核查 | test_tier_full |
| [REQ-EXT-015](#req-ext-015) / [AC-EXT-015](#ac-ext-015) | producer_system_designer / viewer_engineer / game_visual_interaction_designer / qa_engineer | [Web/native 世界舞台](player-readable-world-stage.prd.md#req-agent-stage-004); [Viewer](../../world-simulator/viewer/README.md); [Launcher](../../world-simulator/launcher/README.md) | 浏览器与原生客户端分别操作、回访真实世界，无外部浏览器代签 | test_tier_full |
| [REQ-EXT-016](#req-ext-016) / [AC-EXT-016](#ac-ext-016) | viewer_engineer / game_visual_interaction_designer / qa_engineer | [世界探索](player-readable-world-stage.prd.md#req-agent-stage-003); [Fragment LOD](../../world-simulator/viewer/viewer-pixel-world-fragment-lod.prd.md) | 总览、区域、缩放、对象搜索选择、来源/权限负例 | test_tier_full |
| [REQ-EXT-017](#req-ext-017) / [AC-EXT-017](#ac-ext-017) | agent_engineer / viewer_engineer / runtime_engineer / qa_engineer | [间接指导](player-readable-world-stage.prd.md#req-agent-stage-005); [Prompt](agent-conversation-and-prompt-control.prd.md) | 地图选中、目标应用、权威结果、无权直接动作与故障恢复 | test_tier_full |

### 5.3 证据范围与判定

每次判断保留适用 Runtime/版本、Skill 发布版本/来源、Game API 版本、adapter（若有）、模型/profile、世界与玩法候选、Agent、primary mode、执行 lane、权限/预算、场景和时间窗口。Skill 首次引导、独立普通网络客户端的正负例、真实 Runtime 首局和无旧 Bridge 迁移验证应分别取证。普通 CI 优先验证可稳定复现的协议、授权、反馈、重复与恢复负例；真实 Runtime 会话证明实际适配及多轮行为，世界 receipt/journal 证明世界效果，玩家体验核对证明目标、阻塞和下一步可读。各类证据分别说明其覆盖范围，不要求为同一可自动验证事实重复增加专用环境验收。

OpenClaw 与 Codex 逐项、分别出结论；一个组合可以先形成其有限证据，两者完成才满足本文首批目标。需要宣称 `viewer` 与 `pure_api` 都可用时分别提供入口证据；主流程没有图形依赖不自动证明 Viewer 交互或浏览器内 Runtime 已实现。

模型行为存在不确定性，重复样本、任务完成率、等待与失败阈值沿用专业 parity 对相应场景的定义；真实首局场景应有自己的对应证据，不能搬用六动作 profile 的历史阈值和样本。产品验收不要求不同模型产生相同计划，也不把底层模型能力差异算作接入缺陷；接入导致的能力缺失、结果丢失和恢复错误须单独识别。

## 6. 权威与相邻专题

| 本文组合的语义 | 唯一主责与边界 |
| --- | --- |
| 外部 Runtime 首次接入至持续游玩的完整体验 | 本 PRD；同名 design 负责体验组织。 |
| 认领、维护、生产成果、资源与经营恢复 | [玩法产品](../world-rules-core-gameplay/prd.md)与其首局/所有权分册；本文不新增经济权利或完成定义。 |
| 自治、委托、提示词与 provider 切换 | [自治与责任](agent-authority-ownership-and-accountability.prd.md)、[Prompt](agent-conversation-and-prompt-control.prd.md)、[体验连续性](provider-agent-experience-continuity.prd.md)；本文消费其规则，新增外部 Runtime 路径的可用性要求。 |
| 账户、会话、访问模式和公开 claim | [玩家接入与发行](../player-entry-distribution/prd.md)；Runtime 接入不产生新玩家模式或默认发行资格。 |
| 游戏 Skill 来源、能力引导与外部 Game API 的产品体验 | 本文拥有可被外部 Runtime 使用的组合承诺；[公开旧 Skill](../../../site/skills/oasis7.md)是当前操作材料，产品层不把其桥接操作固定为目标形态。 |
| 观察/决策/反馈、适配、记忆策略与持续调度 | [LLM/provider 专业入口](../../world-simulator/llm/README.md)及对应合同；协议、网络鉴权、动作能力和状态机由后续专业系统设计决定。 |
| 世界提交、持久化、恢复与最终性 | [权威世界基础设施](../world-infrastructure/prd.md)与 [Runtime lifecycle](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md)；Runtime 自述不产生世界事实。 |
| 具体组合的行为、时延与发布条件 | [parity 权威](../../world-simulator/llm/provider-agent-experience-parity.prd.md)及 QA 证据；旧 P0、Harness 验证和本文首局各自保留其范围。 |

## 7. 设计取舍与后续细化

产品目标、首批 Runtime、**官方 Skill + 主动网络 Game API 的推荐接入路径**、代表性首局与上述验收义务已明确。后续系统设计需给出标准 Game API 的服务归属、认证和安全授权、世界状态读写与查询边界、幂等/游标、版本化、两个 Runtime 的 Skill 适配、可执行预算和恢复方案；这些实现选择不能降低产品完成标准，也不在本 PRD 中预设未验证的端点/schema。

旧 Provider Bridge 是可取代的实现选择，不再作为新外部 Runtime 产品标准路径的兼容前提。若其调用方向、功能覆盖、身份安全或用户成本不适合新目标，优先删减而非围绕旧形状叠新包装；弃用前审计仍依赖该桥的运行模式与历史成果，新的 API 必须沿用世界权威校验，不迁移伪造的本地事实。该产品选择不改变现有代码，专业方案可分片逐项替换。

进入具体组合的实现和试点前，agent/runtime/QA owner 应选定可复现版本、部署环境、模型/profile 与对应场景阈值；判断记录保存在 Git、PR 和实际验证结果中。跨 Runtime 私有上下文迁移、更多玩法和托管运行按各自范围推进，不作为首期闭环的隐藏依赖。
