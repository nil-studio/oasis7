# 经营决策、目标委托与回访续接系统设计

- 状态：`active`（长期设计约定，不代表实现、试玩或发行已完成）
- Owner role：`gameplay_designer`
- 专业协同：`producer_system_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`、`qa_engineer`
- 产品要求：[`首局与持续游玩 PRD`](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md)
- 产品体验方案：[`设计范围与具体体验`](../../product/world-rules-core-gameplay/first-session-and-continuation.design.md#gameplay-experience-priorities)
- 系统边界：本文细化既有要求的组合接线、事实投影、异常与验证；领域规则、授权、报价、结算和持久化仍由相邻专业 authority 拥有。
- 审读范围：现有 Smelter 首产物、后继工业分支、间接控制和 Viewer/API 接点。固定源码基线、审读发现、实现状态与实际验证结果留在 PR，不由文档状态推导。

## 1. 问题、目标与非目标

即使动作、分支说明、回执和恢复字段分别存在，玩家仍可能无法比较取舍、需要逐步操作，或回访时不知道如何继续。目标是把这些能力组织成“理解目标与投入 → 有界委托 → 看见后果 → 作出下一决定 → 回访”的体验；可用性、玩家理解和愿意继续分别验证。

当前范围保持 `starter-industrial-smelter-to-assembler-v1` / `production_only`。真实生产结算与首产物里程碑成立后只开放 Assembler 候选；建成 Assembler、稳定窗口、真实交易与策略模板均不增加为首产物条件。首局信任、首次持续能力、合作交付与真人续玩意愿分别判断。

非目标：不新增经济数值、全知需求服务、万能 Agent、平行回执、授权账本或运行时反事实引擎；不改变共识、WASM ABI、发行承诺或 CI 门禁。合作与策略复用保留后续范围，设计方法不创造新的执行权限或验收通过事实。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

每行只承接一个上游条款与一个本地设计条款；同一关系在验证表逐项对应。适用 AC 保持原文条件，不用局部工业样例代签其他领域。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；[REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001) | 当前首局的真实后继用途、投入取舍与下一决定可归因于原成果 | [路线与经营价值](#des-adopt-001) | 原 profile / M4 / gameplay / QA | 不把两个推荐或 enabled 动作当作实质分支 |
| Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；[REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001) | 玩家已接受目标与有效委托关联实际应用及原结果，理由不代替世界因果 | [有界委托与纠正](#des-adopt-002) | 原意图/授权/任务与领域执行 authority | 不以 Prompt ACK、模型自述或环境变化证明玩家影响 |
| Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；[REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001) | 同一方案、请求、结果与当前 blocker 支持可理解且真实的下一步 | [方案与下一步](#des-adopt-003) | 原 quote/receipt / Viewer / pure API | 本 profile 的局部链不代签其他受支持路径 |
| Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；[REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001) | 回访保留原成果和未决义务，当前合法下一步与继续意愿分别证明 | [成果与回访](#des-adopt-004) | Runtime 原恢复 / gameplay / QA | 重连、预约返回和摘要可读不证明自发回访 |
| [REQ-FIRST-BRANCH-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-001) Trace refs: O7-REQ-4c622b94-22bd-4559-9595-6b134bdc00b1 | 主指引和每个推荐的完整承诺与当前阶段、实际可用动作同义 | [DES-ADOPT-001](#des-adopt-001) | gameplay / Runtime 动作来源 / Viewer | 不由内部 Governance 阶段推导治理或冲突开放 |
| [REQ-FIRST-BRANCH-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-002) Trace refs: O7-REQ-2e82fef6-bbb5-43b6-a7d0-833905e8e8a3 | 适用回退的窗口、代价、保留与失去价值一致，关闭后不被恢复过程重新打开 | [DES-ADOPT-001](#des-adopt-001) | gameplay 回退合同 / Viewer / QA | 不在没有正式回退能力的路径创造退款或撤销 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | 成果连接真实后继用途和可达分支；比较投入顺序与机会成本，不增加首产物条件 | [DES-ADOPT-001](#des-adopt-001) | gameplay / M4 当前配方与建设规则 | 不以首产物完成证明稳定能力或两条路线同时可做 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | 当前首产物可选承接重复能力、真实受限后的选择、扩展与回访，分别判定技术与体验 | [可选持续目标](#first-capability-continuation) | 原native初态/quote/结算与真实入口能力、QA | 不新增首产物条件、资源、退款或里程碑；当前Viewer小批能力不由native样例补签。 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002) Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | 同一对象、配方、批量与账本的预览、提交、结算支持玩家理解投入和所得 | [DES-ADOPT-003](#des-adopt-003) | M4 报价 / Runtime 执行 / Viewer | 不在展示端复制配方常量或把单批值当本次总量 |
| [REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003) Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | 适用的关键间接决策保留意图、理由/证据、stakes、预期后果、替代、纠正最早生效点与结果 | [DES-ADOPT-002](#des-adopt-002) | Agent / Runtime 因果链 / gameplay / QA | 本设计选取记忆驱动的工业决策子场景；不代签社交、治理或冲突全部 AC 范围 |
| [REQ-AGENT-AUTH-001](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | 委托只在支持能力与有效授权交集中执行，保留累计额度与撤销边界 | [DES-ADOPT-002](#des-adopt-002) | Runtime 原授权合同 / Agent | 不把软偏好、目标或 WASM capability 当玩家授权 |
| [REQ-AGENT-AUTH-001](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | 后续方法复用不携带授权、秘密或未决请求，再次采用需当前合法条件 | [DES-ADOPT-006](#des-adopt-006) | producer / Agent / Runtime / WASM | 模板仍是后续候选，不加入当前首产物验证 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 事实漂移和乱序不混用旧报价，结果未知沿原请求恢复而非重新执行 | [DES-ADOPT-003](#des-adopt-003) | M4 / Runtime 请求与恢复合同 | 关闭界面、新快照或换向不能被当成原请求取消 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 回访、重连和重放不复制结果或复活资格，旧目标义务仍保留 | [DES-ADOPT-004](#des-adopt-004) | Runtime 持久化 / 领域 receipt / Viewer | 不由增量窗口缺失推断未发生效果 |
| [REQ-FIRST-INDUSTRIAL-006](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-006) Trace refs: O7-REQ-d0041aa9-1dfb-46a0-95e0-67417b39bfa3 | 主要阻塞、保留/占用/消费/损失价值与真实下一步可读 | [DES-ADOPT-003](#des-adopt-003) | Runtime blocker / M4 恢复条件 / Viewer | 不编造补料、补电、取消或等待恢复动作 |
| [REQ-FIRST-INDUSTRIAL-007](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-007) Trace refs: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812 | Viewer 与 pure API 对同一可见事实的动作、主要 blocker、完成边界与下一步同义 | [DES-ADOPT-003](#des-adopt-003) | Runtime 共同事实 / Viewer / API / QA | 不把 prepared-world CLI 的局部能力当作所有正式入口已支持 |
| [REQ-FIRST-INDUSTRIAL-007](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-007) Trace refs: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812 | 回访中的成果、当前目标、阻塞和下一步在两入口同义且各自可用 | [DES-ADOPT-004](#des-adopt-004) | Runtime 恢复 / Viewer / pure API | 不要求两个入口采用同一布局或共享私有信息 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | 成果、后继用途、自然离场和回来后的下一决定连续 | [DES-ADOPT-004](#des-adopt-004) | 领域 receipt / gameplay / Runtime / Viewer | 不将合作交付或真人留存当作生产完成的附属结果 |
| [REQ-FIRST-INDUSTRIAL-004](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-004) Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2 | 用途、需求、生产、合作承诺和交付后果分层，production_only 不代签需求满足 | [DES-ADOPT-005](#des-adopt-005) | 现行需求与生产/交付 authority | 当前不实施完整市场或 terminal-admission 正向链 |

### 2.2 相邻 authority 与覆盖范围

[产品具体样本](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#first-session-concrete-sample)与[首次分支](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#31-首次分支承诺)拥有体验与完成边界；[M4 报价到提交](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)拥有方案绑定；[未知结果分离](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)拥有结果与观察状态；[Runtime 因果与纠正](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007)、[授权](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010)与[Viewer 方案交互](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#starter-plan-interaction)拥有各自执行和表现合同。

REQ-WR-IA-003 / AC-WR-IA-003 包含记忆驱动、社交、治理或冲突决策。本设计只选取适用的记忆驱动工业决策验证完整因果，不扩大当前玩法范围；没有对应能力时保留该项未覆盖。普通目标/约束对照服务首局主体感观察，不能替代这个 AC。产品功能的设计/实现状态仍从所属 PRD 与真实证据读取，本设计不提升其已验证范围。

## 3. 当前状态、目标状态与差距

源码接点与产品可用性分开。表中当前事实仅用于定位，不代表整条链已经运行；源码常量不在本文复制为第二份数值规范。

| 当前可核对接点 | 目标状态 | 待实现或待验证的差距 |
| --- | --- | --- |
| [branch_commitment.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/branch_commitment.rs) 根据 BranchReady 与 enabled 动作筛选推荐，含即时收益、后续步骤与风险 | 阶段一致性、动作可用性与路线价值分别成立 | 两个 enabled 动作不证明机会成本、后续两次决策或持续能力；内部 Governance 阶段不代表政治玩法开放 |
| [builtin_recipe_catalog.rs](../../../crates/oasis7_world_state/src/builtin_recipe_catalog.rs) 与 [builtin_factory_catalog.rs](../../../crates/oasis7_world_state/src/builtin_factory_catalog.rs) 提供规范配方和建设规格 | 比较合金与 Assembler 时核对共享的铁锭/铜线、额外建设材料、地点及各自消费账本 | 只有铁锭不能证明两条路线已就绪；建成 Assembler 不证明其后继配方都可运行；静态维护/槽位字段不证明实际有效瓶颈 |
| [gameplay_actions.rs](../../../crates/oasis7/src/runtime_host/gameplay_actions.rs) 将部分快捷动作绑定固定批量，再使用 canonical plan；first_delivery_preview 的数量和 local_need 仍含静态文案 | 本次总量与单批数量分开，预览/提交/实际结果一致；用途与需求分层 | 固定多批动作不能展示为单批成果；静态文案不得充当真实报价、生产结果或需求方证据 |
| [gameplay_snapshot.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_snapshot.rs) 组合动作、阶段、反馈、阻塞与因果信号 | 各入口消费同一可见事实，区分请求状态、方案和世界结果 | 字段存在不证明当前上下文绑定、完整 causal receipt、迟到结果处理或两入口组合可用 |
| [正式游戏 Skill](../../../skills/oasis7-play/SKILL.md) 描述 prepared-world native task：签名客户端、已配置 Agent、任务/上下文、建厂与配方报价、原结果查询 | 复用真实支持的动作与授权，不要求模型绕过游戏接口 | 该 profile 不提供冷启动、claim/pairing、社交或完整 HTTP 写目录；不能从 CLI 支持推断 Viewer 默认代办或所有 Runtime 已接通 |
| 现有产品与 Runtime 设计已有成果、归因、回访与授权合同 | 在原事实链上形成可理解、可恢复的组合体验 | 需分别验证回访窗口完整性、能力发现、累计边界、玩家理解与自发回访；设计不等于实现 |

## 4. 边界与结构

复用既有链路，不引入新的世界 authority 或独立编排服务：

`玩家目标/约束 → Agent 计划或玩家基线操作 → 既有 Runtime 授权与提交 → M4/领域结算 → gameplay/receipt 投影 → Viewer 与 pure API`。

- gameplay 负责路线价值、阶段和真实可比条件；不能靠文案制造不存在的取舍。
- Agent 在已支持动作和有效授权内安排步骤。控制权、累计额度、资源与世界效果仍在执行边界判断；内部/外部执行者都不能凭目标获得更高权限。
- Runtime/M4 提供原请求身份、报价、领域结算与恢复；既有账本保留经济真值。
- 投影层组织既有事实，Viewer/API 不各自补算资格、需求或余额。显示解释与可选语言润色均不写回世界。
- 观察材料按最小可见范围输出，不读取或公开私有 Prompt、完整内部推理、秘密或其他玩家的私有目标。

## 5. 关键运行流程

Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；本节按[玩家杠杆原要求](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)接收当前首局的组合义务：DES-ADOPT-001 的真实选择，经 DES-ADOPT-002 的有效委托或原玩家操作，进入 DES-ADOPT-003 的同一原请求/领域结果，再由 DES-ADOPT-004 连接后继用途与回访。前景目标、Agent 自主活动和环境推进分别归属；只有原意图/执行/结果的准确关联才说明本次玩家改变了什么。当前不支持的委托步骤仍保留真实玩家操作基线，不补造因果或下一动作。

这条接收沿用本文 production_only 与原授权/恢复范围。成熟成长及受阻后的恢复分别由[原成长组合](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-continuing-growth)和[原恢复比较](../../world-runtime/runtime/production-recovery-comparison.design.md#mature-delivery-recovery-comparison)承担；本地样本不能代签它们或其他已声明路径。规则可达、真实操作、玩家理解和继续意愿分层取证，按[原证据消费方法](../../testing/design.md#playability-evidence-consumer-reception)保留范围和未证明项。

<a id="des-adopt-001"></a>
### DES-ADOPT-001：可达路线、投入顺序与经营价值

从当前阶段、可见范围和规范动作集合获取候选，再读取匹配对象、配方、批量、账本与规则版本的报价及后继条件。立即可做、前置未满足但可查看和事实未确认分别表达；查看候选不锁定产能或创造提交资格。

深化 Smelter 与建设 Assembler 是待核对的比较对象，不是默认互斥职业。先比较铁锭/铜线等共享输入所在账本、额外建设条件、真实执行时长与约束，以及完成后可达用途。两项均可无实质代价地先后完成时，应描述互补扩展或投入顺序；存在真实稀缺或窗口压力且后继选择不同，才可以说明相应机会成本。引用槽位、维护和吞吐时，必须确认执行路径确实消费这些约束，不能仅凭规格字段存在推导瓶颈。

后续两个 beat 表示玩家会面对的两个可辨认决策，不是两个固定按钮或两次等待。例如产物是否投入一个当前可达用途，以及是否继续投入原产线；建厂路径则先核对第一项可用配方的投入，再决定如何支持其输入。条件不支持这些选择时保留缺口，不把“更高级”“长期更赚钱”当作证据。

当前只有一条合法路线时保留操作/学习基线，不凑数；后续阶段原有分支义务仍未完成。选择后的提交重新核验当前状态；回退继续使用原合同的窗口、代价与保留价值，换向不撤销既有未知请求或已结算建设结果。回退窗口关闭后，恢复成功也不重开窗口。

规则对照在相同合法初始状态的隔离样例上比较不同路径，不是在正式世界复制资源。比较范围包含各自合法转移、后继前置与真实时长；不要只比较抽象材料总数。小批/大批按实际参数能力和综合机会成本核验，不默认等价或有优劣。

<a id="first-capability-continuation"></a>
#### 当前首局的可选持续目标

Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3；承接 [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008)。首产物之后可由玩家选择“保留当前产线，理解一次真实受限并作出可执行的后继决定”；这不是新增 milestone、奖励、固定任务次数或强制事故。实际可执行边界从[当前 native 初态与消费账本](industrial-native-content-delivery.design.md#starter-and-initial-state-applicability)和 fresh quote 获取。

| 决策节点 | 原事实与可作的选择 | 独立完成判断 |
| --- | --- | --- |
| 建立有限成果 | 支持批量选择的 native 入口可以从同一合法初态选择 1 batch iron；匹配结算形成正量 iron 与 slag，并沿原规则开放 Assembler 候选 | 首产物仍为 `production_only`；玩家可在此自然离场，不必继续生产。 |
| 继续原产线或扩展 | 在当前授权与余额下比较再做同参数 iron、建造 Assembler 或停止；读取各自真实消费账本、剩余投入与后继用途 | 同一 factory 的三次同 canonical snapshot 已完成 job，只是原 `factory_has_canonical_stable_line` 的技术候选；一次 3-batch job 不是三次完成；当前玩家能力还须经原 current-stable 检查，Blocked/Paused 或当前/未决候选不匹配不能由历史完成窗补签。技术 stable 也不证明玩家已掌握持续能力。 |
| 评估扩展后的用途 | Assembler 建造消耗 owner 原建造料；gear 消耗该 site 的合法预置铁，按实际 quote 与匹配结果接收 | 新产 Smelter 铁保留在其原账本；既没有转入 builder，也没有因材料同名成为 gear 输入。扩展是当前可达用途与下一选择，不是假交付。 |
| 遇到真实约束后重新选择 | 原授权内比较当前支持的减量、保留扩展后停止或其他真实可达目标；若选择不同批量，重新形成明确的新方案并重新报价 | quote 拒绝与真实运行中 factory blocked/resumed 是不同事实。未提交的大批量没有 receipt、job、资源消费或需恢复的业务历史。 |

**规则算例。** 对原合法 starter 初态、相同 authority 和足够的有效授权，owner 电力 132，经 Smelter 建造 10 后为 122；三个相同 1-batch iron job 各消耗 8，累计得到 Smelter site 的 9 iron / 3 slag，电力为 98。再建 Assembler 消耗 10，四个 gear batch 消耗 16，电力为 72。此时 12-batch iron 需要 96，缺 24；若当前其它前置均满足，选择 1 batch 仅需 8，实际匹配结算后电力为 64，Smelter 产物累计为 12 iron / 4 slag。算例数值来自原规则，方法会随规则变更重新核算；它不是新平衡值或运行结果。

原 60 iron_ore / 20 carbon_fuel 经过前三次小批后剩 48/17，足够尝试 12 batch，因此该算例可以独立识别电力不足；减为 1 batch 后剩 44/16。owner 建造料与 Assembler site 的 8 iron 分账核对，gear 的 4 件产出不减少 Smelter site 的 9 件 iron。未确认的 upkeep、base_power_draw 或 maintenance 字段不加入这份已知扣费轨迹；原执行规则若增加实际约束，须据新规则重算并更新证据。

这条正向小批路线仅在入口实际支持选择及提交该参数时适用。当前 Viewer iron 快捷动作固定 12 batch，不能用 native 的小批样本声称其相同路线已可操作；不支持时显示真实能力缺口与当前安全停点。授权只覆盖大批原意图、目的限定或额度不足时，减量也不能由界面擅自改写；按原目标/授权合同取得合法新选择。旧大批请求若曾提交且结果未知，必须先恢复原结果，不能借改量再次执行。

首次持续能力的验收由四项独立事实组成：原产线可重复获得匹配结果；玩家读懂真实投入与用途；出现本次候选确实支持的阻塞/恢复决定且后续结果匹配；至少一个真实中循环方向与回来后的下一步明确。原技术 stable、首局信任、首次分支及继续意愿各自判断。没有自然发生的约束时可在隔离测试中选择规则本已允许的低电状态；不能在玩家世界暗改余额或强制失败。完整方法见 [IC-CONTINUATION](industrial-native-content-delivery.manual.md#ic-first-capability-continuation)。

<a id="des-adopt-002"></a>
### DES-ADOPT-002：有界委托、关键请示与完整纠正

先发现入口/profile 真正支持的动作、当前目标及 executor/context 边界，再区分目标、软偏好和执行端强制约束。某个 CLI 支持建厂或排产，只证明该 profile 的能力入口；默认 Agent 接线、玩家冷启动与 Viewer 代办仍分别验证。未支持步骤保留清楚的玩家操作基线。

| 变化 | 执行与呈现 | 不变的边界 |
| --- | --- | --- |
| 有效授权内的重复检查与合法执行 | Agent 继续代办，保留可查状态，不因内部步骤重复索要确认 | 每次提交仍检查控制权、累计额度、资源与当前上下文 |
| 恢复手段已支持且在授权范围内 | 执行对应恢复，说明原因与复查条件 | 不为完成目标擅自增加购买、建厂、转账或运输 |
| 新增目标/权限/累计承诺，或原授权未覆盖的不可逆取舍 | 新动作提交前请示，说明实际 stakes、替代与保留价值 | 已明确授权的同类成本不反复重问，软偏好不变硬保证 |
| 失效授权、控制权或 executor/context 变化 | 停止新增不合法效果，先核对既有请求，再提供真实重新授权/缩小范围/停止路径 | 不自动重绑自己、不复活授权、不抹除既有结算 |
| 原结果未知或部分结算 | 按原请求查询或原协议安全恢复，分别显示已知与未知部分 | 不换 request identity 重做，不以切换目标代替取消 |

关键因果说明复用既有 causal-decision receipt：被接受意图、可见理由/证据、stakes、预期后果、真实替代、打断/纠正的最早生效点、纠正后的结果逐项关联。最早生效点来自原执行合同；不支持立即中断已开始工作时明确说明哪些后继动作还能受影响。没有可证明的字段就保留缺口，不用自然语言补全。

合并重复请示只影响展示。新决定可以替换前景提示，但原请求、未履行义务和不可逆风险不因提示归档而消失；迟到结果仍归原请求。行为差异要结合完整因果和相同条件对照理解，不能以 Prompt 更新或一次随机输出差异证明玩家已控制结果。

<a id="des-adopt-003"></a>
### DES-ADOPT-003：方案、结果与可理解的下一步

决策面先组织目标与实际方案、主要投入和约束、一个主动作及真实替代；已接受、执行中、结果未知和已结算分别表达。安全、权利、授权和不可逆后果优先，详细账本按需展开。不能为简洁而省略影响本次决定的成本，也不能把未知数填成零。

同一 factory/recipe/batches/input-output ledger 与有效报价贯穿预览、确认、提交和结果。特别核对快捷动作固定的多批参数：总投入、总产出、旁产物与实际执行时长取自原 canonical plan/quote，不用静态 expected_output 字符串或前端乘算创建权威数值。没有该量的权威依据时显示未确认，而不是退回误导性的单批数字。

对象、参数、账本、规则版本、权限或状态变化后，旧报价只作历史参考，重新报价并走原提交重验。迟到响应不能覆盖新选择；舍弃旧展示不等于丢弃原请求查询。原结果未知先恢复查询，不以新快照推断未发生；部分结算保留真实已发生与未确认范围。

阻塞说明受影响目标、已保留/占用/消费/损失价值、复查边界与实际可用下一步。多个 blocker 不得隐藏同级安全风险；没有推进动作时保留真实复核、恢复或重新定目标，而不是空列表。设施关联只画真实依赖，不虚构物流、多输入或自动补料。

两入口在同一 world/owner/profile、可比权限和权威窗口下对照动作语义、完成条件、主要 blocker、下一步和复查点；空、陈旧、冲突与当前状态分别可读。布局可以不同，能力不支持或读取被截断必须明确，不能把未返回项当不存在。Viewer 的动画和 pure API 的字段数量都不代替领域事实。

<a id="des-adopt-004"></a>
### DES-ADOPT-004：成果、自然停点与回访连续性

先显示领域结算支持的归属成果与后继用途，再在原意图、action/job 和结果精确关联时说明玩家贡献。owner、材料同名或时间相近不足以归因。复用 Action Receipt 的单一展示责任，不另发 receipt、可结算奖励或里程碑。

首产物是有限成果的停点，不要求当场完成 Assembler、稳定能力或交付。回访先确认世界/归属与可读状态，读回成果并核对旧目标及未决请求，再重验当前授权和能力，最后提供真实下一步。旧目标完成、仍在执行和需要新决定分别表达，不重复首局教学来掩盖恢复失败。

只有原事件窗口连续且可核验时才总结“自上次查看之后”。窗口缺失、cursor 不兼容、重组、归属改变或源不可用时，降级为当前已确认事实并标明缺口。无新事件、服务未运行与无法确认分别表达，不按离线时长或库存差推算收益。

摘要按原事件身份去重。已读位置仅控制阅读，不改变结算、意图完成或未决状态；读取摘要失败不能阻塞生产。普通环境变化可合并，损失、授权变化和未决义务不能被裁剪为不存在。两入口可以拥有不同已读位置，但必须保留同义事实、义务和可执行下一步，各自独立证明回访可用。

微循环、会话成果与跨会话目标按真实事件相接，不增加强制等待或事故来填满时长。系统提供可解释的进展和自然退出条件；看懂回访不等于愿意回来，预约测试回访也不等于自发留存。

对[当前可选持续目标](#first-capability-continuation)，回访重读原匹配 job/receipt、未决请求及三类账本，分别显示一次成果、原技术 stable 与实际仍可做的选择。低电 quote 只留下本次观察；若未提交，不生成或恢复一个“失败生产”。重新取得当前权限和 quote 后，玩家可继续小批、扩展或停下；电力不会因离线、重连或读取 summary 自动恢复。验证沿 [IC-CONTINUATION](industrial-native-content-delivery.manual.md#ic-first-capability-continuation)覆盖原结果未知、无新事件和入口不支持小批的情形。

<a id="des-adopt-005"></a>
### DES-ADOPT-005：用途、需求与交付分层

| 证据 | 可以表达 | 不得推导 |
| --- | --- | --- |
| 只有配方、科技或后继能力关系 | 潜在用途、自己的后继目标与实际缺口 | 某个邻居或组织正在等货 |
| 当前可见、有效且明确需求方的需求记录 | 该记录支持的需求、条件、未满足量与复查边界 | 已签约、保底利润或永久需求 |
| 已接受的合作承诺 | 对应义务与未履行范围 | 已生产、已交付或需求已减少 |
| 匹配交付/领域结算与目的地后果 | 实际交付范围和可证明的需求变化 | 以 production receipt 代签 delivery，或把部分交付当全部完成 |

first_delivery_preview 的兼容字段名不授予交付语义。当前先沿原字段澄清来源和文案；需要分类扩展时按原生产者/消费者合同评估兼容，不直接改名破坏旧快照。缺需求证据就回到用途说明，不能用模型补出需求方或报价。

后续最小合作限定一个真实需求方、一种材料或服务和可核对后果；需求取消、缩减、到期或未知后，未履行部分沿[需求变更合同](gameplay-industrial-demand-change-disposition-contract.prd.md)处理。只有对应专题正式采用后才实施正向链，不改变当前 production_only 首产物。

<a id="des-adopt-006"></a>
### DES-ADOPT-006：复用方法而非复制权力

后续优先复用现有目标配置、参数或受支持模块。模板只携带目标、适用条件、可解释参数和来源版本等非授权信息；grant、秘密、控制权、余额、原请求身份和未决承诺不能被复制。导入是待核验输入，不是可信指令来源。

保存不产生世界效果；再次采用在当前对象、能力、版本和授权下重新预览、形成新的合法意图，不重放上次提交。历史成功只作历史记录。WASM 继续服从现有发布、许可、资源和升级合同，不因模板获得新的 capability。

不冻结模板 schema、存储、商城、分享权限或发布渠道。producer/gameplay 与 Agent/runtime/WASM 在可重复玩法、采用入口和兼容范围明确后细化；此前不显示已可用的保存/导入/执行功能，也不把授权负例通过当成模板已实现。

## 6. 接口与数据合同

以下为原读模型需保留的逻辑关系，不是新增 DTO 或已存在字段声明。缺少事实时由原生产者明确缺口，消费端不猜测。

| 逻辑关系 | authority | 消费规则 |
| --- | --- | --- |
| world/owner/profile、可见范围、新鲜度与能力 | Runtime / 会话 / 正式入口 profile | 只组合兼容上下文；未支持、缺失、截断与不存在分开 |
| 动作、对象、配方、批量、账本、quote 与计划 | 规范动作来源 / M4 | 单批、本次总量和实际结果分开；动作快捷参数与预览一致 |
| 意图、理由/证据、stakes、替代、纠正与结果 | Agent / Runtime 原因果合同 | 只读可见理由，不暴露私有推理；缺失不补造 |
| grant/source/object/period、请求身份与执行关系 | 原授权与请求合同 | 累计检查、有效期、原结果恢复不由显示状态代替 |
| 消费、保留、产出、交付与持久成果 | 领域 ledger/receipt/里程碑 | 只证明其明确结果范围，不跨阶段推导 |
| blocker、可恢复条件、下一动作与后继用途 | Runtime / gameplay / 需求 authority | 不从文案生成权限、需求或余额 |
| 阅读窗口、已读位置与摘要 | 原 journal/receipt 身份 + 非权威显示状态 | 控制阅读，不触发结算或离线进度 |

## 7. 状态、事务与持久化

不新增世界状态机。目标接受、动作提交、执行/阻塞、未知结果、领域结算和玩家已读仍各自由原 authority 维护，不能合并为一个 completed。

预览与摘要只读；资源检查和扣减、原请求去重及结果持久化继续在现有提交边界。并发 Agent、玩家操作、拆分和重试不能绕过累计额度，也不由前端余额预扣代替领域事务。

缓存隔离 world/owner/profile、可见范围和版本；上下文或授权变化后失效。已读位置和展示偏好可以独立保存，不与生产提交组成新事务；位置丢失可以重读历史但不产生新成果。原事件不可获得时只报告可证明的当前事实与未知范围。

## 8. 部署、安全与运行约束

本设计不新增服务、端口、provider、凭据或生产权限。Viewer/API 使用原认证和可见性过滤；公开需求和私有经营记录继续遵循领域合同。测试使用已有正式 Skill 与给定设备配置，不读取 owner 密钥、服务端存储或其他玩家私有状态。

目标、消息、导入内容与模型理由都不能覆盖授权。没有强制校验的预算不宣称硬保证；不为演示自动化注入免费资源或更高权限。Stop 与取消已提交动作分开；查询原结果仍遵循当前合法读取权限，不能通过失效写授权复活新写入。

## 9. 质量与容量

比较与摘要优先复用 snapshot、quote、journal 和 receipt，不在每帧扫描世界或重新调用模型。沿现有分页、保留与可见性界限读取；缓存不能只按 action_id 键控可执行性。读取被截断时不从遗漏推导零库存或无需求。

事实摘要保留不依赖 LLM 的路径；可选语言润色失败时回退事实，不阻断操作与结算。观察读次数、投影耗时、缓存失效、摘要窗口、重复确认和可交互性，数值预算由专业 owner 依据实际场景确定。操作少不等于参与弱，等待长也不自动等于失败；需结合是否知道进展、是否有意义的观察/决定及是否能安全离场判断。

## 10. 兼容、迁移与回滚

先改善现有读模型的来源绑定和解释，必要时才扩展原生产者合同；扩展须核对序列化、旧快照、Viewer、pure API、Agent 与测试消费者。缺字段不能默认为已授权、已交付、零成本或完整事件窗口。本文不冻结新 wire schema。

新比较或摘要不可可靠生成时，回退原真实动作、状态与 receipt，不回退乐观猜测。禁用表现层不撤销已提交或结算的效果，不删除原证据，也不改变 production_only 与 terminal-admission 边界。旧请求和旧上下文沿原协议恢复，不以新版本重做未知动作。

## 11. 验证设计与可追溯性

每次执行记录固定候选、实际入口/profile、world/owner、权限、动作范围与未覆盖项。先核对一个 Smelter 纵向样本，再验证适用阶段的路线、纠正与回访；不要求新人同场完成所有场景。

### 11.1 验证映射表

下表“测试手册”指 [`testing-manual.md`](../../../testing-manual.md)；“自测手册”指 [`agent-runtime-self-playtest.manual.md`](../../testing/manual/agent-runtime-self-playtest.manual.md)。每个场景均按对应手册选择真实入口与固定候选，不以场景名冒称已有同名自动化测试。首产物、稳定能力和交付按各自 profile 独立判定。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；[REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001) | [路线与经营价值](#des-adopt-001) | 原成果后的真实替代、机会成本和下一决定 | [真人分项观察](../../testing/manual/first-session-human-playtest.manual.md#human-study-assessment)：固定当前首局候选/入口/初态，按原 10.1/10.2 核对真实选择及仅标签、无真实替代的反例；领域事实由原 oracle 独立确认 | 真实用途/成本、实际选择、提示及原结果定位 | 未执行真人观察或领域动作不报告通过；不代签其他 profile |
| Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；[REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001) | [有界委托与纠正](#des-adopt-002) | 同任务的目标实际应用、当前授权与原结果分层 | [ST-04](../../testing/manual/agent-runtime-self-playtest.manual.md#st-04)：固定实际受支持 Runtime/profile，联合原 ST-03 检查批准/拒绝/撤销、目标应用及断连恢复；按原 required/full 分层 | 接受/应用/上下文/世界结果及原请求关联、拒绝与未知记录 | 脚本 owner 不等于真人主体感；缺实际能力保留缺口 |
| Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；[REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001) | [方案与下一步](#des-adopt-003) | 实际行动的因果、首产物和阻塞后合法继续 | [ST-03](../../testing/manual/agent-runtime-self-playtest.manual.md#st-03)：同候选/正式入口/合法初态对照原请求、领域结果及真实阻塞后进展；仅自报、ambient 库存变化、ACK和未知结果不得补成完成 | 原 action/operation/receipt、归属成果、blocker 和实际继续条件 | 局部首产物不证明稳定能力、交付或整条外部 Runtime 生命周期 |
| Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；[REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001) | [成果与回访](#des-adopt-004) | 旧成果、待决与当前下一决定连续，预约和自发返回分开 | [真人接续观察](../../testing/manual/first-session-human-playtest.manual.md#human-study-session-two)：同候选与可比较世界历史按原 T5–T8 观察首个动作、旧/新变化、提示和停止；候选变化按原复测规则保留 | 旧成果/原请求、当前权限、实际第一动作与参与者原话 | 预约、重连或主持提示不代签自发回访及长期留存 |
| [REQ-FIRST-BRANCH-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-001) Trace refs: O7-REQ-4c622b94-22bd-4559-9595-6b134bdc00b1 | [DES-ADOPT-001](#des-adopt-001) | 主指引和每个推荐的完整承诺与当前阶段、实际可用动作同义 | VAL-ADOPT-001：依[测试手册](../../../testing-manual.md)核对主指引、完整分支和 enabled 过滤；同时按下述规则对照比较后继条件。AC-FIRST-BRANCH-001 的阶段一致性独立判定 | PR 中的两入口输出与源动作集合 | 两张卡片或不同文案不证明经营价值 |
| [REQ-FIRST-BRANCH-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-002) Trace refs: O7-REQ-2e82fef6-bbb5-43b6-a7d0-833905e8e8a3 | [DES-ADOPT-001](#des-adopt-001) | 适用回退的窗口、代价、保留与失去价值一致，关闭后不被恢复过程重新打开 | VAL-ADOPT-009：依[自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md)分别覆盖截止前、截止时、截止后和恢复后，核对 AC-FIRST-BRANCH-002；仅 fixture 支持时只报告结构结果 | PR 中的原窗口、操作与结果对照 | 不以样例通过证明真实 Runtime 回退已开放 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | [DES-ADOPT-001](#des-adopt-001) | 成果连接真实后继用途和可达分支；比较投入顺序与机会成本，不增加首产物条件 | VAL-ADOPT-001：依[自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md)在同一合法初始状态分别核对合金生产/Assembler 建设、单路线、两者可兼得及共享材料受限的情形；比较真实消费账本、投入后剩余条件和后续两个决策；结合 AC-FIRST-INDUSTRIAL-004 与 FS-10 分项观察 | PR 中的规则对照、后继用途及玩家选择理由 | 不虚构需求、免费材料或强制时限来制造分支 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002) Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | [DES-ADOPT-003](#des-adopt-003) | 同一对象、配方、批量与账本的预览、提交、结算支持玩家理解投入和所得 | VAL-ADOPT-004：依[测试手册](../../../testing-manual.md)核对规范动作的实际 batches 与 canonical plan，再比较 quote、提交、receipt 及展示；包含单批和多批、旁产物与数量缺失；承接 AC-FIRST-INDUSTRIAL-001 的本地方案链 | PR 中的同方案身份、数量和领域结算 | 只看静态文案或计划不能证明真实生产结果 |
| [REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003) Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | [DES-ADOPT-002](#des-adopt-002) | 适用的关键间接决策保留意图、理由/证据、stakes、预期后果、替代、纠正最早生效点与结果 | VAL-ADOPT-002：依[自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md)使用实际支持且可观察的记忆驱动工业选择，逐项读取完整 causal-decision receipt，提交一次合法纠正并核对最早生效点、实际结果和未改变的既有结算；另做单目标/约束行为对照。AC-WR-IA-003 的完整字段与接受/应用边界分别判定；缺记忆或纠正接点就记录未覆盖，普通生产对照不能补签 | PR 中的同一因果链的七类信息、纠正结果及提示记录 | Prompt ACK、单次随机差异或普通非记忆生产不证明完整 AC-WR-IA-003 |
| [REQ-AGENT-AUTH-001](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | [DES-ADOPT-002](#des-adopt-002) | 委托只在支持能力与有效授权交集中执行，保留累计额度与撤销边界 | VAL-ADOPT-003：依[测试手册](../../../testing-manual.md)按 AC-AGENT-AUTH-001 覆盖累计耗用、并发、拆分、重试、过期/撤销、控制权改变及软偏好不等于硬限额；核对每次真正提交而非仅前端余额 | PR 中的grant/source/object/period、原请求和领域结果 | 不新增跨 grant 额度；文档不能证明自动执行已接线 |
| [REQ-AGENT-AUTH-001](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | [DES-ADOPT-006](#des-adopt-006) | 后续方法复用不携带授权、秘密或未决请求，再次采用需当前合法条件 | VAL-ADOPT-003：模板进入正式采用范围后，依[测试手册](../../../testing-manual.md)覆盖保存无效果、导入含旧授权/秘密/请求、版本不兼容、当前资格不足及合法新意图；拒绝不改变原请求 | PR 中的导入内容、当前授权与新意图边界 | 当前授权用例不证明模板功能已实现 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [DES-ADOPT-003](#des-adopt-003) | 事实漂移和乱序不混用旧报价，结果未知沿原请求恢复而非重新执行 | VAL-ADOPT-004：依[测试手册](../../../testing-manual.md)覆盖对象/批量/账本/权限变化、迟到响应、已生效但响应丢失、部分结算与并发；按 AC-FIRST-INDUSTRIAL-003 核对原请求 disposition 和至多一次效果 | PR 中的原请求与 quote/receipt/ledger 对照 | UI 单测或刷新成功不能证明未知结果已核对 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [DES-ADOPT-004](#des-adopt-004) | 回访、重连和重放不复制结果或复活资格，旧目标义务仍保留 | VAL-ADOPT-006：依[自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md)覆盖原结果迟到、重复重放、授权失效、世界切换与已读位置丢失，按 AC-FIRST-INDUSTRIAL-003 核对旧结果保留、未决范围和无重复结算 | PR 中的原 receipt、重入事实与未决请求 | 重新展示历史不能视为新成功或离线收益 |
| [REQ-FIRST-INDUSTRIAL-006](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-006) Trace refs: O7-REQ-d0041aa9-1dfb-46a0-95e0-67417b39bfa3 | [DES-ADOPT-003](#des-adopt-003) | 主要阻塞、保留/占用/消费/损失价值与真实下一步可读 | VAL-ADOPT-005：依[自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md)覆盖缺料、低电、权限变化、多 blocker、无安全路径与来源缺失；按 AC-FIRST-INDUSTRIAL-002 对照实际安全动作，再观察提示前后的单约束理解迁移 | PR 中的权威阻塞、价值与可用动作及玩家解释 | 源字段存在不证明玩家理解；不强制新人遇到事故 |
| [REQ-FIRST-INDUSTRIAL-007](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-007) Trace refs: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812 | [DES-ADOPT-003](#des-adopt-003) | Viewer 与 pure API 对同一可见事实的动作、主要 blocker、完成边界与下一步同义 | VAL-ADOPT-008：依[自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md)在同候选、world/owner/profile 和可比权限下分别读取两入口，覆盖 cold start、进行中、空/阻塞、陈旧及迟到结果；按 AC-FIRST-INDUSTRIAL-004 对照语义而非布局，未支持窗口明确未覆盖 | PR 中的两入口原输出与对应权威事实 | 单入口通过、fixture 或 CLI 结果不代签另一入口 |
| [REQ-FIRST-INDUSTRIAL-007](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-007) Trace refs: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812 | [DES-ADOPT-004](#des-adopt-004) | 回访中的成果、当前目标、阻塞和下一步在两入口同义且各自可用 | VAL-ADOPT-008：依[自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md)分别执行离场/重入，比较各自已读位置下的权威事实、未决义务、权限变化和下一动作；按 AC-FIRST-INDUSTRIAL-004 允许展示窗口不同但不允许事实矛盾 | PR 中的两入口回访输出、窗口与事实关联 | 一个入口自动重连不证明另一入口可续玩 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | [DES-ADOPT-004](#des-adopt-004) | 成果、后继用途、自然离场和回来后的下一决定连续 | VAL-ADOPT-006：依[自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md)在首笔匹配结算后自然离场，覆盖无新事件、服务未运行、事件窗口缺口与下一候选不可达；按 AC-FIRST-INDUSTRIAL-004 分别记录链路、理解、继续意愿、预约与真实自发回访 | PR 中的原成果、后继条件及实际玩家观察 | 合成摘要、Agent 活动、预约回访不证明自然留存 |
| [REQ-FIRST-INDUSTRIAL-004](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-004) Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2 | [DES-ADOPT-005](#des-adopt-005) | 用途、需求、生产、合作承诺和交付后果分层，production_only 不代签需求满足 | VAL-ADOPT-007：依[测试手册](../../../testing-manual.md)覆盖静态 local_need、真实需求未知/取消、仅 production receipt、部分或未确认交付；按 AC-FIRST-INDUSTRIAL-005 核对生产可完成但没有 delivery-only 需求减少或奖励 | PR 中的来源分类、可见说明及领域结果 | 需求存在不证明盈利；交付样例不增加首产物门槛 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | [可选持续目标](#first-capability-continuation) | 分账后继、真实限制、合法减量、技术stable/持续能力/回访分开 | [IC-CONTINUATION CN01–09](industrial-native-content-delivery.manual.md#ic-first-capability-continuation)；同合法初态按实际入口选择，静态/运行/真人分层 | 原job/receipt与三账、电力轨迹、授权/入口差异及参与者观察 | 不代签Viewer未开放的小批、自然留存或通用市场/物流；不将quote拒绝伪造为运行停机。 |

[吸引力自动化入口](../../../scripts/verify-gameplay-attraction-automation.sh)继续区分结构、真实运行与真人证据。跨入口两行分别归 REQ-FIRST-INDUSTRIAL-007，成果动机归 REQ-FIRST-INDUSTRIAL-008；同一 AC-FIRST-INDUSTRIAL-004 的局部通过不能掩盖另一项。单目标对照不补签完整因果，运行正确不补签理解或留存。

<a id="playtest-observation-design"></a>
### 11.2 体验假设的观察设计

承接[产品体验假设](../../product/world-rules-core-gameplay/first-session-and-continuation.design.md#experience-hypotheses-and-pacing)与[试玩判断](../../product/world-rules-core-gameplay/first-session-and-continuation.design.md#playtest-hypothesis-review)。这些方法细化既有 AC/FS，不新增评分模型、问卷、通关数量或 CI gate。

先做规则回归与入口检查，再使用目标/情境而非按钮路径观察玩家。分别记录实际动作/receipt、参与者原话、测试员提示、等待原因和下一选择；材料去标识化，不把私有目标或完整模型上下文贴入公共 PR。记录中区分“能做”“理解”“自主选择”“表示愿意继续”“实际自发回来”，不得合并成一个通过分数。

对照规则时固定合法初始世界和目标；比较人类体验时记录既有经验、呈现顺序、版本与辅助程度，必要时换序或使用新参与者，避免学习效应。比较 Agent 行为时记录真实模型与配置、可见事实和干预，只改变支持的一项条件；随机响应需重复观察或说明不可复现限制。精确因果链验证与人类归因感仍分开。

缺陷归属按原因处理：计划/数量或结算不一致归 Runtime/M4/投影；能力正确但入口找不到归 Viewer/API；信息准确仍无法理解归教学/反馈；理解了却只有支配性路线归玩法；有真实选择但不想继续归目标与成果价值。不要为增加操作量虚构稀缺、强制事故、奖励或后续市场。

小规模观察用于发现反证与修订，不推断普遍留存改善。预约回访只证明约定窗口的可续接性；自发回访需独立观察。没有自然回访记录就保持未验证，不因缺少该记录否定已经证明的运行或理解结果。

## 12. 决策、长期风险与未决问题

| 决定或未决项 | 处理方式与理由 | owner / 复核触发 |
| --- | --- | --- |
| 保持既有工业链和要求 | 组合决策、委托、反馈与回访，复用 Trace ID；不创建新 authority 或首产物条件 | producer/gameplay；目标或 profile 改变 |
| 路线是否形成有效取舍 | 对照真实账本、共享输入、执行约束和后继用途；可能只是顺序选择，不强行命名永久分支 | gameplay/M4；规则、目标或资源范围改变 |
| 哪些步骤可代办 | 逐入口/profile 核对动作、有效授权与实际因果，prepared-world 与 cold start 分开 | Agent/Runtime；动作接线或 executor 合同改变 |
| 完整纠正链的覆盖 | 字段、纠正最早生效点和结果缺失则报告部分覆盖；普通生产不代签记忆/社交/治理/冲突 | Agent/Runtime/gameplay/QA；对应 producer 或样例可用 |
| 回访窗口与体验节奏 | 原 journal/receipt 优先，缺口降级当前事实；事件推进而非强制等待 | Runtime/Viewer；cursor、保留期限、恢复或权限改变 |
| 真实合作与策略复用 | 保留后续范围；真实需求/交付和可重复方法明确后由原专题承接 | producer/gameplay/Agent/M4/WASM；采用范围与兼容方案确定 |
| 解释可信度与隐私 | 事实优先、可见性过滤、因果不足不补全；模型润色不参与执行 | Viewer/Runtime/QA；来源或摘要逻辑改变 |
| 方法可能不成立 | 以玩家目标、胜任、自主与后继期待提出假设；按独立反证修订，不以方法名或小样本证明乐趣/留存 | producer/gameplay/QA；受影响机制或试玩反证出现 |
