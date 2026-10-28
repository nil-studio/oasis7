# LLM 工厂闭环策略稳定性优化（llm_bootstrap）设计

- 对应需求文档: `doc/world-simulator/llm/llm-factory-strategy-optimization.prd.md`
- 专题入口与权威边界: `doc/world-simulator/llm/README.md`

## 1. 设计定位
定义 llm_bootstrap 工厂闭环策略稳定性优化设计，让工厂策略生成、评估与纠偏形成稳定闭环。

### 1.1 问题、目标与范围

既有 builtin 工业策略需要区分拒绝后的补救、报告动作结果、工具协议与类型化输入、改写原因及成本可读性。目标是让这些已采用义务可逐项追踪和验证；本次不实现策略算法，不把原初版建议、参数待定或历史计划升级成新产品承诺。

## 2. 设计结构
- 策略生成层：基于工厂状态生成可执行策略建议。
- 精炼恢复解释层：当硬件不足恢复为 `refine_compound` 时，解释电力成本、硬件产出和首个工业目标缺口变化。
- 电力恢复解释层：当电力不足恢复为 `harvest_radiation`、`BuyPower` 或等待发电时，解释恢复后 runway、状态变化、下一步动作可负担性和防停机收益。
- 闭环评估层：比较策略执行结果与目标偏差。
- 纠偏优化层：自动或半自动调整低效策略。
- 稳定性回归层：围绕关键工业场景验证策略波动与收敛。

### 2.1 需求承接与分配表

Owner：`agent_engineer`；共同审读：`gameplay_designer`、`runtime_engineer`、`qa_engineer`。本次为既有合同的兼容性文书整理；只补本表、§6 的精确条款与§11验证关系，不实施算法、协议或权限变更。产品关系经专业 PRD 精确追溯根 SC-2/SC-7 的既有 builtin 子范围；EXT、学习条款及 Provider 输入/参数两个身份不能代签这里的独立结果。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-fffbfcf7-4ab2-46b5-9771-bc14276f9521；[失败恢复与反循环](llm-factory-strategy-optimization.prd.md#lfo-recovery) | 失败原因驱动补救并避免无效重复，使 builtin 工业策略收敛到建厂和排产。 | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-recovery) | agent_engineer / gameplay_designer；原专业参数及执行 authority | 恢复目标为原强制要求；初版四项映射仍是建议，N 与模板下限不在此重定。 |
| Trace refs: O7-REQ-ff9e304a-1782-434c-a344-f5377affe3a3；[动作触发、成败与首达可分辨](llm-factory-strategy-optimization.prd.md#lfo-metrics) | 报告可独立判断建厂/排产是否触发、成败次数与首达 tick。 | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-metrics) | agent_engineer / qa_engineer；原专业参数及执行 authority | 四个报告 map 保留原语义，不新增触发率分母或首次成功 tick。 |
| Trace refs: O7-REQ-67e3afba-3cce-4e00-b84e-7be5960020f0；[最终决策工具协议](llm-factory-strategy-optimization.prd.md#lfo-tool-only) | builtin 最终决策经既定工具提交，缺 function_call 不成为有效最终决策。 | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-tool-only) | agent_engineer；原专业参数及执行 authority | LFO7 的 required、查询工具与 guardrail/修复边界保持，不扩大工具或动作集合。 |
| Trace refs: O7-REQ-265317d7-916b-439e-8668-30123bbfdaab；[类型化决策输入单路径](llm-factory-strategy-optimization.prd.md#lfo-typed-input) | builtin 行为层消费类型化 turn，SDK 解析失败不启用 raw-body JSON 后备入口。 | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-typed-input) | agent_engineer；原专业参数及执行 authority | LFO9 删除字符串往返不删除 tool 参数/决策语义校验；output 仍供 trace/debug。 |
| Trace refs: O7-REQ-bdf9e6ce-431b-4bbc-a1d0-4eaecab050e1；[已解析内嵌等待的既定处理](llm-factory-strategy-optimization.prd.md#lfo-nested-wait) | 旧 builtin lane 的 execute_until 内嵌 wait/wait_ticks 按已选最小动作改写继续推进。 | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-nested-wait) | agent_engineer / runtime_engineer；原专业参数及执行 authority | 仅限 LFO11 已解析的内嵌等待；不是 provider 失败 fallback，也不是 native durable Wait。 |
| Trace refs: O7-REQ-876ef676-3032-4d78-989a-9d5af5971f43；[改写原因可追踪](llm-factory-strategy-optimization.prd.md#lfo-rewrite-receipt) | builtin 的决策改写保留 from/to/reason，并在 trace 与下一轮语境解释原请求为何变化。 | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-rewrite-receipt) | agent_engineer；原专业参数及执行 authority | LFO11 的已有 decision_rewrite 与两个 future quote diagnostics 分开，回执不证明成本预览。 |
| Trace refs: O7-REQ-46cda05b-3486-48e6-a30c-75123ff2822f；[精炼恢复价值可读](llm-factory-strategy-optimization.prd.md#lfo-refine-preview) | 硬件不足恢复建议让玩家读到精炼净收益、电力机会成本、硬件缺口与首目标关联。 | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-refine-preview) | agent_engineer / gameplay_designer / qa_engineer；原专业参数及执行 authority | strategy hint 或未来升级 schema 路线；缺预览保留 refine_quote_missing，不宣称当前公共 observation 已有字段。 |
| Trace refs: O7-REQ-a66aa3a6-7e7b-49f7-9cef-51c94c229179；[电力恢复生存价值可读](llm-factory-strategy-optimization.prd.md#lfo-power-preview) | 电力恢复建议让玩家读到恢复后 runway、下一步可负担性和防停机收益。 | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-power-preview) | agent_engineer / gameplay_designer / qa_engineer；原专业参数及执行 authority | power_survival_quote_missing 仍是 future routed diagnostic；不新增公共 last_action/schema 字段。 |

## 3. 关键接口 / 入口
- 工厂状态输入
- 策略建议输出
- `refine_quote` 摘要输出：`electricity_cost`、`hardware_output`、`electricity_after`、`hardware_shortfall_before/after`、`first_goal_relevance`、`recommended_refine_amount`、`refine_value_class`
- `power_survival_quote` / `energy_recovery_preview` 摘要输出：`current_power_level`、`power_state_before/after`、`recovery_action`、`power_gain_estimate`、`price_or_time_cost`、`survival_runway_ticks`、`next_action_affordability`、`shutdown_avoidance_reason`、`recommended_power_action`
- 纠偏与评估接口
- 工业闭环回归

## 4. 约束与边界
- 优化不能破坏基础工业主路径。
- 策略调整需要可解释、可追踪。
- `refine_compound` 恢复建议不得只展示动作改写结果；缺少净收益/机会成本时标记 `refine_quote_missing`。
- 精炼预览不改变恢复规则强度、精炼公式、runtime ABI 或 viewer 实现。
- 电力恢复建议不得只展示“去补电/采集辐射”的动作改写结果；缺少恢复后 runway、下一步可执行性或防停机收益时标记 `power_survival_quote_missing`。
- 电力恢复预览不改变电力消耗、发电、价格、阈值、runtime ABI、viewer 实现或 `harvest_radiation` / `BuyPower` 动作语义。
- `power_survival_quote_missing` 是 future routed diagnostic；若未来进入 agent observation 或公共 `last_action`，必须同步升级 dual-mode provider contract / schema version，当前设计只约束 strategy hint 或玩家可读预览的可读性。
- 不在本专题引入黑盒自动调参系统。

## 5. 设计演进计划
- 先建立策略输入输出。
- 再接闭环评估与纠偏。
- 最后执行稳定性回归。

## 6. 既有合同的精确承接

适用性：本专题保留历史 builtin/llm_bootstrap lane；旧 20/30 tick 规划不成为当前队列。Runtime M4、训练/RL、外部策略服务、viewer 修改和黑盒自动调参继续排除。不改变动作、资源经济、ABI、provider 权限或当前 observation/action schema。

<a id="des-lfo-recovery"></a>
### 失败恢复与反循环

恢复规则模板、最近拒绝原因和固定离线回归分别承接；强度和参数保持原文。 恢复目标为原强制要求；初版四项映射仍是建议，N 与模板下限不在此重定。 对应[上游及验收](llm-factory-strategy-optimization.prd.md#ac-lfo-recovery)。

<a id="des-lfo-metrics"></a>
### 动作触发、成败与首达可分辨

累计、成功、失败计数与首次动作时间各自保留；无触发时的缺 key/null 兼容待核。 四个报告 map 保留原语义，不新增触发率分母或首次成功 tick。 对应[上游及验收](llm-factory-strategy-optimization.prd.md#ac-lfo-metrics)。

<a id="des-lfo-tool-only"></a>
### 最终决策工具协议

请求设置 tool_choice=required；agent_submit_decision 与查询共用工具调用面，协议违规沿既有修复路径。 LFO7 的 required、查询工具与 guardrail/修复边界保持，不扩大工具或动作集合。 对应[上游及验收](llm-factory-strategy-optimization.prd.md#ac-lfo-tool-only)。

<a id="des-lfo-typed-input"></a>
### 类型化决策输入单路径

SDK response 归一化为决策或模块 turn，decision_flow 负责语义解析，behavior_loop 直接消费 typed turn。 LFO9 删除字符串往返不删除 tool 参数/决策语义校验；output 仍供 trace/debug。 对应[上游及验收](llm-factory-strategy-optimization.prd.md#ac-lfo-typed-input)。

<a id="des-lfo-nested-wait"></a>
### 已解析内嵌等待的既定处理

内嵌 wait/wait_ticks 默认改写 harvest_radiation(max_amount=1)，Act 保持原逻辑；本条不扩大到新 lane。 仅限 LFO11 已解析的内嵌等待；不是 provider 失败 fallback，也不是 native durable Wait。 对应[上游及验收](llm-factory-strategy-optimization.prd.md#ac-lfo-nested-wait)。

<a id="des-lfo-rewrite-receipt"></a>
### 改写原因可追踪

解析/guardrail 改写回执经 llm_step_trace/system message 与 last_action.decision_rewrite 回灌；仅类型变化时降低噪声。 LFO11 的已有 decision_rewrite 与两个 future quote diagnostics 分开，回执不证明成本预览。 对应[上游及验收](llm-factory-strategy-optimization.prd.md#ac-lfo-rewrite-receipt)。

<a id="des-lfo-refine-preview"></a>
### 精炼恢复价值可读

完整字段以专业 PRD 为准，包含 compound_mass_g；不只展示动作改写结果，不改变精炼公式或恢复规则强度。 strategy hint 或未来升级 schema 路线；缺预览保留 refine_quote_missing，不宣称当前公共 observation 已有字段。 对应[上游及验收](llm-factory-strategy-optimization.prd.md#ac-lfo-refine-preview)。

<a id="des-lfo-power-preview"></a>
### 电力恢复生存价值可读

harvest_radiation、BuyPower 或等待发电的预览按原字段解释状态变化、成本和可行下一步，不改变电力/发电/价格/阈值。 power_survival_quote_missing 仍是 future routed diagnostic；不新增公共 last_action/schema 字段。 对应[上游及验收](llm-factory-strategy-optimization.prd.md#ac-lfo-power-preview)。

### 实现观察与未覆盖边界

静态定位以 `cde57786e711b03a9c9fa2a5174aba357e8350d5` 为源基线，不是执行或验收证据：

- `prompt_assembly.rs` 存在恢复文本和当前重复限制；初版建议与当前 Data/mine/transfer 文案并不逐字等同。本次不替 owner 重定映射、N 或模板下限。
- `oasis7_llm_agent_demo_cli.rs::observe_action_result` 在判断 success 前登记 `first_action_tick`，包括失败的首次结果；`BTreeMap<String,u64>` 对未发生动作的缺 key 与 PRD 可空值表述仍需兼容验证。计数不自动等于有分母的百分比。
- `tests_followups_actions.rs::llm_agent_mock_sequence_recovers_and_completes_factory_recipe_chain` 实际为成功建厂、人工补 Data/电力、精炼、排产，不能证明原 [TODO-3](llm-factory-strategy-optimization.prd.md#lfo-mock-source) 的硬件不足失败→精炼→建厂成功→排产四步及其 data 增长义务。原验收保留 planned，不用测试名代签。
- `openai_payload.rs` 有 Required 与 SDK typed-turn 转换，`behavior_loop.rs` 消费 `completion.turns`；错误枚举仍有 ParseBody 不能单凭名称判定 raw-body fallback 存在或不存在。SDK 失败路径需独立核验。
- `decision_flow.rs` 及 `tests_followups_regressions.rs` 有内嵌等待改写；`behavior_prompt.rs` 有 decision_rewrite 回灌。它们不证明精炼/电力预览，也不允许 provider 失败时改成有副作用动作。native durable Wait 的 admission、scheduled readback、wake/resume 属于另一 authority。
- 本专题消费[既有 builtin 输入合同](decision-provider-contract.prd.md#builtin-input-integrity)与[参数说明合同](decision-provider-contract.prd.md#builtin-parameter-consistency)，不修改其身份或预算。`6500` 是该治理参数测试的一个合法正例，不证明本专题恢复、所有参数一致性、输入预算或真实模型行为。

## 11. 验证设计与可追溯性

验证状态均为 planned；本次只运行文档规范检查，不执行下述业务测试、模型或玩家验收。后续实现 PR 固定 candidate/source、lane、环境与真实输出，按当时[测试手册](../../../testing-manual.md)选择适用层级；原 `test_tier_required` 义务保留，不以文档检查代签 required/full 或在线抽样。以下各验证组是计划方法定位，不是已有同名测试。

<a id="ver-lfo-recovery"></a>
### 计划验证：失败恢复与反循环

`planned`，`test_tier_required`；`prompt_assembly.rs` 的恢复文本及 `tests_followups_actions.rs` 的 force_replan 三个现有 fixture；新增或补齐原 TODO-3 四步失败起点、FactoryBuilt/RecipeScheduled、data 增长断言，另核同签名 harvest 超阈值后切换。 由 agent_engineer / gameplay_designer 在同一固定候选、明确 builtin lane 与离线/人工环境记录真实源位置和输出。不冻结 N；现有人工 seed 成功序列不代签原失败恢复，也不证明在线模型收敛。

<a id="ver-lfo-metrics"></a>
### 计划验证：动作触发、成败与首达可分辨

`planned`，`test_tier_required`；在 demo report 层用固定成功、失败、从未触发的 build_factory/schedule_recipe 输入，独立核对四个 JSON map、首次结果时间、缺 key/null 及旧消费方兼容；入口 `oasis7_llm_agent_demo_cli.rs::observe_action_result` 与 runtime JSON 写出。 由 agent_engineer / qa_engineer 在同一固定候选、明确 builtin lane 与离线/人工环境记录真实源位置和输出。首次结果含失败，不是首次成功；无新触发率公式，不由 report 推断玩家体验。

<a id="ver-lfo-tool-only"></a>
### 计划验证：最终决策工具协议

`planned`，`test_tier_required`；在 builtin 请求/离线 fake transport 层核真实序列化 required/tools 与最终 function_call；沿 `tests_tool_execution.rs` 核合法工具及无调用、空响应和协议违规修复，保留 guardrail。 由 agent_engineer 在同一固定候选、明确 builtin lane 与离线/人工环境记录真实源位置和输出。请求设置不证明远端 provider 兼容或全部工具语义。

<a id="ver-lfo-typed-input"></a>
### 计划验证：类型化决策输入单路径

`planned`，`test_tier_required`；在 SDK/behavior 接缝构造 typed decision/module turn、SDK decode 失败、非法参数；核 `openai_payload.rs` 到 `behavior_loop.rs` 单路径、无 raw-body 后备与无字符串往返，参数/语义错误分类及trace展示分别断言。 由 agent_engineer 在同一固定候选、明确 builtin lane 与离线/人工环境记录真实源位置和输出。ParseBody 名称或trace字符串不证明 fallback；不以typed代替参数/权限校验。

<a id="ver-lfo-nested-wait"></a>
### 计划验证：已解析内嵌等待的既定处理

`planned`，`test_tier_required`；沿 `tests_followups_regressions.rs::llm_agent_rewrites_execute_until_wait_action_to_actionable_harvest` 与 parser fixture，覆盖内嵌wait、wait_ticks、Act原逻辑；核最小动作和改写原因，单列普通Wait与provider错误不触发该路径。 由 agent_engineer / runtime_engineer 在同一固定候选、明确 builtin lane 与离线/人工环境记录真实源位置和输出。不证明native durable Wait或异步World推进；不扩大副作用fallback。

<a id="ver-lfo-rewrite-receipt"></a>
### 计划验证：改写原因可追踪

`planned`，`test_tier_required`；在 `behavior_prompt.rs`、`behavior_loop.rs` 的builtin离线回路核from/to/reason、trace/system message、下一轮last_action；分别覆盖内嵌wait与schedule_recipe→refine改写，仅类型变化回灌。 由 agent_engineer 在同一固定候选、明确 builtin lane 与离线/人工环境记录真实源位置和输出。trace存在不证明玩家成本可读性，旧fixture不代签schema扩展。

<a id="ver-lfo-refine-preview"></a>
### 计划验证：精炼恢复价值可读

`planned`，`test_tier_required`；按原字段逐一构造硬件不足→精炼恢复建议，验证净收益、电力机会成本、缺口before/after与首目标；缺失摘要标refine_quote_missing。选择strategy hint/玩家预览层进行人工可读性核对；尚无完整验证source。 由 agent_engineer / gameplay_designer / qa_engineer 在同一固定候选、明确 builtin lane 与离线/人工环境记录真实源位置和输出。planned；不得把未来字段写成oc_dual_obs_v1已携带，进入observation需另行schema合同升级。

<a id="ver-lfo-power-preview"></a>
### 计划验证：电力恢复生存价值可读

`planned`，`test_tier_required`；按原字段逐一覆盖harvest_radiation、BuyPower、等待发电，核runway、状态、价格或时间成本、下一步可负担性与防停机理由；缺失标power_survival_quote_missing。strategy hint/玩家预览层人工核对；尚无完整验证source。 由 agent_engineer / gameplay_designer / qa_engineer 在同一固定候选、明确 builtin lane 与离线/人工环境记录真实源位置和输出。planned；不改变价格/电力规则，不证明公共last_action/schema已实现。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-fffbfcf7-4ab2-46b5-9771-bc14276f9521；[失败恢复与反循环及验收](llm-factory-strategy-optimization.prd.md#lfo-recovery) | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-recovery) | 失败原因驱动补救并避免无效重复，使 builtin 工业策略收敛到建厂和排产。 | planned；`test_tier_required`；按本节“计划验证：失败恢复与反循环”的具体断言与缺口，[现有离线测试源](../../../crates/oasis7/src/simulator/llm_agent/tests_followups_actions.rs)，现有入口不证明全部义务，缺口按对应计划验证组补齐；固定candidate、builtin lane与离线/人工环境，记录真实方法与输出，不执行业务验证 | [计划方法定位](llm-factory-strategy-optimization.design.md#ver-lfo-recovery)；实现 PR 的固定候选、实际 fixture/manual 输出及失败样本；当前未执行 | 不冻结 N；现有人工 seed 成功序列不代签原失败恢复，也不证明在线模型收敛。 |
| Trace refs: O7-REQ-ff9e304a-1782-434c-a344-f5377affe3a3；[动作触发、成败与首达可分辨及验收](llm-factory-strategy-optimization.prd.md#lfo-metrics) | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-metrics) | 报告可独立判断建厂/排产是否触发、成败次数与首达 tick。 | planned；`test_tier_required`；按本节“计划验证：动作触发、成败与首达可分辨”的具体断言与缺口，[测试方法 authority](../../../testing-manual.md)，现有入口不证明全部义务，缺口按对应计划验证组补齐；固定candidate、builtin lane与离线/人工环境，记录真实方法与输出，不执行业务验证 | [计划方法定位](llm-factory-strategy-optimization.design.md#ver-lfo-metrics)；实现 PR 的固定候选、实际 fixture/manual 输出及失败样本；当前未执行 | 首次结果含失败，不是首次成功；无新触发率公式，不由 report 推断玩家体验。 |
| Trace refs: O7-REQ-67e3afba-3cce-4e00-b84e-7be5960020f0；[最终决策工具协议及验收](llm-factory-strategy-optimization.prd.md#lfo-tool-only) | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-tool-only) | builtin 最终决策经既定工具提交，缺 function_call 不成为有效最终决策。 | planned；`test_tier_required`；按本节“计划验证：最终决策工具协议”的具体断言与缺口，[现有离线测试源](../../../crates/oasis7/src/simulator/llm_agent/tests_tool_execution.rs)，现有入口不证明全部义务，缺口按对应计划验证组补齐；固定candidate、builtin lane与离线/人工环境，记录真实方法与输出，不执行业务验证 | [计划方法定位](llm-factory-strategy-optimization.design.md#ver-lfo-tool-only)；实现 PR 的固定候选、实际 fixture/manual 输出及失败样本；当前未执行 | 请求设置不证明远端 provider 兼容或全部工具语义。 |
| Trace refs: O7-REQ-265317d7-916b-439e-8668-30123bbfdaab；[类型化决策输入单路径及验收](llm-factory-strategy-optimization.prd.md#lfo-typed-input) | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-typed-input) | builtin 行为层消费类型化 turn，SDK 解析失败不启用 raw-body JSON 后备入口。 | planned；`test_tier_required`；按本节“计划验证：类型化决策输入单路径”的具体断言与缺口，[现有离线测试源](../../../crates/oasis7/src/simulator/llm_agent/tests_tool_execution.rs)，现有入口不证明全部义务，缺口按对应计划验证组补齐；固定candidate、builtin lane与离线/人工环境，记录真实方法与输出，不执行业务验证 | [计划方法定位](llm-factory-strategy-optimization.design.md#ver-lfo-typed-input)；实现 PR 的固定候选、实际 fixture/manual 输出及失败样本；当前未执行 | ParseBody 名称或trace字符串不证明 fallback；不以typed代替参数/权限校验。 |
| Trace refs: O7-REQ-bdf9e6ce-431b-4bbc-a1d0-4eaecab050e1；[已解析内嵌等待的既定处理及验收](llm-factory-strategy-optimization.prd.md#lfo-nested-wait) | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-nested-wait) | 旧 builtin lane 的 execute_until 内嵌 wait/wait_ticks 按已选最小动作改写继续推进。 | planned；`test_tier_required`；按本节“计划验证：已解析内嵌等待的既定处理”的具体断言与缺口，[现有离线测试源](../../../crates/oasis7/src/simulator/llm_agent/tests_followups_regressions.rs)，现有入口不证明全部义务，缺口按对应计划验证组补齐；固定candidate、builtin lane与离线/人工环境，记录真实方法与输出，不执行业务验证 | [计划方法定位](llm-factory-strategy-optimization.design.md#ver-lfo-nested-wait)；实现 PR 的固定候选、实际 fixture/manual 输出及失败样本；当前未执行 | 不证明native durable Wait或异步World推进；不扩大副作用fallback。 |
| Trace refs: O7-REQ-876ef676-3032-4d78-989a-9d5af5971f43；[改写原因可追踪及验收](llm-factory-strategy-optimization.prd.md#lfo-rewrite-receipt) | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-rewrite-receipt) | builtin 的决策改写保留 from/to/reason，并在 trace 与下一轮语境解释原请求为何变化。 | planned；`test_tier_required`；按本节“计划验证：改写原因可追踪”的具体断言与缺口，[现有离线测试源](../../../crates/oasis7/src/simulator/llm_agent/tests_followups_regressions.rs)，现有入口不证明全部义务，缺口按对应计划验证组补齐；固定candidate、builtin lane与离线/人工环境，记录真实方法与输出，不执行业务验证 | [计划方法定位](llm-factory-strategy-optimization.design.md#ver-lfo-rewrite-receipt)；实现 PR 的固定候选、实际 fixture/manual 输出及失败样本；当前未执行 | trace存在不证明玩家成本可读性，旧fixture不代签schema扩展。 |
| Trace refs: O7-REQ-46cda05b-3486-48e6-a30c-75123ff2822f；[精炼恢复价值可读及验收](llm-factory-strategy-optimization.prd.md#lfo-refine-preview) | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-refine-preview) | 硬件不足恢复建议让玩家读到精炼净收益、电力机会成本、硬件缺口与首目标关联。 | planned；`test_tier_required`；按本节“计划验证：精炼恢复价值可读”的具体断言与缺口，[现有离线测试源](../../../crates/oasis7/src/simulator/llm_agent/tests_followups_actions.rs)，当前 fixture 不含该预览的完整断言与玩家可读性验收，计划补充strategy hint字段正负例及人工预览检查；固定candidate、builtin lane与离线/人工环境，记录真实方法与输出，不执行业务验证 | [计划方法定位](llm-factory-strategy-optimization.design.md#ver-lfo-refine-preview)；实现 PR 的固定候选、实际 fixture/manual 输出及失败样本；当前未执行 | planned；不得把未来字段写成oc_dual_obs_v1已携带，进入observation需另行schema合同升级。 |
| Trace refs: O7-REQ-a66aa3a6-7e7b-49f7-9cef-51c94c229179；[电力恢复生存价值可读及验收](llm-factory-strategy-optimization.prd.md#lfo-power-preview) | [精确承接](llm-factory-strategy-optimization.design.md#des-lfo-power-preview) | 电力恢复建议让玩家读到恢复后 runway、下一步可负担性和防停机收益。 | planned；`test_tier_required`；按本节“计划验证：电力恢复生存价值可读”的具体断言与缺口，[现有离线测试源](../../../crates/oasis7/src/simulator/llm_agent/tests_followups_actions.rs)，当前 fixture 不含该预览的完整断言与玩家可读性验收，计划补充strategy hint字段正负例及人工预览检查；固定candidate、builtin lane与离线/人工环境，记录真实方法与输出，不执行业务验证 | [计划方法定位](llm-factory-strategy-optimization.design.md#ver-lfo-power-preview)；实现 PR 的固定候选、实际 fixture/manual 输出及失败样本；当前未执行 | planned；不改变价格/电力规则，不证明公共last_action/schema已实现。 |


## 12. 决策边界与剩余风险

已采用的恢复目标、报告结果、LFO7/9/11 及两种预览义务保持；本次不重新冻结原“初版（建议）”映射、N/模板参数。未来若选择公共 observation 路由，由 `agent_engineer` 会同 provider/schema owner 先更新 dual-mode 合同及版本，再验证消费者，不能把本表当作路由授权。恢复映射或参数改变由 `agent_engineer/gameplay_designer` 按原专业 authority 决定；移植 native lane 或扩大 provider 默认支持须由对应 Runtime/provider owner 独立判断。

剩余风险是固定 mock 与原失败恢复场景不一致、报告缺 key/null 兼容、SDK错误路径以及玩家实际可读性未验证。文书身份缺失可在本范围补齐，业务验证缺口仍为 planned；离线 fixture、单次在线抽样、旧测试名和文档规范通过均不能替代完整验收。
