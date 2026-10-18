# 间接控制下的玩家能动性与续接

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-09-13
- 专业域权威：[`PRD-GAME-014`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)

本文是长期产品分册，承载玩家通过 Agent 间接推动世界时的能动性、因果可读性、干预与续接承诺。它补充 [`首局与持续游玩`](first-session-and-continuation.prd.md)，但不重复后者拥有的首局引导、首次持续能力与成长承接，也不冻结字段、状态枚举、UI/API、runtime/Agent 实现、测试步骤、任务状态或历史 verdict。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 产品权威边界：本 PRD 是本专题唯一的产品权威，直接承载玩家价值、产品承诺、REQ/AC 与组合验收；本专题不设置配对产品 design。其他专业 PRD 或 design 只是依赖，不重定义本文的产品语义。
- 专业合同委托：[`PRD-GAME-014`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md) 继续拥有玩法保证、字段/状态/失败签名与专业验收；[`DES-WR-IA`](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md) 仅承接跨 runtime、Agent、Viewer/API 与 QA 的技术边界和验证映射。上述专业文档不把实现状态、任务或证据迁入本文，也不能替代本文的产品 authority。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 已直接承载产品承诺与 REQ/AC；独立 design 不增加新的信息分层、交互状态编排或策略取舍，专业文档只保留其各自的玩法和技术合同。
- 历史设计判定记录：本次分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，本次闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
本节的产品体验设计豁免不等于专业系统设计不适用；各要求的技术覆盖与实现证据分别按下列状态摘要核对。

## 1. 产品承诺

玩家通过目标与 Agent 间接推动持续世界时，始终能理解自己正在追求什么、系统是否接受并如何推进、当前后果或阻塞为何发生，以及现在可以如何继续、改道或恢复；离开后返回仍可从这一决策链继续，而不是旁观不可解释的自动行为。

间接控制不是第一人称逐帧操控，也不是让 Agent 隐藏过程后替玩家做完一切。Agent 可以在世界规则、资源、权限和治理边界内自主执行，但玩家必须保有理解主要因果、修正方向和作出下一次决定的能力。

## 2. 玩家体验边界

- 每个受支持的玩家意图形成可读闭环：`选择目标或行动 -> 接受或拒绝 -> 推进或阻塞 -> 可读后果 -> 下一决策或恢复`。
- 对显式推进或 step 意图，请求被排队或接受必须与观察到的完成分开表达；反馈应能关联当前请求。适用完成窗口内没有观察到进展时，必须表达为本次窗口的无进展而不是成功或永久失败，并保留下一决策。
- 被接受的控制只取得一次真实处理机会，不保证 Agent 必然产生行动或世界事件。等待、空结果、自动调度、界面计数或重新加载都不能被包装成玩家已经推进；玩家应看到最近可信状态并保有等待、改道、修正或重新聚焦的下一决策。
- 玩家能够把当前世界变化归因到自己的主意图，读懂主要付出、进展或无进展、阻塞与下一步；世界仍在运行或存在原始日志不能代替这一结果。
- 当前路线不可行、被改道或不再值得等待时，玩家保有下一次决策权，能够理解原因并选择等待、修复、改道、重新聚焦，或安全结束当前意图后重新定目标。
- 玩家可以暂时探索或离开，但能够重新聚焦；重连或回流后能够恢复当前目标、主要阻塞、最近后果与可执行下一步，而不是回到无目标观察。
- 当 Agent 使用长期记忆影响当前决定时，玩家能够理解相关记忆的来源和作用，并能提交纠正或重排当前意图；纠正是否接受、影响范围及生效结果由 Agent 与 Viewer 专业合同拥有。
- 对记忆驱动、社交、治理或冲突决策，玩家能够沿同一因果链理解：被接受的意图、Agent 的可读理由/证据、stakes 与预期后果、替代方向、可用的打断或纠正、最早生效点及纠正后的结果。请求或提案被接受只代表进入权威处理，不代表世界规则已经应用、后果已经发生或提案已经结算。
- Viewer 与 pure API 等正式玩家入口以同一权威世界事实支撑上述体验；入口布局、字段和实现机制可以不同，但不能制造不同的意图、因果或下一步真值。

## 3. 权威与组合关系

| 层级 | 本产品分册拥有 | 下层专业域拥有 |
| --- | --- | --- |
| 玩家价值 | 间接控制仍然保有可理解、可干预、可恢复的玩家能动性 | `game` 定义玩法规则、保证项、失败签名与专业验收 |
| 权威执行 | 玩家意图、世界后果和阻塞处于同一可解释因果链 | `world-runtime` 定义权威状态、校验、执行、回放与恢复合同 |
| 玩家入口与 Agent | 正式入口能够表达同一意图、因果、干预和续接结果 | `world-simulator`、Agent 与 Viewer 专业文档定义 API/UI、记忆和交互实现 |
| 验证 | 组合证据必须证明玩家能理解并继续决策 | `testing` 与 QA 权威拥有测试矩阵、命令、样本和当前 verdict |

`PRD-GAME-014` 的长期产品承诺由本文承载；其专业合同以 [`gameplay-indirect-control-agency-contract.prd.md`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md) 为主入口，继续拥有玩法保证、失败判据与专业验证。IA-1 至 IA-7 只裁定组合体验，不能替代该专业合同；本分册不复制其中的字段矩阵、状态 taxonomy、bounded-response 规则、API parity、失败签名或任务证据。

## 4. 组合验收

- IA-1：代表性流程能够证明 `玩家意图 -> Agent 解释与执行 -> 权威规则校验 -> 世界后果 -> 可读因果 -> 下一决策或恢复` 形成同一条端到端链路。
- IA-2：正式玩家入口都能回答四个问题：玩家要求了什么、系统是否接受、为什么当前这样推进或阻塞、现在最有效的下一步是什么。
- IA-3：路线被阻塞、替换或改道时，玩家能够理解原因并使用至少一种有效的干预、重排、fallback 或恢复路径；没有安全路径时能够返回新的决策面。
- IA-4：离开和返回后，玩家能够从最近有效意图、主要阻塞、最近后果和下一步继续，而不依赖原始日志重建上下文。
- IA-5：记忆驱动的行动能够说明相关记忆为何影响当前决定，并提供可理解的纠正结果；未使用长期记忆的流程不因此被要求引入记忆系统。
- IA-6：产品层、game、runtime、Agent/Viewer 与 testing 的证据指向同一候选和权威事实；任一局部 green、文档建档或世界持续 tick 都不能单独证明本产品承诺通过。
- IA-7：代表性记忆驱动、社交、治理或冲突决策可用同一 receipt 证明意图接受、理由、stakes、替代、纠正、生效点与纠正结果，并明确请求/提案接受与权威应用的边界。

### 4.1 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| IA-1 | gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer | PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md` | 同一意图贯通 Agent、权威校验、世界后果、玩家因果与下一步的组合证据 | test_tier_full |
| IA-2 | gameplay_designer / viewer_engineer / qa_engineer | PRD-GAME-014 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | Viewer 与 pure API 各自入口的意图、接受、主因果和下一步对账 | test_tier_full |
| IA-3 | gameplay_designer / runtime_engineer / agent_engineer / qa_engineer | PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 阻塞、替换、改道、fallback、重排及无安全路径时返回决策面的证据 | test_tier_required |
| IA-4 | gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 重连或回流后的意图、阻塞、后果与下一步恢复证据 | test_tier_required |
| IA-5 | gameplay_designer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-014 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 记忆来源、当前用途、纠正与结果可读性证据 | test_tier_required |
| IA-6 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 同候选跨域组合审计；产品文档、局部测试或世界 tick 不得代签 | test_tier_full |
| IA-7 | gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 跨动作 causal-decision receipt、请求/提案接受与权威应用边界、纠正生效与结果的组合证据 | test_tier_required |

## 4.2 产品要求与叶子验收

<a id="req-wr-ia-001"></a>
### REQ-WR-IA-001：间接控制必须区分意图接受与世界生效

Trace ID: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185

- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 系统设计依据：[indirect-control-agency-execution-and-continuation.design / des-wr-ia-001](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-001)；[gameplay-indirect-control-agency-contract.design / 1-intent-legibility-beforeat-commit](../../game/gameplay/gameplay-indirect-control-agency-contract.design.md#1-intent-legibility-beforeat-commit)；[gameplay-indirect-control-agency-contract.design / 2-execution-acknowledgement-with-causality](../../game/gameplay/gameplay-indirect-control-agency-contract.design.md#2-execution-acknowledgement-with-causality)；[gameplay-indirect-control-agency-contract.design / agency-interruption-hooks](../../game/gameplay/gameplay-indirect-control-agency-contract.design.md#agency-interruption-hooks)；[gameplay-indirect-control-agency-contract.design / 4-bounded-consequence-readability](../../game/gameplay/gameplay-indirect-control-agency-contract.design.md#4-bounded-consequence-readability)。已承接：本条的意图接受、阻塞恢复或因果纠正及结果投影合同。
- 实现与验证依据：已交付 exact intent/digest 重试和 accepted→completed 边界；completion 要求完整 authority tuple、effect intent 与先前已提交匹配 receipt。 实现定位：[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/world/agent_intent.rs#L222-L261)、[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/world/agent_intent_terminal.rs#L289-L381)；断言/历史样本：[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/tests/agent_intent_v2.rs#L615-L711)。本轮只读源码和断言，未运行 Rust、正式入口或真人验收；历史样本仍以其原候选/环境为准。 其余实现交付 PR 与适用运行记录未逐条核定；[核对任务 #4698](https://github.com/eng-cc/oasis7/issues/4698)仅记录审查。
- 核对日期与基线：2026-10-15；需求、专业设计与静态源码基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；本轮无新运行环境或端到端验收结果。
- 剩余范围：已核对 canonical intent accepted、exact retry 与 matching committed receipt completion；所有意图类型、领域结算、真实 provider 和各正式入口仍需独立证据。

- 性质：`目标要求`
- 适用条件：玩家通过 Agent 提交目标、行动、step、社交、治理或冲突意图。
- 要求：产品必须让玩家分别看到意图被接受/拒绝、当前推进或阻塞、权威世界后果和下一步；接受或排队不得被表达为行动已发生或结果已结算。
- 理由：玩家需要知道自己的行动是否仍有机会改变世界，避免把等待、空结果或界面计数误认成推进。
- 上位承诺：间接控制下的玩家能动性与续接。
- 专业权威：[`PRD-GAME-014`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)。
- 验收：[AC-WR-IA-001](#ac-wr-ia-001)。

<a id="req-wr-ia-002"></a>
### REQ-WR-IA-002：阻塞、离开与回流必须保留下一次决策

Trace ID: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63

- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 系统设计依据：[indirect-control-agency-execution-and-continuation.design / des-wr-ia-003](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-003)；[indirect-control-agency-execution-and-continuation.design / des-wr-ia-006](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-006)。已承接：本条的意图接受、阻塞恢复或因果纠正及结果投影合同。
- 实现与验证依据：已交付 pending feedback 的不安全等待说明、enabled action 过滤、修复/改道比较及无可用路径时返回 goal selection 的投影。 实现定位：[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot_fallback.rs#L6-L31)、[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot_fallback.rs#L42-L76)、[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot_fallback.rs#L123-L182)；断言/历史样本：[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot_fallback.rs#L188-L224)。本轮只读源码和断言，未运行 Rust、正式入口或真人验收；历史样本仍以其原候选/环境为准。 其余实现交付 PR 与适用运行记录未逐条核定；[核对任务 #4698](https://github.com/eng-cc/oasis7/issues/4698)仅记录审查。
- 核对日期与基线：2026-10-15；需求、专业设计与静态源码基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；本轮无新运行环境或端到端验收结果。
- 剩余范围：已有失效等待/修复/改道投影及无安全路径返还决策；跨版本恢复、真实可执行领域动作、全部重连入口与当前权限变化仍未闭合。

- 性质：`目标要求`
- 适用条件：路线不可行、被改道、玩家暂时离开或重新连接。
- 要求：产品必须保留当前目标、主要阻塞、最近后果和可执行下一步，并允许玩家等待、修复、改道、重新聚焦或安全结束意图；无安全路径时必须返回新的决策面。
- 理由：持续世界可以自主运行，但不能把玩家降格为无法解释的旁观者。
- 上位承诺：玩家续接、恢复与可干预性。
- 专业权威：[`PRD-GAME-014`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)。
- 验收：[AC-WR-IA-002](#ac-wr-ia-002)。

<a id="req-wr-ia-003"></a>
### REQ-WR-IA-003：关键间接决策必须保留可读因果与纠正结果

Trace ID: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376

- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 系统设计依据：[indirect-control-agency-execution-and-continuation.design / des-wr-ia-007](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007)。已承接：本条的意图接受、阻塞恢复或因果纠正及结果投影合同。
- 实现与验证依据：已交付部分因果receipt生产及owner读面：cognition提交实际调用finish_agent_causal_commit，按同一action的domain event refs生成actual consequence，并保留reason/evidence/stakes/alternative/correction refs；snapshot真实按当前intent关联receipt及私有memory correction。接受纠正与实际效果receipt保持分层。显式supported动作限Move/Factory/Transfer/Recipe，不能外推全部社交/治理/冲突因果链。 [源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L46-L123)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L166-L205)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/cognition_persistence.rs#L1037-L1046)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/agent_delegation.rs#L664-L801)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/player_agency_projection.rs#L101-L194)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/control_blocking.rs#L373-L413)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/llm_sidecar_memory_correction.rs#L29-L48)；[测试定义](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/tests/agent_memory_correction_control.rs#L898-L965)。本轮只读静态源码与断言，未运行Rust或正式玩家入口；同SHA CI的业务job为skipped，不能记为这些测试通过。实现交付PR到本条的完整映射仍未核清。
- 核对日期与基线：2026-10-17；需求、专业设计与静态源码基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；仅GitHub只读源码及CI日志审查，未执行本条业务测试、正式入口、跨节点或真人验收。
- 剩余范围：已确认receipt→owner snapshot与记忆纠正关联的局部生产链；社交/治理/冲突动作尚未进入所读producer显式supported集，须核清各类reason/evidence/stakes/alternative的真实输入、纠正最早生效点和权威结果。真实provider、正式玩家可理解性与跨入口组合验收仍缺。

- 性质：`目标要求`
- 适用条件：记忆驱动、社交、治理或冲突等会改变玩家取舍的 Agent 决策。
- 要求：产品必须让玩家沿同一因果链理解被接受的意图、Agent 理由/证据、stakes、预期后果、替代方向、打断/纠正点及纠正后的结果；提案接受不得被表达为权威规则已应用。
- 理由：高层控制仍应让玩家能修正方向并理解代价，而不是只看到自动化结果。
- 上位承诺：可理解、可干预、可恢复的玩家能动性。
- 专业权威：[`PRD-GAME-014`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)。
- 验收：[AC-WR-IA-003](#ac-wr-ia-003)。

<a id="ac-wr-ia-001"></a>
### AC-WR-IA-001：接受、推进、后果和下一步保持可区分

Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185；声明：[REQ-WR-IA-001](#req-wr-ia-001)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-IA-001。
- 给定：一个玩家提交的目标或 step 意图。
- 当：请求被接受、排队、处理、产生空结果或被阻塞。
- 则：正式入口分别表达接受/拒绝、推进/阻塞、可读后果和下一步；等待、空结果、自动调度、界面计数或重新加载不被包装为世界已推进。

<a id="ac-wr-ia-002"></a>
### AC-WR-IA-002：阻塞和回流保留有效恢复动作

Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63；声明：[REQ-WR-IA-002](#req-wr-ia-002)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-IA-002。
- 给定：玩家离开后路线失效、被替换、改道或当前目标受到阻塞。
- 当：玩家返回、重连或主动查看当前意图。
- 则：玩家能看到当前目标、主要阻塞、最近后果和可执行下一步，并能等待、修复、改道、重新聚焦或安全结束；没有安全路径时返回新的决策面。

<a id="ac-wr-ia-003"></a>
### AC-WR-IA-003：因果解释和纠正结果可追溯

Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376；声明：[REQ-WR-IA-003](#req-wr-ia-003)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-IA-003。
- 给定：一次记忆驱动、社交、治理或冲突决策及其替代方向。
- 当：玩家查看 Agent 理由、提交纠正或打断，并等待权威结果。
- 则：同一 causal-decision receipt 能说明意图、理由/证据、stakes、预期后果、替代、最早生效点和纠正结果，并明确请求/提案接受与权威应用的边界。

### 4.3 未决问题与证据边界

- 尚未决定：哪些间接控制路径进入当前可验证产品入口，以及各入口何时具备同候选的 Agent、runtime、Viewer 和 QA 证据；影响 IA-1 至 IA-7 的当前 verdict，决策负责角色为 `producer_system_designer` 联合 gameplay、Agent、runtime、Viewer 与 QA，触发条件是进入实现或公开 claim 审查前，解决前临时不承诺所有间接控制路径均已支持。
- 本分册证据只能证明指定意图类型、入口、版本/窗口和环境中的因果与续接行为；单次 Agent 行动、持续 world tick、文档建档或局部 green 不能证明玩家理解、完整 API parity 或发行 readiness。

## 5. Non-Goals

- 不把 oasis7 改成第一人称逐帧操控、逐块建造或以增加动作数量代替能动性。
- 不冻结 UI 布局、API 字段、状态枚举、tick、数值、runtime/Agent 实现、测试矩阵或任务状态。
- 不把本分册或 `PRD-GAME-014` 建档包装成留存、active-LLM readiness、QA gate 或公开发行已经通过。
- 不承诺复杂预测、完整分支模拟或完整记忆编辑系统。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-WR-IA-001](#req-wr-ia-001) / [AC-WR-IA-001](#ac-wr-ia-001) | `producer_system_designer` | [`PRD-GAME-014`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#1-executive-summary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-IA-002](#req-wr-ia-002) / [AC-WR-IA-002](#ac-wr-ia-002) | `producer_system_designer` | [`PRD-GAME-014`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#1-executive-summary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-IA-003](#req-wr-ia-003) / [AC-WR-IA-003](#ac-wr-ia-003) | `producer_system_designer` | [`PRD-GAME-014`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#1-executive-summary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
