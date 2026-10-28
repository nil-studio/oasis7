# LLM 工厂闭环策略稳定性优化（llm_bootstrap）

历史计划边界：LFO0–4、LFO7/9/11 的里程碑是 2026-02 旧实施规划，原 Task 未唯一定位，过程原件保留作固定来源，不作为当前执行队列或验收通过。Recovery Policy、统计字段、typed turn、parse/guardrail/decision_rewrite 与兼容/验证方法仍是本专题的专业内容；当前输入边界服从 builtin 输入合同。

- 对应设计文档: `doc/world-simulator/llm/llm-factory-strategy-optimization.design.md`
- 专题入口与权威边界: `doc/world-simulator/llm/README.md`

审计轮次: 5

## 1. Executive Summary
- 在 `llm_bootstrap`（20 tick 默认口径）下，将“可建厂但未稳定排产”的问题从能力缺口转为可度量、可回归的策略问题。
- 引入“失败原因 -> 下一动作”强规则，减少无效重复 `harvest_radiation`，推动动作链路稳定收敛到 `refine_compound -> build_factory -> schedule_recipe`。
- 增强闭环可观测性：报告中直接统计动作触发率与关键动作首达 tick。
- 当策略把硬件不足恢复到 `refine_compound` 时，必须提供精炼净收益 / 电力机会成本预览，避免玩家只看到动作改写和执行结果。
- 建立固定 mock 输出序列回归，保证策略主链路可在离线测试稳定复现。

## 2. User Experience & Functionality
- In Scope:
  - 在 LLM prompt 组装中新增拒绝原因恢复规则模板（Recovery Policy），并将最近拒绝原因结构化透出给模型。
  - 在 `oasis7_llm_agent_demo` 报告增加动作级指标（`action_kind_counts` 等），用于量化建厂/排产触发率。
  - 新增 deterministic mock-sequence 回归，覆盖“资源不足恢复 -> 建厂 -> 排产”成功链路。
  - 补充 `test_tier_required` 单测与最小在线复跑口径。
- Out of Scope:
  - 不切换到 runtime M4 完整生产链路。
  - 不引入模型训练、RL 或外部策略服务。
  - 不改动 viewer 渲染层。

## 3. AI System Requirements (If Applicable)
- N/A: 本专题不新增 AI 专属要求。

## 4. Technical Specifications

<a id="lfo-recovery-source"></a>

### 1) Prompt 恢复规则（TODO-1）
- 位置：`crates/oasis7/src/simulator/llm_agent/prompt_assembly.rs`
- 设计：
  - 在“决策约束”段新增固定恢复规则表（文本模板 + 可枚举 reject reason）。
  - 明确“当上一步因资源拒绝时，下一个动作必须是补资源动作；禁止连续超过 N 次相同采集动作”。
- 规则初版（建议）：
  - `insufficient_resource.hardware -> refine_compound`（`compound_mass_g` 下限由模板给出）
  - `insufficient_resource.electricity -> harvest_radiation`
  - `factory_not_found -> build_factory`
  - `agent_already_at_location -> schedule_recipe | refine_compound`（禁止重复 move）
<a id="lfo-refine-source"></a>

- 当恢复规则推荐 `refine_compound` 时，后续实现必须通过 strategy hint，或通过升级后的 observation schema version，暴露 `refine_quote` 摘要；当前文档仅冻结恢复建议的可读性字段，不声明现行 `oc_dual_obs_v1` observation 或公共 `last_action` 已携带这些字段：
  - `compound_mass_g`
  - `electricity_cost`
  - `hardware_output`
  - `electricity_after`
  - `hardware_shortfall_before`
  - `hardware_shortfall_after`
  - `first_goal_relevance`
  - `recommended_refine_amount`
  - `refine_value_class`
- 数据透出：
  - 若未来将恢复建议诊断写入 prompt observation 或公共 `last_action`，必须同步更新 dual-mode provider contract 与 schema version；本轮文档不改变当前 observation/action schema：
    - `last_action.kind`
    - `last_action.success`
    - `last_action.reject_reason`
    - `last_action.refine_quote_missing`（future routed diagnostic：当硬件不足恢复建议缺少精炼预览时为 true；若进入 observation，必须同步更新 dual-mode provider contract 与 schema version）
<a id="lfo-power-source"></a>

- 当恢复规则推荐 `harvest_radiation`、`BuyPower` 或等待发电时，后续实现必须通过 strategy hint，或通过升级后的 observation schema version，暴露 `power_survival_quote` / `energy_recovery_preview` 摘要；当前文档仅冻结玩家可读性字段，不声明现行 `oc_dual_obs_v1` observation 或公共 `last_action` 已携带这些字段：
  - `current_power_level`
  - `power_state_before`
  - `recovery_action`
  - `power_gain_estimate`
  - `price_or_time_cost`
  - `power_state_after`
  - `survival_runway_ticks`
  - `next_action_affordability`
  - `shutdown_avoidance_reason`
  - `recommended_power_action`
  - `last_action.power_survival_quote_missing`（future routed diagnostic：当电力不足恢复建议缺少 runway / 防停机预览时为 true；若进入 observation，必须同步更新 dual-mode provider contract 与 schema version）

<a id="lfo-metrics-source"></a>

### 2) 报告动作触发率指标（TODO-2）
- 位置：`crates/oasis7_provider_tools/src/bin/oasis7_llm_agent_demo.rs`（及其 report 结构）
- 新增字段（JSON）：
  - `action_kind_counts: { "<action_kind>": <u64> }`
  - `action_kind_success_counts: { "<action_kind>": <u64> }`
  - `action_kind_failure_counts: { "<action_kind>": <u64> }`
  - `first_action_tick: { "<action_kind>": <u64|null> }`
- 最小验收指标：
  - `build_factory`、`schedule_recipe` 必须可在报告中独立统计。
  - 可通过报告快速判断“是否触发过建厂/排产、首达在第几 tick”。

<a id="lfo-mock-source"></a>

### 3) 固定序列离线回归（TODO-3）
- 位置：
  - `crates/oasis7/src/simulator/llm_agent/tests.rs`
  - `crates/oasis7/src/simulator/llm_agent/tests_part2.rs`
  - 可按需继续扩展 `crates/oasis7/src/simulator/llm_agent/tests.rs` 或新增分段测试文件
- 用例设计：
  - 用例 A（成功主链路）：
    - mock 决策序列：`build_factory`（失败: insufficient hardware）-> `refine_compound` -> `build_factory`（成功）-> `schedule_recipe`（成功）。
    - 断言：最终存在工厂，`data` 增长，事件包含 `FactoryBuilt`/`RecipeScheduled`。
  - 用例 B（反循环约束）：
    - mock 连续 `harvest_radiation` 超阈值后，验证重规划约束触发并切换动作建议。
    - 断言：不会无限重复同一无效动作签名。

## 5. Risks & Roadmap
- LFO0：设计与项目文档立项。
- LFO1：Prompt 恢复规则接线 + 相关单测。
- LFO2：报告动作指标接线 + JSON 回归断言。
- LFO3：固定序列离线回归补齐（成功链路/反循环）。
- LFO4：在线 `llm_bootstrap` 复跑，更新结果与阈值（20 tick 与 30 tick 对比）。

### Technical Risks
- 规则过强风险：过度硬编码可能压制模型在复杂场景的探索能力。
- 决策误读风险：若 `schedule_recipe -> refine_compound` 只作为 guardrail 改写结果出现，玩家/模型会知道“该精炼”但不知道电力是否值得花。
- 指标兼容风险：报告 JSON 新字段需保持向后兼容（新增不破坏旧消费方）。
- 测试漂移风险：离线 mock 策略与在线真实模型行为仍可能存在差异，需双口径保留（离线确定性 + 在线抽样）。

<a id="lfo-tool-source"></a>

## LFO7 增量设计：LLM 决策全量 Tool 化（2026-02-17）

### 目标
- 将 `llm_agent` 的“最终决策”从文本 JSON 输出升级为与查询模块一致的 tool call 协议，消除“同轮多段 JSON / `---` 分隔”的不确定行为。
- 建立单一调用面：查询能力与最终动作均通过 OpenAI Responses tools 触发，减少输出协议分叉。

### 范围
- In Scope:
  - 新增最终决策提交工具（`agent_submit_decision`），覆盖 `wait/wait_ticks/move/harvest/transfer/refine/build/schedule/execute_until`。
  - 请求 payload 改为 `tool_choice=required`，强制模型通过 tool call 输出。
  - tool call 结果统一映射到内部决策解析器；保留现有 guardrail 与执行链路。
  - 更新 prompt 协议文案与相关测试，确保“输出约束”从 JSON 迁移到 tool call。
- Out of Scope:
  - 不改动世界动作语义与资源经济参数。
  - 不引入新的 runtime action 类型。
  - 不调整 viewer/web 可视化链路。

### 接口 / 数据
- 工具扩展（Responses tools）：
  - 现有查询类工具保留：
    - `agent_modules_list`
    - `environment_current_observation`
    - `memory_short_term_recent`
    - `memory_long_term_search`
  - 新增决策工具：
    - `agent_submit_decision`
- 决策工具参数：
  - 包含 `decision` 与对应可选字段（如 `ticks/to/max_amount/...`）。
  - `execute_until` 通过 `action/until/max_ticks` 传递。
  - `message_to_user` 继续作为可选字段透传。
- 兼容策略：
  - 运行时优先消费 function_call；若无 function_call 则视为协议违规并进入修复回路。

### 里程碑
- LFO7.1 设计与项目文档回填。
- LFO7.2 完成 tools/解析链路改造（含 `tool_choice=required`）。
- LFO7.3 完成 prompt 协议迁移与单测更新。
- LFO7.4 required-tier 测试通过并记录 devlog。

### 风险
- 工具 schema 过宽风险：若参数约束不足，可能导致 tool 参数语义漂移，需依赖解析/guardrail 兜底。
- 模型兼容风险：强制 required 后，少数模型可能出现空调用或异常中止，需要明确错误路径与修复提示。
- 回归覆盖风险：历史“文本 JSON 直出”路径若完全关闭，需确保现有测试全部迁移到 tool-first 口径。

<a id="lfo-typed-source"></a>

## LFO9 增量设计：Tool-only 类型化输入收敛（2026-02-17）

### 目标
- 删除 OpenAI SDK 解析失败时的 raw-body JSON fallback（立即移除兼容路径 #1），避免双入口行为差异。
- 保留 tool 参数与决策语义解析（#2/#5 必留），但移除“function_call 先转字符串 JSON 再反向解析”的中间层（#3/#4）。
- 将 `behavior_loop` 的主路径改为消费类型化 turn 数据，减少字符串拼接/拆分引入的不确定性。

### 范围
- In Scope:
  - `OpenAiChatCompletionClient::complete` 删除 `ParseBody(raw)` fallback，统一走 SDK typed response。
  - `openai_payload` 输出从 `String` 中间 JSON 改为 typed turn（决策 turn / 模块调用 turn）。
  - `decision_flow` 新增 typed turn 解析入口，`behavior_loop` 改为直接消费 typed turn。
  - 更新单测：删除 raw fallback 用例，迁移为 typed turn 转换与行为回归用例。
- Out of Scope:
  - 不变更工具集合与 `tool_choice=required` 策略。
  - 不改动经济参数、动作语义与 world kernel 行为。
  - 不处理 `execute_until + wait` 语义优化（该项仍属 TODO-5）。

### 接口 / 数据
- `LlmCompletionResult` 增加 typed turn 列表字段（用于行为层解析），`output` 字符串保留用于 trace/debug 展示。
- typed turn 至少包含：
  - 决策 turn：决策 payload（JSON value）
  - 模块调用 turn：`module` + `args`
- 解析职责：
  - `openai_payload`：负责 function_call 到 typed turn 的归一化映射。
  - `decision_flow`：负责 typed turn 到 `ParsedLlmTurn` 的语义解析与错误归类。

### 里程碑
- LFO9.1 文档更新与任务拆解。
- LFO9.2 删除 raw-body fallback（#1）。
- LFO9.3 完成 typed turn 重构并删除字符串中间解析链路（#3/#4）。
- LFO9.4 完成 required-tier 回归并回写结果。

### 风险
- provider 兼容风险：移除 fallback 后，SDK 解析失败将直接报错；需通过测试确认常用 provider 响应兼容。
- 迁移回归风险：历史测试大量依赖字符串输出，需同步改造 mock client 构造路径，避免误报。
- trace 可读性风险：若 typed turn 序列化展示不稳定，会影响调试日志可读性。

<a id="lfo-rewrite-source"></a>

## LFO11 增量设计：TODO-5/TODO-6 收口（2026-02-17）

### 目标
- 收敛 `execute_until + wait` 语义：当模型在 `execute_until.action` 里给出 `wait/wait_ticks` 时，不再直接判定 parse_error，而是改写为最小可执行动作继续推进。
- 补齐“决策改写回执”可观测性：将 guardrail/解析层改写结果结构化记录到 trace，并回灌到下一轮 observation 的 `last_action` 语境。
- 保持 tool-only 闭环稳定：避免引入新的协议分叉，维持现有 `test_tier_required` 稳定通过。

### 范围
- In Scope:
  - `decision_flow`：为 `execute_until.action=wait/wait_ticks` 增加可执行改写路径（默认改写为最小 `harvest_radiation(max_amount=1)`）。
  - `behavior_loop`：消费结构化改写回执，并将其写入 `llm_step_trace`/system message。
  - `llm_agent` observation 组装：在 `last_action` 中增加 `decision_rewrite` 字段，用于下一轮 prompt 恢复语义。
  - 增加/更新 `test_tier_required` 单测覆盖：`execute_until + wait` 改写、`schedule_recipe -> refine_compound` 改写回执回灌。
- Out of Scope:
  - 不调整经济参数（配方成本、精炼参数、电力参数）。
  - 不引入新的 world action 类型。
  - 不改动 viewer UI 展示协议。

### 接口 / 数据
- 新增结构化回执字段（内部）：
  - `decision_rewrite.from`
  - `decision_rewrite.to`
  - `decision_rewrite.reason`
- prompt observation 变更：
  - `last_action` 保持现有字段（`kind/success/reject_reason`）；
  - 增加可选 `last_action.decision_rewrite`，用于向模型解释“上一轮请求为何被改写”。
- `execute_until` 解析策略：
  - 若 `action` 为可执行动作（Act），保持原逻辑；
  - 若 `action` 为 `wait/wait_ticks`，改写为最小可执行动作并产出结构化 `decision_rewrite`，而非抛出 parse_error。

### 里程碑
- LFO11.1 更新设计/项目文档并拆解任务。
- LFO11.2 完成 `execute_until + wait` 语义收敛（TODO-5）。
- LFO11.3 完成决策改写回执 trace + observation 回灌（TODO-6）。
- LFO11.4 跑通 required-tier 回归并回填文档/devlog。

### 风险
- 过度自动改写风险：错误改写可能掩盖模型真实策略问题；通过结构化回执保留可审计性。
- 行为漂移风险：`wait` 被改写为动作可能改变资源曲线；通过“最小动作 + 回归测试”控制影响范围。
- 可观测性噪声风险：回执过多可能污染 prompt；通过仅在“决策类型发生变化”时回灌降低噪声。

## 6. Validation & Decision Record

- 本文记录可复核的策略/协议约束和验证方法；历史执行任务、状态与
  原始运行记录由对应 PR、实际验证记录与 Git history 追溯。
- 离线 mock 回归和单次在线抽样分别证明其明确覆盖的行为，不能彼此
  替代，也不能单独证明跨 provider 稳定性、成本边界或默认启用资格。

## 7. 既有义务定位与独立验收

以下只为上述已采用条款建立精确接收；不是新增算法、产品范围或执行队列。每项的原字段、参数 authority、排除范围及历史里程碑均保留。验证均为 planned，静态 source/测试存在不表示行为通过；离线与在线不可互相代签。

<a id="lfo-recovery"></a>
### 既有义务：失败恢复与反循环

Trace refs: O7-REQ-fffbfcf7-4ab2-46b5-9771-bc14276f9521；对应产品根 [SC-7 builtin 子范围](../../product/agents-world-simulation/prd.md#sc-agent-007-factory-recovery)。

失败原因驱动补救并避免无效重复，使 builtin 工业策略收敛到建厂和排产。 原合同见[原始条款](#lfo-recovery-source)；恢复目标为原强制要求；初版四项映射仍是建议，N 与模板下限不在此重定。

<a id="ac-lfo-recovery"></a>
#### 验收关系：失败恢复与反循环

Trace refs: O7-REQ-fffbfcf7-4ab2-46b5-9771-bc14276f9521；验收上述[同一义务](#lfo-recovery)，不另分配身份。按[专业验证设计](llm-factory-strategy-optimization.design.md#ver-lfo-recovery)分别核对适用输入、结果和失败边界，当前未执行。

<a id="lfo-metrics"></a>
### 既有义务：动作触发、成败与首达可分辨

Trace refs: O7-REQ-ff9e304a-1782-434c-a344-f5377affe3a3；对应产品根 [SC-2 builtin 子范围](../../product/agents-world-simulation/prd.md#sc-agent-002-factory-metrics)。

报告可独立判断建厂/排产是否触发、成败次数与首达 tick。 原合同见[原始条款](#lfo-metrics-source)；四个报告 map 保留原语义，不新增触发率分母或首次成功 tick。

<a id="ac-lfo-metrics"></a>
#### 验收关系：动作触发、成败与首达可分辨

Trace refs: O7-REQ-ff9e304a-1782-434c-a344-f5377affe3a3；验收上述[同一义务](#lfo-metrics)，不另分配身份。按[专业验证设计](llm-factory-strategy-optimization.design.md#ver-lfo-metrics)分别核对适用输入、结果和失败边界，当前未执行。

<a id="lfo-tool-only"></a>
### 既有义务：最终决策工具协议

Trace refs: O7-REQ-67e3afba-3cce-4e00-b84e-7be5960020f0；对应产品根 [SC-2 builtin 子范围](../../product/agents-world-simulation/prd.md#sc-agent-002-factory-tool-only)。

builtin 最终决策经既定工具提交，缺 function_call 不成为有效最终决策。 原合同见[原始条款](#lfo-tool-source)；LFO7 的 required、查询工具与 guardrail/修复边界保持，不扩大工具或动作集合。

<a id="ac-lfo-tool-only"></a>
#### 验收关系：最终决策工具协议

Trace refs: O7-REQ-67e3afba-3cce-4e00-b84e-7be5960020f0；验收上述[同一义务](#lfo-tool-only)，不另分配身份。按[专业验证设计](llm-factory-strategy-optimization.design.md#ver-lfo-tool-only)分别核对适用输入、结果和失败边界，当前未执行。

<a id="lfo-typed-input"></a>
### 既有义务：类型化决策输入单路径

Trace refs: O7-REQ-265317d7-916b-439e-8668-30123bbfdaab；对应产品根 [SC-2 builtin 子范围](../../product/agents-world-simulation/prd.md#sc-agent-002-factory-typed-input)。

builtin 行为层消费类型化 turn，SDK 解析失败不启用 raw-body JSON 后备入口。 原合同见[原始条款](#lfo-typed-source)；LFO9 删除字符串往返不删除 tool 参数/决策语义校验；output 仍供 trace/debug。

<a id="ac-lfo-typed-input"></a>
#### 验收关系：类型化决策输入单路径

Trace refs: O7-REQ-265317d7-916b-439e-8668-30123bbfdaab；验收上述[同一义务](#lfo-typed-input)，不另分配身份。按[专业验证设计](llm-factory-strategy-optimization.design.md#ver-lfo-typed-input)分别核对适用输入、结果和失败边界，当前未执行。

<a id="lfo-nested-wait"></a>
### 既有义务：已解析内嵌等待的既定处理

Trace refs: O7-REQ-bdf9e6ce-431b-4bbc-a1d0-4eaecab050e1；对应产品根 [SC-7 builtin 子范围](../../product/agents-world-simulation/prd.md#sc-agent-007-factory-nested-wait)。

旧 builtin lane 的 execute_until 内嵌 wait/wait_ticks 按已选最小动作改写继续推进。 原合同见[原始条款](#lfo-rewrite-source)；仅限 LFO11 已解析的内嵌等待；不是 provider 失败 fallback，也不是 native durable Wait。

<a id="ac-lfo-nested-wait"></a>
#### 验收关系：已解析内嵌等待的既定处理

Trace refs: O7-REQ-bdf9e6ce-431b-4bbc-a1d0-4eaecab050e1；验收上述[同一义务](#lfo-nested-wait)，不另分配身份。按[专业验证设计](llm-factory-strategy-optimization.design.md#ver-lfo-nested-wait)分别核对适用输入、结果和失败边界，当前未执行。

<a id="lfo-rewrite-receipt"></a>
### 既有义务：改写原因可追踪

Trace refs: O7-REQ-876ef676-3032-4d78-989a-9d5af5971f43；对应产品根 [SC-2 builtin 子范围](../../product/agents-world-simulation/prd.md#sc-agent-002-factory-rewrite-receipt)。

builtin 的决策改写保留 from/to/reason，并在 trace 与下一轮语境解释原请求为何变化。 原合同见[原始条款](#lfo-rewrite-source)；LFO11 的已有 decision_rewrite 与两个 future quote diagnostics 分开，回执不证明成本预览。

<a id="ac-lfo-rewrite-receipt"></a>
#### 验收关系：改写原因可追踪

Trace refs: O7-REQ-876ef676-3032-4d78-989a-9d5af5971f43；验收上述[同一义务](#lfo-rewrite-receipt)，不另分配身份。按[专业验证设计](llm-factory-strategy-optimization.design.md#ver-lfo-rewrite-receipt)分别核对适用输入、结果和失败边界，当前未执行。

<a id="lfo-refine-preview"></a>
### 既有义务：精炼恢复价值可读

Trace refs: O7-REQ-46cda05b-3486-48e6-a30c-75123ff2822f；对应产品根 [SC-7 builtin 子范围](../../product/agents-world-simulation/prd.md#sc-agent-007-factory-refine-preview)。

硬件不足恢复建议让玩家读到精炼净收益、电力机会成本、硬件缺口与首目标关联。 原合同见[原始条款](#lfo-refine-source)；strategy hint 或未来升级 schema 路线；缺预览保留 refine_quote_missing，不宣称当前公共 observation 已有字段。

<a id="ac-lfo-refine-preview"></a>
#### 验收关系：精炼恢复价值可读

Trace refs: O7-REQ-46cda05b-3486-48e6-a30c-75123ff2822f；验收上述[同一义务](#lfo-refine-preview)，不另分配身份。按[专业验证设计](llm-factory-strategy-optimization.design.md#ver-lfo-refine-preview)分别核对适用输入、结果和失败边界，当前未执行。

<a id="lfo-power-preview"></a>
### 既有义务：电力恢复生存价值可读

Trace refs: O7-REQ-a66aa3a6-7e7b-49f7-9cef-51c94c229179；对应产品根 [SC-7 builtin 子范围](../../product/agents-world-simulation/prd.md#sc-agent-007-factory-power-preview)。

电力恢复建议让玩家读到恢复后 runway、下一步可负担性和防停机收益。 原合同见[原始条款](#lfo-power-source)；power_survival_quote_missing 仍是 future routed diagnostic；不新增公共 last_action/schema 字段。

<a id="ac-lfo-power-preview"></a>
#### 验收关系：电力恢复生存价值可读

Trace refs: O7-REQ-a66aa3a6-7e7b-49f7-9cef-51c94c229179；验收上述[同一义务](#lfo-power-preview)，不另分配身份。按[专业验证设计](llm-factory-strategy-optimization.design.md#ver-lfo-power-preview)分别核对适用输入、结果和失败边界，当前未执行。
