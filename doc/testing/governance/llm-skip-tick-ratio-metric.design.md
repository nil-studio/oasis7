# oasis7: LLM 跳过 Tick 占比指标设计

- 对应需求文档: [LLM 跳过 Tick 占比指标](llm-skip-tick-ratio-metric.prd.md)
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

<a id="1-设计定位"></a>

## 1. 设计定位与目标

本指标用于观察 LLM 调用节省，不新增 fail gate、阈值或阻断策略，不改变 LLM 决策与既有 release gate。数据由 demo trace 采集并持久化到 report，再进入 stress 的单场景与聚合输出。

<a id="llmskip-count-output-design"></a>
## 2. 计数与计算

<a id="des-llmskip-count"></a>

- 分母为 `active_ticks`，不是 wall-clock 或总请求数。
- 每个 active tick 命中 `llm_input == None` 或 `skip_llm_with_active_execute_until` 即累计一次 `llm_skipped_ticks`；同 tick 同时命中仍只计一次。
<a id="des-llmskip-ratio"></a>

- `llm_skipped_tick_ratio_ppm = floor(llm_skipped_ticks * 1_000_000 / active_ticks)`；`active_ticks=0` 返回 0。采用足够宽/饱和的整数计算避免中间乘法溢出。
<a id="des-llmskip-aggregate"></a>

- 聚合先求和 skipped 与 active 计数，再按同式计算，不平均各场景的 ppm；保留单场景原值用于解释权重。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [llmskip-classification](llm-skip-tick-ratio-metric.prd.md#llmskip-classification) | active 分母、两种 skipped 与同 tick 一次；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=trace 分类 | [des-llmskip-count](#des-llmskip-count) | demo/runtime、stress 脚本维护者与 QA | 现有 tests.rs 仅复合条件 1/4 fixture，独立分类待验证 |
| [llmskip-ratio-contract](llm-skip-tick-ratio-metric.prd.md#llmskip-ratio-contract) | floor ppm、零分母、足够宽/饱和整数；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=数值公式 | [des-llmskip-ratio](#des-llmskip-ratio) | demo/runtime、stress 脚本维护者与 QA | 零分母/整数实现已定位，边界执行未证实 |
| [llmskip-persistence](llm-skip-tick-ratio-metric.prd.md#llmskip-persistence) | TraceCounts 字段、终态日志与 report.trace_counts 持久化；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=demo/report 输出 | [des-llmskip-report](#des-llmskip-report) | demo/runtime、stress 脚本维护者与 QA | 源码定义存在，未取得真实同 run artifact |
| [llmskip-stress-output](llm-skip-tick-ratio-metric.prd.md#llmskip-stress-output) | 单场景 summary/TSV 与 aggregate JSON/summary 字段；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=stress 文件消费 | [des-llmskip-stress](#des-llmskip-stress) | demo/runtime、stress 脚本维护者与 QA | 字段位置已定位，未运行 stress 或证明序列化一致 |
| [llmskip-stress-output](llm-skip-tick-ratio-metric.prd.md#llmskip-stress-output) | 先 sum skipped/active 再算 ppm，不平均各场景；保留场景权重；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=加权聚合 | [des-llmskip-aggregate](#des-llmskip-aggregate) | demo/runtime、stress 脚本维护者与 QA | 求和实现已定位，整数溢出/浮点精度和两路等价未证实 |
| [llmskip-fallback](llm-skip-tick-ratio-metric.prd.md#llmskip-fallback) | 无 jq 时 Python/log 同口径；缺来源不冒充实际零；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=解析 fallback 与来源诊断 | [des-llmskip-fallback](#des-llmskip-fallback) | demo/runtime、stress 脚本维护者与 QA | 默认 0 已定位，缺来源与真实零的完整诊断未证明 |
| [llmskip-noninterference](llm-skip-tick-ratio-metric.prd.md#llmskip-noninterference) | 不改决策、gate、阈值、Viewer UI，不增敏感输出；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=观测兼容与安全 | [des-llmskip-noninterference](#des-llmskip-noninterference) | demo/runtime、stress 脚本维护者与 QA | 本次无行为改动；不替全部运行链验收 |
| [llmskip-regression](llm-skip-tick-ratio-metric.prd.md#llmskip-regression) | 离线 fixtures 和演进回归，不以缺环境记 pass；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=验证设计 | [des-llmskip-fixtures](#des-llmskip-fixtures) | demo/runtime、stress 脚本维护者与 QA | 现有单测未执行，不能代签全部计划 fixture 或真实模型 |


## 3. 输出链路与边界

<a id="des-llmskip-report"></a>

- demo 的 `TraceCounts` 提供计数与 ppm，终态日志输出同名字段；`report.json` 使用 `trace_counts.llm_skipped_ticks` 和 `trace_counts.llm_skipped_tick_ratio_ppm`，分母为 `active_ticks`。
<a id="des-llmskip-stress"></a>

- `llm-longrun-stress.sh` 从 report/log 提取相同指标，写 summary、TSV、聚合 JSON/summary；无 jq 时的 Python/log fallback 保留字段和计算口径。
<a id="des-llmskip-fallback"></a>

- trace 缺失按 PRD 的无 LLM 请求语义判断，不能由日志文件不存在臆造有效观测；缺失的来源与实际零值须在诊断中区分。
<a id="des-llmskip-noninterference"></a>

- 采集失败与 fallback 的说明服务于结果解释，不引入本指标独有的阈值门禁；现有 fail gate 按原合同执行。

<a id="des-llmskip-fixtures"></a>

## 4. 验证设计

用离线 trace/report fixture 覆盖两类 skipped、同 tick 双命中、普通请求、零 active、非整除 ppm 与不同 active 数量的聚合；分别核对 jq、Python 和 log fallback 的相同输出。跨环境缺失路径不能写成已执行通过。

<a id="des-llmskip-evolution"></a>

## 5. 演进边界

新增跳过路径时先补计数语义与回归，再同步所有输出消费者；本专题不定义调用调度算法、Viewer UI 或新的发布门禁。


## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [llmskip-classification](llm-skip-tick-ratio-metric.prd.md#llmskip-classification) | [des-llmskip-count](#des-llmskip-count) | active 分母、两种 skipped 与同 tick 一次；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=trace 分类 | 独立 None-only、marker-only、双命中、普通 request、无 trace fixtures；核 runtime active 与 optional trace；固定候选源码核读与未来运行结果分记  [testing-manual](../../../testing-manual.md)；现有 tests.rs 复合 fixture 和 stress 源定位见 §12；| 实施 PR 的候选、命令、独立 fixture、真实输出与未执行范围；本轮只有静态定位 | 现有 tests.rs 仅复合条件 1/4 fixture，独立分类待验证；不代签浏览器、节点、provider 或发布验收 |
| [llmskip-ratio-contract](llm-skip-tick-ratio-metric.prd.md#llmskip-ratio-contract) | [des-llmskip-ratio](#des-llmskip-ratio) | floor ppm、零分母、足够宽/饱和整数；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=数值公式 | 零 active、非整除、近上界 fixtures；核 CLI finalize 和 DEC-LLMSKIP-001/002；固定候选源码核读与未来运行结果分记  [testing-manual](../../../testing-manual.md)；现有 tests.rs 复合 fixture 和 stress 源定位见 §12；| 实施 PR 的候选、命令、独立 fixture、真实输出与未执行范围；本轮只有静态定位 | 零分母/整数实现已定位，边界执行未证实；不代签浏览器、节点、provider 或发布验收 |
| [llmskip-persistence](llm-skip-tick-ratio-metric.prd.md#llmskip-persistence) | [des-llmskip-report](#des-llmskip-report) | TraceCounts 字段、终态日志与 report.trace_counts 持久化；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=demo/report 输出 | 核 AC-1/2、runtime write_report_json；未来比较同 run stdout/report 字段和 active 分母；固定候选源码核读与未来运行结果分记  [testing-manual](../../../testing-manual.md)；现有 tests.rs 复合 fixture 和 stress 源定位见 §12；| 实施 PR 的候选、命令、独立 fixture、真实输出与未执行范围；本轮只有静态定位 | 源码定义存在，未取得真实同 run artifact；不代签浏览器、节点、provider 或发布验收 |
| [llmskip-stress-output](llm-skip-tick-ratio-metric.prd.md#llmskip-stress-output) | [des-llmskip-stress](#des-llmskip-stress) | 单场景 summary/TSV 与 aggregate JSON/summary 字段；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=stress 文件消费 | 逐路径核 stress 提取和输出；未来同 fixture 比较实际四类文件键路径；固定候选源码核读与未来运行结果分记  [testing-manual](../../../testing-manual.md)；现有 tests.rs 复合 fixture 和 stress 源定位见 §12；| 实施 PR 的候选、命令、独立 fixture、真实输出与未执行范围；本轮只有静态定位 | 字段位置已定位，未运行 stress 或证明序列化一致；不代签浏览器、节点、provider 或发布验收 |
| [llmskip-stress-output](llm-skip-tick-ratio-metric.prd.md#llmskip-stress-output) | [des-llmskip-aggregate](#des-llmskip-aggregate) | 先 sum skipped/active 再算 ppm，不平均各场景；保留场景权重；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=加权聚合 | 不同 active 权重、非整除和近上界 fixtures；核 shell 与 Python aggregate JSON；固定候选源码核读与未来运行结果分记  [testing-manual](../../../testing-manual.md)；现有 tests.rs 复合 fixture 和 stress 源定位见 §12；| 实施 PR 的候选、命令、独立 fixture、真实输出与未执行范围；本轮只有静态定位 | 求和实现已定位，整数溢出/浮点精度和两路等价未证实；不代签浏览器、节点、provider 或发布验收 |
| [llmskip-fallback](llm-skip-tick-ratio-metric.prd.md#llmskip-fallback) | [des-llmskip-fallback](#des-llmskip-fallback) | 无 jq 时 Python/log 同口径；缺来源不冒充实际零；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=解析 fallback 与来源诊断 | 无 jq/无 Python、缺失/损坏 report、日志缺失、真实零 fixtures；比较三路字段和来源标记；固定候选源码核读与未来运行结果分记  [testing-manual](../../../testing-manual.md)；现有 tests.rs 复合 fixture 和 stress 源定位见 §12；| 实施 PR 的候选、命令、独立 fixture、真实输出与未执行范围；本轮只有静态定位 | 默认 0 已定位，缺来源与真实零的完整诊断未证明；不代签浏览器、节点、provider 或发布验收 |
| [llmskip-noninterference](llm-skip-tick-ratio-metric.prd.md#llmskip-noninterference) | [des-llmskip-noninterference](#des-llmskip-noninterference) | 不改决策、gate、阈值、Viewer UI，不增敏感输出；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=观测兼容与安全 | 核 PRD Non-Goals / NFR-3 / Security 及只含 MD 的变更边界；原 gate 只读；固定候选源码核读与未来运行结果分记  [testing-manual](../../../testing-manual.md)；现有 tests.rs 复合 fixture 和 stress 源定位见 §12；| 实施 PR 的候选、命令、独立 fixture、真实输出与未执行范围；本轮只有静态定位 | 本次无行为改动；不替全部运行链验收；不代签浏览器、节点、provider 或发布验收 |
| [llmskip-regression](llm-skip-tick-ratio-metric.prd.md#llmskip-regression) | [des-llmskip-fixtures](#des-llmskip-fixtures) | 离线 fixtures 和演进回归，不以缺环境记 pass；产品适用性：N/A；reason=仅 QA/debug 工程观测，不改变玩家行为或调度；scope=验证设计 | S8 / test_tier_required 选择器见本设计 §13；将各 oracle 和实际命令/环境分别记录；固定候选源码核读与未来运行结果分记  [testing-manual](../../../testing-manual.md)；现有 tests.rs 复合 fixture 和 stress 源定位见 §12；| 实施 PR 的候选、命令、独立 fixture、真实输出与未执行范围；本轮只有静态定位 | 现有单测未执行，不能代签全部计划 fixture 或真实模型；不代签浏览器、节点、provider 或发布验收 |


<a id="des-llmskip-source-gaps"></a>

## 12. 已定位源码、未决差距与风险

固定核读基线：`a0b6ab12ad14c6c881e48cecdb4adbdb322dd30e`。以下是源码与 fixture 定义的 located 事实，不是 executed/pass；本文所有原判定、整数计算、fallback 和不新增 gate 约束继续有效。规范与实现差距留给 demo/runtime 与脚本 owner 核对，本次不改变规范迁就实现，不增加第三类规范判定、调度策略、阈值或发布条件。

- [CLI / TraceCounts](../../../crates/oasis7_provider_tools/src/bin/oasis7_llm_agent_demo/oasis7_llm_agent_demo_cli.rs)：`TraceCounts` 定义 skipped 与 ppm；`observe_trace` 每次调用最多加一次。`trace_skipped_llm_tick` 的 OR 除 `llm_input == None` 和 `input_summary=skip_llm_with_active_execute_until`，还接受 `step_type=execute_until_continue`。额外兼容判定与本文两种规范判定的语义关系待 owner 确认，不能推断三者各自等价。
- [runtime](../../../crates/oasis7_provider_tools/src/bin/oasis7_llm_agent_demo/oasis7_llm_agent_demo_runtime.rs)：`tick_decide_only` 返回 Some 才增加 `active_ticks`；仅 `decision_trace.as_ref()` 为 Some 才调用 `observe_trace`。无 trace 对象时只增加 active、不增加 skipped；这不同于有 trace 但 `llm_input=None`。缺 trace 规则仍需分类证据，不能以缺文件产生的 0 冒充实际观测。
- `finalize` 使用 u64 `saturating_mul(1_000_000).checked_div(active_ticks).unwrap_or(0)`；已定位零分母处理和饱和乘法，但未验证近上界输入仍符合理想数学比例。整数 floor/中间溢出合同不变。
- runtime `write_report_json` 以 serde_json 写完整 report，终态 stdout 包含同名字段。未取得此次真实运行的 report/log artifact，序列化定义不等于输出链实跑通过。
- [stress 脚本](../../../scripts/llm-longrun-stress.sh) 已定位 jq、Python、log 三路解析，场景 summary、TSV 和 aggregate summary/JSON。report/scene 使用 `llm_skipped_ticks`，aggregate text 使用 `llm_skipped_ticks_total`，aggregate JSON 使用 `totals.llm_skipped_ticks`；不能机械统一键路径。
- shell 先求 `agg_active_ticks`、`agg_llm_skipped_ticks` 的和再计算比率，不平均场景 ppm。shell 使用整数乘除；Python aggregate JSON 的 `int((sum_skipped * 1_000_000) / max(sum_active, 1))` 中间 `/` 为浮点。不同权重、非整除、极大输入和溢出/精度一致性未验证，不能声称全范围三路数学等价。
- jq 的 `// 0`、Python 默认 0、log 的 `echo 0` fallback 已定位；尚未取得专用断言证明缺来源、损坏来源与真实零值在完整诊断中区分。规范要求仍保留，不能从字段存在推断有效采集。
- [现有测试](../../../crates/oasis7_provider_tools/src/bin/oasis7_llm_agent_demo/tests.rs) 的 `observe_trace_counts_llm_skipped_tick_ratio` 设置 active=4，观察一条三条件同时命中的 skip 和一条普通 request，断言 skipped=1、ppm=250000。它仅定义此复合 fixture 和同时命中不重复的断言，不能分别证明 None-only、marker-only、step-type-only，也不能证明零 active、非整除、无 trace、近上界、持久化、加权聚合或 fallback。
- 已核 stress gate 仍针对 llm_errors、parse_errors、repair_rounds 与 gameplay coverage；未在该脚本定位到 skipped 比率专属 fail gate。此只读定位不替全部调用链背书，不修改已有 gate。

## 13. 验证执行边界与兼容决策

真实 fixture 选择器为 `env -u RUSTC_WRAPPER cargo test -p oasis7_provider_tools --bin oasis7_llm_agent_demo --features test_tier_required tests::observe_trace_counts_llm_skipped_tick_ratio -- --exact`。该命令本次未执行，即使将来通过也仅证明上述复合 fixture。`bash -n scripts/llm-longrun-stress.sh` 仅为语法检查，不证明 fallback 或数值边界。S8 的 demo/report/stress 入口保留于 [testing-manual](../../../testing-manual.md)，不因纯文档变更启动 provider、长跑或改变 tier。

待验证的独立 oracle：None-only、marker-only、双命中与普通请求；零分母、非整除及近上界；无 trace 的 active/skip 分离；report 序列化与终态日志一致；不同 active 权重先 sum 后 ratio；jq/Python/log 在无 jq、无 Python、report 缺失或损坏、实际全零时的来源诊断。源码额外 step-type-only 应由 owner 先确认语义，不能用现有复合测试替它作规范裁决。

产品适用性：N/A；reason=本设计只固定 QA/debug 观测计数与离线文件消费，不改变玩家可见行为、Agent/provider 身份或 LLM 调度；scope=各承接行列明的局部工程机制。N/A 不取消技术验证义务。新增跳过路径仍先补判定与回归、同步所有消费者；数值与来源差距保持待核，不借 Agent 产品 ID 宣称实现覆盖。
