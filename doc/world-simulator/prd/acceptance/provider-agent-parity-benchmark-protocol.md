# Local Provider vs 内置 Agent parity benchmark 协议

审计轮次: 1

## 目标
- 为 `PRD-WORLD_SIMULATOR-038` 冻结 builtin 与 `Local Provider(Local HTTP)` 的统一 fixture benchmark 协议，避免不同 provider 使用不同输入条件导致结果不可比。
- 定义 benchmark 运行时所需的固定字段、trace 收集要求、输出产物与失败签名分类。
- 为后续自动脚本实现提供稳定契约，使 `test_tier_required` 可以在 mock provider 与 builtin provider 上离线复用。

## 范围
- In Scope:
  - benchmark 输入包结构。
  - 运行约束（seed、timeout、采样次数、provider 标识）。
  - trace 汇总字段与 JSON/CSV 产物结构。
  - 错误码分类与 benchmark 结果状态。
- Out of Scope:
  - 不直接定义最终脚本实现语言或 CLI 细节。
  - 不包含真实 `Local Provider` 服务端实现。

## Benchmark 输入协议

### 1. 顶层运行字段
- `benchmark_run_id`: 本次对标批次 ID。
- `parity_tier`: `P0` / `P1` / `P2`。
- `scenario_id`: 取自 parity 场景矩阵。
- `fixture_id`: 固定 observation/seed 样本 ID。
- `provider_kind`: `builtin` / `provider_loopback_http` / `mock_provider`。
- `provider_version`: provider 自报版本。
- `adapter_version`: adapter 自报版本。
- `protocol_version`: benchmark 协议版本。
- `seed`: 固定随机种子。
- `timeout_ms`: 单轮 benchmark 的最大允许时长。
- `max_steps`: 单轮最大决策步数。

### 2. 场景输入字段
- `initial_world_snapshot_ref`: 输入世界快照引用。
- `observation_sequence_ref`: 固定 observation 序列引用。
- `goal_definition`: 当前样本的目标达成条件。
- `action_catalog_ref`: 动作白名单引用。
- `player_context_ref`: player/agent 绑定上下文引用。
- `memory_fixture_ref`: 预置短期/长期记忆样本引用（如适用）。

### 3. Provider 执行约束
- builtin 与 Local Provider 必须使用同一 `fixture_id`、`seed`、`timeout_ms`、`max_steps`。
- 若任一输入引用缺失，则该样本直接标记 `invalid_fixture`，不得进入正式对比结果。
- provider 不得绕过 benchmark 提供的动作白名单或额外读取未声明上下文。
- 聚合只接收当前调用的 `benchmark_run_id/scenario_id/parity_tier/provider_kind` 与预期样本下标；双方包含相同旧样本不等于批次匹配。入口应写前拒绝非空目录，或用本次显式产物集合隔离并校验来源，不静默删除旧证据。
- 离线批次验收覆盖 run-a 三样本后 run-b 一样本、scenario 变化、两 provider 同时混旧样本和 fresh 正例。文档约束不表示现有目录复用实现已修复。

## Trace 汇总字段

### 1. 每步 trace 记录
- `step_index`
- `provider_kind`
- `decision`
- `action_ref`
- `latency_ms`
- `error_code`
- `retry_count`
- `trace_present`
- `trace_message_count`
- `trace_tool_call_count`
- `context_drift_flag`

### 2. 单样本 summary
- `benchmark_run_id`
- `scenario_id`
- `fixture_id`
- `provider_kind`
- `status`: `passed` / `failed` / `blocked` / `invalid_fixture`
- `goal_completed`
- `completion_time_ms`
- `decision_steps`
- `invalid_action_count`
- `illegal_schema_count`
- `illegal_schema_rate`: metric object，见下述统一字段合同
- `recoverable_error_resolution_rate`: metric object，见下述统一字段合同
- `timeout_count`
- `recoverable_error_count`
- `fatal_error_count`
- `trace_completeness_ratio`
- `median_latency_ms`
- `p95_latency_ms`
- `notes`

### 3. 聚合 summary
- `parity_tier`
- `scenario_id`
- `provider_kind`
- `sample_count`
- `completion_rate`
- `invalid_action_rate`
- `illegal_schema_count`
- `illegal_schema_rate`: metric object，不能退化成 scalar
- `timeout_rate`
- `recoverable_error_resolution_rate`
- `median_extra_wait_ms`
- `p95_extra_wait_ms`
- `trace_completeness`
- `context_drift_count`
- `benchmark_status`

### 4. 比率字段的唯一合同

`illegal_schema_rate` 与 `recoverable_error_resolution_rate` 的计数、关联和缺项语义继承[主 PRD 的 metric-object 合同](../../llm/provider-agent-experience-parity.prd.md#parity-metric-object-contract)；`illegal_schema_rate` 只统计 `error_counts.invalid_action_schema`，不等同于动作业务拒绝率 `invalid_action_rate`。

sample、aggregate summary 与 `combined.csv` 的目标合同必须保留 `illegal_schema_count` 及完整 metric object 的 `numerator/denominator/value/zero_case/gate_status`。JSON 使用 object；现有 CSV 表头为 `metric,builtin,provider_loopback_http,gap_or_note`，比率指标各占一行，双方对象序列化在对应 backend 单元格，不是同名列。`illegal_schema_count` 行当前尚缺，属于待补实现；比率对象不能退化成单个 value。非法 schema 的聚合分子、分母分别求和 `illegal_schema_count`、`decision_steps`；恢复指标按主 PRD 的独立事件口径计算。

最小形状（仅说明 schema，不是运行结果）：

```json
{
  "decision_steps": 6,
  "illegal_schema_count": 0,
  "illegal_schema_rate": {
    "numerator": 0,
    "denominator": 6,
    "value": 0.0,
    "zero_case": null,
    "gate_status": "evaluable"
  }
}
```

必需 metric-object 字段缺失、计数矛盾或恢复事件/metric-object 的结构非法时按主 PRD 标 `blocked`，不能以默认零填充或缩小分母。普通 `invalid_action_schema` 是分子中记录的动作错误，继续按既有 `≤ 3%` / 相对 builtin `≤ 2 倍` 阈值判定，不因出现单次错误自动将样本结构判非法。分母为零时 `value=null`、`zero_case=not_applicable`、`gate_status=not_evaluable`，不构成通过证据。

<a id="parity-latency-contract"></a>
## 时延统计与解释

### 1. 当前保留的统计目标与公式

当前 benchmark 保留**最坏单样本分位上界**作为保守对照量，不将其命名或解释为所有决策汇池后的 p95。每个有效样本先对实际存在的 `latency_ms` 排序；当前 `percentile_u64` 使用零基下标 `round_half_up((n-1)*p)`（p50 为 0.50，p95 为 0.95），不是 nearest-rank，也不插值。空列表的代码输出 0 只是旧序列化值，证据解释必须标无可评估时延。

聚合保持既有字段与算法：

- `median_latency_ms` / `median_extra_wait_ms` = `round_ties_to_even(median(sample median_latency_ms))`；每个有效样本权重相同，偶数样本先取中间两值均值，再舍入整数毫秒。
- `p95_latency_ms` / `p95_extra_wait_ms` = `max(sample p95_latency_ms)`；准确解释为“最坏样本 p95”，不按各样本决策数加权。
- `extra_wait` 为历史字段名，此处仍是 provider 的**绝对观测等待**，没有减去 builtin；相对差值只在 `relative_wait_gap_{median,p95}_ms = max(0, provider - builtin)`。
- 既有行为阈值与 A/B/C 发布分级继续消费这些保守量；若改用汇池 percentile，须同时更新指标名称、阈值解释、实现和回归，不能静默替换同名字段。

### 2. 计时边界与缺样本

当前逐步 `latency_ms` 取自 decision trace 的 `llm_diagnostics.latency_ms`，不同 producer 的实际计时边界必须随候选记录：

| producer | 当前观测起止/累计方式 | 包含与不包含 |
| --- | --- | --- |
| Provider 自带 diagnostics | provider 返回的 `diagnostics.latency_ms` 优先保留 | 起止点由该 provider 声明并验证；未声明时不能推断已覆盖排队、重试或完整等待 |
| `ProviderDecisionBehavior` 的缺省计时 | 仅 response 未给 latency 时回填：构造请求后、调用 `provider.decide*` 前开始，到成功响应返回且适用的 continuous-lineage 校验完成后取 elapsed | 包含该同步调用内部网络、排队、重试/等待；不包含调用前调度/请求构造、后续 action_ref/query_ref/capability 校验与世界动作执行；提前错误路径可能无 latency |
| builtin LLM behavior | 各次 `client.complete` 前后 elapsed 的和，包括成功与失败调用及 repair 调用 | 包含调用内部等待；不包含调用之间的 prompt/repair 处理或外部调度间隙 |

因此，现有 trace 不能自动代表完整玩家端到端等待。需要比较该完整体验时，计时应从一次逻辑决策获调度开始，贯穿排队、重试/backoff，直到有效响应或终态失败/timeout；必须另有同边界的采集证据，不能将现有局部耗时改名后声称已覆盖。

有效 fixture 中 failed/blocked/timeout 的实际已测 latency 仍保留在观测分布，不因结果失败删除；没有 latency 的步保留错误与缺样本事实，不补 0、timeout 上限或成功重跑耗时。`invalid_fixture` 不入统计，但保留排除原因。记录 decision_steps 与实际 latency 样本数；边界不同或必要样本缺失时，相关时延结论为 unavailable/not_evaluable，现有数值和其他独立错误/行为门禁均保留。此解释不宣称当前采集器已补足缺失端到端计时。

### 3. 可复算反例与分组边界

固定 20 个样本、每样本 6 次决策：18 个样本为 `6×100ms`，2 个为 `5×100ms + 30000ms`；builtin 全为 100ms。采用 nearest-rank 作对照时，120 个决策的汇池 p95 取第 `ceil(120×0.95)=114` 项，等于 100ms；单样本 p95 后取最大值为 30000ms。当前上述下标算法在本例得到相同的 100ms/30000ms 两种结果，但在其他小数组上不保证与 nearest-rank 相同。

本协议的保守量为 30000ms，相对 builtin 为 29900ms；汇池对照则分别为 100ms 和 0ms。两个数字说明统计单位差异，不是任何 provider 的实测评级。重排样本不影响结果；合并/拆分样本会改变当前保守量，应固定 fixture/sample 边界，不把分组变化当性能提升。离线验证同时覆盖此数组、偶数样本中位数舍入、失败/timeout 缺值与空样本；实现结果与文档分别核对，不代替真实 provider/Runtime/双视角证据。

## 产物结构
- `artifacts/<run_id>/raw/*.jsonl`: 每步 trace 原始记录。
- `artifacts/<run_id>/summary/<scenario_id>.<provider>.json`: 单场景聚合结果。
- `artifacts/<run_id>/summary/combined.csv`: builtin 与 Local Provider 并排对比表。
- `artifacts/<run_id>/summary/failures.md`: 失败签名与阻断项摘要。
- `artifacts/<run_id>/scorecard-links.md`: 对应主观评分卡和截图/trace 链接。

## 错误码分类
- `invalid_fixture`
- `provider_unreachable`
- `version_mismatch`
- `timeout`
- `invalid_action_schema`
- `action_rejected`
- `trace_missing`
- `context_drift`
- `session_cross_talk`
- `unclassified`

## 判定规则
- 单样本命中 `invalid_fixture` 时，不纳入 completion/latency 统计。
- 单样本命中 `session_cross_talk` 或 runtime 权威绕过时，直接记为 `blocked`。
- 聚合 summary 只在有效样本数满足最小样本数时输出 `benchmark_status`；否则为 `insufficient_data`。

## PRD-ID 映射
- `PRD-WORLD_SIMULATOR-038`: 统一 fixture benchmark、trace 汇总与聚合口径。
- `PRD-WORLD_SIMULATOR-037`: provider local HTTP 模式下的 benchmark 可执行性。
- `PRD-WORLD_SIMULATOR-036`: Decision Provider 契约字段复用。

## 风险与约束
- 若 `trace_present` 仅记录布尔值而缺具体 trace 字段计数，后续会难以追根因。
- 若 combined.csv 不并排输出 builtin/Local Provider，同层对比会增加误读风险。
- 若错误码粒度过粗，失败签名会失去行动指导意义。
