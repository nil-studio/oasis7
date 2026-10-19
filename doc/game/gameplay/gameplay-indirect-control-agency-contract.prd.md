# Gameplay 间接控制 agency 合同 PRD

- 对应产品设计：[`Gameplay 间接控制 agency 合同设计`](gameplay-indirect-control-agency-contract.design.md)；对应专业系统设计：[`DES-WR-IA`](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md)。长期玩家承诺仍由产品分册承载，本文继续拥有 `PRD-GAME-014` 的 gameplay guarantee、字段语义、状态与失败签名，专业系统设计承接跨 runtime、Agent、Viewer/API 与 QA 的技术边界和验证映射。
- 可变执行状态: 对应 GitHub Project task 与 issue evidence comments
- 产品层长期承诺: [`doc/product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md)；本文继续拥有保证项字段、状态与失败签名，以及 gameplay、runtime、Viewer/API、Agent 和 QA 的专业合同与任务证据。
- 产品叶子入口：[`REQ-WR-IA-001`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)；本文件保留字段、状态、失败签名与专业证据权威。

审计轮次: 1

## 1. Executive Summary

- Problem Statement: oasis7 当前正式主路线已经明确是“通过 agent 间接控制文明”，但还缺少一份正式合同来回答：玩家在不直接操纵每一步执行的前提下，什么时候仍然会觉得“这是我在控制”，而不是“我只是在旁观 AI 自己决定”。缺少这份合同会让 retention、Viewer 表达、action contract 与 playability verdict 继续各说各话。
- Proposed Solution: 新增 `PRD-GAME-014`，把间接控制的 control-feeling 正式拆成一组可验证的交互保证：`intent legibility before/at commit`、`execution acknowledgement with causality`、`interrupt/reprioritize/recover hooks`、`bounded consequence readability`。同时明确这些保证怎样映射到 headed Web/UI、pure API、runtime canonical 语义与 QA gate。
- Success Criteria:
  - SC-1: `game` 根 PRD、`gameplay` 主文档与本专题对“间接控制下何谓玩家仍然在控制”采用统一口径，不再只停留在笼统的“有 goal/progress/blocker/next_step”描述。
  - SC-2: 至少冻结 3 条可验证的 interaction guarantees，并明确每条 guarantee 的字段、状态、失败签名与 owner role。
  - SC-3: headed Web/UI 与 pure API 都能直接回答 4 个问题：`我刚刚让系统做什么`、`系统是否接受了`、`为什么当前这样推进/没推进`、`我现在最有效的下一步是什么`。
  - SC-4: future UX / runtime / agent contract 变更可以被 QA 按本专题直接判定为 `strengthens / preserves / weakens control-feeling`，而不是只看 smoke 是否还能跑。
  - SC-5: 本专题不改变 `PRD-GAME-012` 当前正式 verdict；它负责定义 trust/capability 修复所依赖的“控制感合同”，而不是替代 active-LLM 留存验证本身。
- SC-6: 若 agent 使用长期记忆影响当前行动，玩家必须能看到“记住了什么 / 来自哪里 / 为什么影响当前行动 / 如何纠正”的最小可读合同。
<a id="sc-7"></a>
### SC-7：工业行动的因果与恢复

Agent guardrail、记忆或世界约束若使工业/资源动作改道、裁剪或拒绝，必须保留原 accepted intent、替换/override 原因和下一步恢复面；不得把自动改写或 debug 工具结果包装成玩家已经完成的世界后果。

Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185; O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63；产品适用承接：[REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)、[REQ-WR-IA-002](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)。 本承接仅对应意图/世界结果区分与受阻后的下一决策，不将工业专属规则归入本条。

<a name="2-user-experience-functionality"></a>

## 2. User Experience & Functionality

- User Personas:
  - 新玩家 / 试玩玩家：需要在不直接操控每一步的前提下，仍然感觉“我的决定真的改变了世界”。
  - 回流玩家：需要快速恢复上一次意图、当前执行状态、阻塞原因与最短下一步，而不是重新猜测 agent 在做什么。
  - `producer_system_designer`: 需要把“间接控制是否仍然像控制”冻结成正式可裁决合同。
  - `viewer_engineer`: 需要知道哪些首屏与反馈语义是正式能力地板，而不是可有可无的 polish。
  - `runtime_engineer` / `agent_engineer`: 需要知道哪些 canonical 状态、ack、override、blocked reason 与 resume hook 是产品合同，而不是实现细节。
  - `qa_engineer`: 需要一个比“能点按钮、有日志”更强的 playability 验收基准。
- User Scenarios & Frequency:
  - 首次正式会话：每位新玩家首次进入 `PostOnboarding` 后都会经历。
  - active-LLM 10 分钟 trust gate 回归：每个候选版本至少 3 条正式样本。
  - `PostOnboarding -> first capability gate` 跟踪：每个候选版本至少 1 组 `30` 分钟或 `1~3` 次会话样本。
  - headed Web/UI 或 pure API 行为面变更：每个影响 action/ack/goal/blocker/next_step 的改动至少 1 次合同复核。
- User Stories:
  - PRD-GAME-014: As a 玩家, I want indirect agent gameplay to preserve a concrete sense of control, so that I feel I am directing outcomes rather than merely observing an autonomous simulation.
  - PRD-GAME-014-A: As a 玩家, I want the system to show what intent it accepted and why it changed course or got blocked, so that I can still trust my decisions.
  - PRD-GAME-014-B: As a 回流玩家, I want clear resume and reprioritization hooks, so that I can recover agency without rereading raw logs.
  - PRD-GAME-014-C: As a QA / gameplay owner, I want future UX and runtime changes to be evaluated against an explicit control-feeling contract, so that “still playable” does not silently drift into “technically alive but emotionally passive”.
- Critical User Flows:
  1. Flow-CFC-001: `玩家给出目标/动作 -> 系统在 commit 前展示意图边界或提交后立即回写 accepted intent -> 玩家确认“系统正在执行我刚让它做的事”`
  2. Flow-CFC-002: `执行中出现等待/阻塞/改道 -> 系统把当前状态归类为 executing / blocked / overridden / completed_no_progress 之一 -> 玩家读懂主因果`
  3. Flow-CFC-003: `玩家对当前方向不满意 -> 使用 interrupt / reprioritize / focus goal / change target -> 系统在同一语义面上反馈新旧意图交接`
  4. Flow-CFC-004: `玩家中断会话后返回 -> 系统恢复最近 accepted intent、当前主阻塞、最近可见后果与下一步建议 -> 玩家无需翻 raw history 即可继续`
  5. Flow-CFC-005: `agent 基于长期记忆继续行动 -> 系统展示记忆摘要、来源、当前用途与纠正提示 -> 玩家判断是否继续、修正或重排`
  6. Flow-CFC-006: `QA 评审某项 UX/runtime/agent 变更 -> 对照 guarantees 判断该改动是增强、保持还是削弱 control-feeling -> 输出 blocker 或 pass`
- Functional Specification Matrix:

| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 意图可见性保证 | `accepted_intent_id`、`intent_summary`、`intent_scope`、`intent_target`、`intent_age_ms` | 玩家发出 `play/step/select/gameplay_action/prompt_control` 后，界面或 API 必须能回读当前被接受的主意图 | `implicit -> acknowledged -> stale/replaced` | 最近一次仍有效的 accepted intent 优先；若被新意图取代，旧意图必须显式标记 `replaced` | 已认证玩家可见自己的当前主意图；不得暴露他人私有 prompt 细节 |
| 因果反馈保证 | `execution_status`、`status_reason`、`blocker_type`、`override_reason`、`last_world_change` | 玩家查看当前结果时，必须看到 “accepted / executing / blocked / overridden / completed_no_progress / completed_with_progress” 之一 | `acknowledged -> executing -> blocked/overridden/completed_*` | 当前主目标相关的执行状态优先于历史日志噪音；同一时刻只高亮 1 个主因果 | 玩家可读；runtime/canonical snapshot 为真值来源，Viewer/API 不得各算一套 |
| bounded-response 与 fallback 保证 | `response_window_class`、`stalled_reason`、`escalation_hint`、`fallback_action_id`、`fallback_action_label`、`fallback_tradeoff_preview`、`blocked_intent_id`、`blocker_reason`、`wait_option`、`repair_option`、`reroute_option`、`progress_kept`、`opportunity_cost`、`recommended_fallback_action_id`、`fallback_value_class`；当分类为 `safe_wait` 时还必须提供 `wait_resolution_quote`，包含 `resolution_trigger`、`expected_wait_class`、`next_recheck_tick_or_event`、`state_change_expected`、`wait_risk_if_unresolved`、`alternate_action_unlock_condition`；当分类为 `no_safe_fallback` 时必须提供 `no_safe_fallback_reason`、`required_next_decision_action_id`、`required_next_decision_class` | 若 accepted intent 未形成有效推进，系统必须在一个 bounded-response 窗口内把当前状态升级成 `stalled / blocked / reprioritize_recommended / fallback_ready` 之一；进入 `fallback_ready` 时必须说明、比较、分类并推荐等待、修复、改道或结束当前意图后重新决策，而不是只给单个按钮；`safe_wait` 必须说明在等什么、何时复查和何时换方案；`no_safe_fallback` 必须显式进入 `reprioritize_goal / escalate_constraint / return_to_goal_selection` 之一 | `acknowledged -> executing -> stalled/escalation_ready -> fallback_ready -> waiting/repaired/rerouted/no_safe_fallback -> reprioritized/escalated/goal_selection` | 不允许连续静默等待；同一主意图至多允许 1 次“继续观察”类弱反馈，之后必须升级为明确 fallback 或重排建议；`fallback_value_class` 固定为 `safe_wait / repair_now / reroute_now / no_safe_fallback`。`safe_wait` 只允许绑定 canonical trigger，且 repair/reroute 不可用或其已披露机会成本更差；若无有界触发条件、存在更优可用替代动作，或复查时预期变化未发生，必须重新分类，不得静默延长或伪造 ETA；`no_safe_fallback` 只终止当前 blocked intent，不得终止玩家的下一决策面 | 正式玩家可见；runtime 提供 blocker/recovery 与触发条件真值，Viewer/API 只负责表达；本保证不要求完整恢复系统、quest 系统、respec 系统或 UI 重写 |
| 可打断与重排保证 | `can_interrupt`、`can_reprioritize`、`replacement_intent_summary`、`handoff_result` | 提供 `focus goal`、重新选目标、重新提交 prompt/动作等显式重排入口；不允许只剩“等 AI 自己继续” | `continuing -> interrupt_requested -> reprioritized / resume_required` | 当前主阻塞若持续存在且玩家可采取更高收益动作，应优先暴露 reprioritize hook | 正式玩家可对自己的主意图重排；不允许越权改动其他玩家/组织控制面 |
| 后果可读性保证 | `cost_summary`、`progress_delta`、`world_change_summary`、`next_step_hint` | 每次关键动作后必须给出“付出了什么 / 世界改变了什么 / 下一步最该做什么” | `opaque -> readable -> decision_ready` | 当前主目标相关的成本、产出、阻塞、恢复优先于调试日志与次级事件 | 玩家可读；系统生成，owner 冻结字段语义 |
| 恢复与续玩保证 | `resume_anchor`、`last_accepted_intent`、`primary_blocker`、`resume_next_step` | 玩家重连或回流时，直接恢复当前 agency surface，不要求先读原始事件流 | `fresh_session -> resumed -> replanned` | 优先恢复主目标链最近一次 accepted intent；若旧意图已失效，必须显式写出失效原因 | 已认证玩家可恢复自己的续玩面板/API 字段 |
| 长期记忆可读与纠正保证 | `memory_summary`、`memory_source_event`、`used_for_current_decision`、`player_correction_hint`、`memory_confidence_or_staleness`、`memory_correction_outcome`、`correction_status`、`affected_memory_or_decision_scope`、`earliest_applied_action`、`correction_scope_confidence`、`stale_or_ignored_reason` | 当 agent 引用长期记忆影响当前行动时，展示脱敏摘要、来源、当前用途和纠正提示；玩家提交纠正后必须回写接受或拒绝、影响的记忆或决策范围，以及最早会应用到的下一决策/行动。接受的纠正必须反映在下一条 memory-driven action summary；若未反映，必须给出明确的 stale/ignored reason | `memory_hidden -> memory_referenced -> correction_submitted -> correction_accepted/rejected -> applied/stale/ignored/reconfirmed` | 当前决策实际引用的记忆优先；过期或低置信记忆必须标记 staleness，不得伪装成确定事实。结果只报告作用范围与置信度，不承诺单条纠正必然决定下一行动 | 玩家只能看见自己可访问 agent / session 的脱敏记忆摘要与纠正结果；不得暴露私有 prompt 全文、内部 trace 或其他玩家记忆 |

### 2.1 Reusable causal-decision receipt

Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376；产品适用承接：[REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)。

所有会由 Agent 解释、自动推进、改道或可被玩家修正的动作，共用一份语义上的 causal-decision receipt；它适用于记忆驱动、社交、治理与冲突动作，而不是为每类动作另造一条玩家因果链。receipt 必须让玩家读懂：

1. 被接受的玩家意图是什么；
2. Agent 为什么如此解释或选择推进，及其使用的可读证据或世界约束；
3. 当前 stakes 与预期世界后果；
4. 可选择的替代方向；
5. 可用的打断或纠正动作；
6. 该打断/纠正最早能影响的后续决策点；以及
7. 纠正后的实际结果，或尚未生效/被忽略的可读原因。

请求、提案或玩家意图被接受，只表示系统已承认并会在权威边界内处理它；它不等同于规则已应用、投票已结算、社会关系已改变、冲突已开始或任何预期后果已发生。receipt 必须把“已接受”“正在按规则处理”“权威应用或未应用的结果”区分开，并在被阻塞、改道、被替换或纠正后保留同一条因果链。

本 receipt 定义 gameplay 可读性与验收语义，不新增 runtime 字段、状态枚举、治理规则、Agent 推理保证或 Viewer/API 布局。具体事实、可中断性和何时应用仍以对应权威状态与专业合同为准。

同一边界适用于采集、精炼、移动和排程等工业恢复动作：若 Agent 因资源不足、地点不可达、世界约束或 guardrail 改写而不能按原意图推进，receipt 必须把原意图与替换动作/拒绝原因连接起来，并进入既有的 wait、repair、reroute 或 reprioritize 恢复面。仅 debug/probe 可用的资源注入不属于玩家意图、玩家奖励或正式 playability 样本，不能用于伪造该因果链。

<a id="prd-game-014-acceptance"></a>
### Acceptance Criteria
<a id="ac-1"></a>
#### AC-1：验收条款

验收：本专题至少冻结 5 条 control-feeling guarantees，其中至少 4 条具备可直接验收的字段、状态与失败签名。

<a id="ac-2"></a>
#### AC-2：验收条款

验收：“间接控制仍然像控制”在本专题中被具体定义为：玩家始终能回答 `我让系统做了什么 / 系统有没有接受 / 为什么现在这样 / 我下一步该做什么`，而不是只看到世界在变化。

<a id="ac-3"></a>
#### AC-3：验收条款

验收：当前正式主路线不要求玩家获得第一人称逐帧操控；但若 accepted intent、主因果、打断重排或续玩恢复四者缺一，则不得宣称 control-feeling 合格。

<a id="ac-3a"></a>
#### AC-3A：验收条款

验收：若 accepted intent 之后连续出现无主因果、无升级解释、无 fallback 建议的静默等待，则即便世界仍在 tick，也必须判定为 agency weakened。

<a id="ac-3b"></a>
#### AC-3B：验收条款

验收：若系统进入 `fallback_ready` 但缺少 `fallback_tradeoff_preview`，或只能展示单个推荐动作而无法比较 `wait / repair / reroute` 的成本、保留进度、损失机会和推荐理由，则判定为 `fallback_tradeoff_missing`，不得把 fallback CTA 本身当作 control-feeling 通过。

Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63；产品适用承接：[REQ-WR-IA-002](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)。

<a id="ac-3c"></a>
#### AC-3C：验收条款

验收：若系统推荐 `safe_wait` 但缺少完整 `wait_resolution_quote`，玩家无法回答“在等什么、何时复查、何时换方案”，或到了复查 tick/event 仍未发生预期状态变化却继续静默等待，则判定为 `safe_wait_resolution_missing`；系统必须重新分类为 `repair_now / reroute_now / no_safe_fallback`。若重新分类为 `no_safe_fallback` 却缺少原因和 `reprioritize_goal / escalate_constraint / return_to_goal_selection` 之一的下一决策动作，则判定为 `no_safe_fallback_dead_end`，不得通过 control-feeling 验收。

Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63；产品适用承接：[REQ-WR-IA-002](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)。

<a id="ac-4"></a>
#### AC-4：验收条款

验收：本专题必须显式承接 `PRD-GAME-012` 第 4 条 lane“间接控制因果与下一步”，并解释它与 `PRD-GAME-004` 微循环反馈可见性、`PRD-GAME-007` PostOnboarding、`PRD-GAME-008` pure API parity 的边界。

<a id="ac-5"></a>
#### AC-5：验收条款

验收：headed Web/UI 与 pure API 都必须具备同等级的 agency surface；API 可以更简洁，但不允许缺少 accepted intent、主因果、阻塞分类或 next-step 语义。

<a id="ac-6"></a>
#### AC-6：验收条款

验收：若系统只是展示原始日志、世界 tick 或泛化状态，而不能把主意图和当前结果绑定到同一语义面，则判定为 control-feeling 失败。

<a id="ac-7"></a>
#### AC-7：验收条款

验收：若玩家无法显式中断、改道或重新聚焦主目标，只能被动等待 agent 自行推进，则判定为 agency weakened，即便世界仍在持续运行。

<a id="ac-8"></a>
#### AC-8：验收条款

验收：本专题必须给出未来 UX/runtime/agent 变更的判据：增强型变更至少强化 1 条 guarantee 且不削弱其余 guarantee；任何削弱 accepted intent、主因果、重排入口或续玩恢复的变更都必须经 `producer_system_designer` 显式裁决。

<a id="ac-9"></a>
#### AC-9：验收条款

验收：本专题完成后，`game` 根 PRD / project、`gameplay` 主文档、索引与当前 task execution log 必须互链到 `PRD-GAME-014` 与 `TASK-GAME-071~075`。

<a id="ac-10"></a>
#### AC-10：验收条款

验收：当前阶段口径继续保持 `internal_playable_alpha_late`，且本专题自身不允许被单独包装成“issue #160 已解决”或“正式留存已恢复”；`#160` 的 closeout 必须继续以独立 formal evidence 为准，而不是把 control-feeling 文档当作替代证据。

<a id="ac-11"></a>
#### AC-11：验收条款

验收：若 agent 当前行动引用长期记忆，headed Web/UI 与 pure API 至少必须能回答 `记住了什么 / 来自哪里 / 影响当前哪一步 / 玩家如何纠正`；若只能看到 agent 自行行动或原始日志，判定为 agency weakened。

<a id="ac-12"></a>
#### AC-12：验收条款

验收：若玩家已提交长期记忆纠正，headed Web/UI 与 pure API 至少必须能回答 `是否接受 / 影响哪段记忆或哪类决策 / 最早在哪个下一行动应用`；接受结果若未出现在下一条 memory-driven action summary，必须说明 `stale_or_ignored_reason`，否则判定为 `memory_correction_outcome_missing`。

<a id="ac-13"></a>
#### AC-13：验收条款

验收：记忆驱动、社交、治理与冲突动作使用同一 causal-decision receipt，且能回答 accepted intent、Agent reason/evidence、stakes/expected consequence、alternative、interrupt/correction、earliest effective point 与 post-correction result；若把请求/提案接受写成权威应用，或纠正后只给状态不说明结果，判定为 `causal_decision_receipt_missing`。

Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376；产品适用承接：[REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)。

### Non-Goals
  - 不把 oasis7 改成第一人称直接操作或逐块建造游戏。
  - 不在本专题中直接修复 active-LLM provider、runtime freeze 或 capability gate 本身；这些仍由对应实现任务负责。
  - 不把“更多按钮”误当作 control-feeling 改善；本专题关注的是 agency 合同，而不是动作数量膨胀。
  - 不要求 v1 引入复杂概率预览、未来分支树模拟器或完整 plan diff 可视化。
  - 不要求 v1 做完整记忆编辑器、向量检索、外部记忆 backend 或全量遗忘权治理；本专题只要求最小可读摘要与纠正提示。

## 3. AI System Requirements (If Applicable)

- Tool Requirements:
  - active-LLM 正式游玩样本与 `agent-browser` headed Web/UI 证据，用于验证 trust gate 语义面是否满足 control-feeling 合同。
  - pure API 对账脚本与 canonical snapshot 检查，用于验证 accepted intent / status / blocker / next_step 没有只留在 UI 私有组装层。
  - playability 卡片与 QA evidence，用于记录“玩家是否真的感觉在控制”而非仅依赖技术成功率。
- Evaluation Strategy:
  - 先做合同级评估，再做样本级评估。合同级评估检查 guarantees 是否在 surface 上存在且一致；样本级评估再看玩家能否在真实 session 中持续读懂 accepted intent、主因果、重排行为与下一步。
  - 对 active-LLM 正式样本，若出现 `world time 没推进`、`goal regress`、`override without explanation`、`raw log 抢焦点`、`resume only through debug history` 中任一签名，则 control-feeling 至少有 1 条 guarantee 失败。

## 4. Technical Specifications

- Architecture Overview:
  - `PRD-GAME-014` 是 gameplay 层的交互合同，不新增单独玩法系统，而是收束已有 `player_gameplay`、goal chain、action ack、blocker taxonomy 与 feedback surfaces 的共同完成定义。
  - runtime / canonical snapshot 负责提供 accepted intent、execution status、blocker/override reason、world change summary、resume anchor 等真值字段。
  - Viewer 与 pure API 负责把这些字段呈现为玩家可用的 agency surface，而不是让玩家去拼原始日志。
  - QA 负责把这些 guarantees 固化为验证矩阵，并在 trust/capability 样本中独立记录哪一条 guarantee 失效。
- Integration Points:
  - `doc/game/prd.md`
  - 对应 GitHub task issue evidence
  - `doc/game/prd.index.md`
  - `doc/game/gameplay/gameplay-top-level-design.prd.md`
  - `doc/game/gameplay/gameplay-top-level-design.prd.md`
  - `doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md`
  - `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md`
  - `doc/game/gameplay/gameplay-top-level-design.prd.md`（PRD-GAME-012 stable early-retention contract）
  - `crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_snapshot.rs`
  - `crates/oasis7_client_cli/src/bin/oasis7_pure_api_client.rs`
  - `crates/oasis7_viewer/viewer.js`
  - `crates/oasis7_viewer/software_safe_src/main.jsx`
  - `crates/oasis7_viewer/software_safe_src/legacy_core.js`
  - `testing-manual.md`
- Edge Cases & Error Handling:
  - accepted intent 已被新动作替换，但表面仍像旧意图在执行：必须显式标记 `replaced` 或 `reprioritized`，否则判定为因果漂移。
  - agent 因世界约束自行改道，但玩家只看到“执行了别的事”而没有 override reason：判定为 agency break，而不是普通日志缺失。
  - accepted intent 已确认，但连续超过一个 bounded-response 窗口仍只有弱反馈或泛化 loading：必须升级为 `stalled`、`blocked` 或显式 fallback 建议，不能继续伪装成“正常执行中”。
  - world 有变化，但当前主目标没有任何 progress/cost/next-step 解释：不得把“世界活着”视为 control-feeling pass。
  - pure API 能读到 stage/goal，却读不到 accepted intent 或 interruption hook：判定为 parity 不完整，不得宣称 API 也满足 control-feeling 合同。
  - 玩家重连后只能从 raw history 猜测最近状态，而没有恢复锚点：判定为 resume guarantee 失败。
  - agent 基于长期记忆行动，但玩家看不到记忆摘要、来源或当前用途：判定为 memory-driven agency break。
  - agent 基于过期或错误记忆继续行动，但没有纠正提示：判定为 `memory_correction_missing`，不得把长期记忆包装成续玩增强。
  - 玩家已纠正长期记忆，但系统未展示接受/拒绝、影响范围或最早应用行动，或已接受的纠正未在下一条 memory-driven action summary 中反映且没有明确 stale/ignored reason：判定为 `memory_correction_outcome_missing`。
  - 系统提供过多 operator/debug 语义，淹没当前主意图与主因果：判定为 presentation-level control-feeling regression。
  - active-LLM lane 因 provider 问题卡死时，不得用 deterministic `--no-llm` 样本代替本专题正式验收；debug lane 只能帮助定位哪条 guarantee 先失效。
<a id="prd-game-014-nfr"></a>
- Non-Functional Requirements:
  - NFR-CFC-1: `PRD-GAME-014` 的 active 入口互链必须在 1 个工作日内完成，并可通过 grep 直接定位到 game 根 PRD、gameplay 主文档、索引、专题 PRD、系统设计与产品分册。
  - NFR-CFC-2: headed Web/UI 与 pure API 的 control-feeling 关键字段覆盖率必须为 100%：`accepted_intent`、`execution_status`、`primary_reason`、`next_step` 四类字段不得缺任一类。
  - NFR-CFC-3: 任何导致 accepted intent 与当前世界结果脱钩、且没有 override/replaced 解释的回归，都必须被 QA 标记为 blocker，而不是低优先级文案问题。
  - NFR-CFC-4: control-feeling 合同验证必须可在 fresh bundle 本地复跑，并能区分 formal active-LLM lane 与 debug/probe lane。
  - NFR-CFC-5: 本专题下所有 follow-up 结论必须继续遵守当前 claim envelope，不得借“控制感合同已定义”扩大对外承诺。
  - NFR-CFC-6: 在正式玩家 surface 上，单个 accepted intent 不得连续经历两次无 fallback 的静默等待周期；若第一次弱反馈后仍无有效推进，第二次必须升级为 `reprioritize`、`repair` 或 `fallback_ready`。
  - NFR-CFC-7: 长期记忆影响当前 agent 行动时，玩家可读记忆摘要覆盖率必须为 `100%`；摘要可以脱敏和压缩，但不得只存在于内部 trace。
- Security & Privacy:
  - 本专题不新增玩家隐私采集；accepted intent 与 resume anchor 只要求表达 canonical 玩家状态，不要求暴露私有 prompt 全文或实现内部 trace。
  - 长期记忆可读性只要求脱敏摘要、来源类型和当前用途；不得暴露私有 prompt 全文、内部 chain trace 或其他玩家私有记忆。

## 5. Risks & Roadmap

- Phased Rollout:
  - R0: 冻结 `PRD-GAME-014`，完成根入口、主文档、索引与任务映射。
  - R1: 对齐 current canonical surface，把 accepted intent / 主因果 / next-step / resume anchor 的正式合同写入 runtime/viewer/API 语义面。
  - R2: 补齐 interrupt / reprioritize / resume hooks 的实现与表面一致性。
  - R3: QA 建立 control-feeling matrix，并将其接入 `PRD-GAME-012` trust/capability 样本复核。
  - R4: 只有在上述 guarantees 稳定后，才允许继续讨论更复杂的 plan preview、branch simulation 或更宽动作面。
- Technical Risks:
  - 风险-1: 如果只补 UI 文案、不补 canonical accepted intent / override / resume truth，合同会再次退化成展示层口号。
  - 风险-2: 如果把 control-feeling 简化为“世界在动、按钮有响应”，会继续把系统原型误判为足够像游戏。
  - 风险-3: 如果为了“更像控制”而盲目扩动作面，反而会打散当前间接控制主路线和 retention 修复优先级。

## 6. Validation & Decision Record

- Test Plan & Traceability:

| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-GAME-014 | `TASK-GAME-071` | `test_tier_required` | 文档治理检查、根入口/主文档/索引/执行日志互链核验 | control-feeling 合同冻结与任务挂载 |
| PRD-GAME-014 | `TASK-GAME-072` | `test_tier_required` | canonical snapshot / feedback surface 对账，确认 accepted intent、execution status、override/blocker reason、next-step truth 已定义 | runtime / viewer / player_gameplay 语义一致性 |
| PRD-GAME-014 | `TASK-GAME-073` | `test_tier_required` | formal Web entry 与 pure API agency surface 对账，人工复核主意图、主因果、重排入口、续玩恢复 | headed Web/UI 与 pure API control-feeling surface |
| PRD-GAME-014 | `TASK-GAME-074` | `test_tier_required` | dual-mode / action contract / interruption semantics 对账，确认重排与打断能力不再隐形 | agent contract、override 解释与 reprioritize hook |
| PRD-GAME-014 | `TASK-GAME-075` | `test_tier_required` + `test_tier_full` | QA control-feeling matrix、active-LLM trust samples、pure API parity 抽样与 blocker 签名归档 | 正式 control-feeling 合同、trust/capability 样本解释力 |

- Decision Log:

| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-CFC-001 | 将 control-feeling 定义为一组 interaction guarantees，而不是一句“间接控制也要有控制感” | 继续让留存、Viewer、runtime、QA 各自用模糊语言描述 | issue #164 的问题不是缺观点，而是缺一份可裁决、可实现、可测试的正式合同。 |
| DEC-CFC-002 | 把 accepted intent、主因果、打断重排、续玩恢复列为 agency 地板 | 只要求“世界在动”或“有 next_step 提示” | 玩家真正缺的不是更多世界变化，而是缺“这是我造成的、我现在能改什么”的稳定心智模型。 |
| DEC-CFC-003 | 本专题承接 `PRD-GAME-012` 第 4 条 lane，但不替代 trust/capability gate | 直接把 issue #164 写成 retention gate verdict 本身 | control-feeling 是 formal gate 的前置合同，不是 gate 结果本身；混写会再次污染 verdict 语义。 |
| DEC-CFC-004 | 继续坚持间接控制主路线，通过更强的 agency surface 解决“像旁观 AI”的问题 | 直接把产品方向改成更多 direct control 或 embodied 操作 | 当前主问题是间接控制表达不够完整，而不是缺一套完全不同的直接控制产品方向。 |
| DEC-CFC-005 | 将长期记忆纳入 control-feeling 地板，要求玩家可读摘要、来源、当前用途和纠正提示 | 只做长期记忆持久化 roundtrip，或暴露完整 prompt/log 让玩家自行判断 | 记忆会改变 agent 行动；玩家需要理解和修正其影响，但不应被迫阅读隐私敏感或过长的内部 trace。 |
