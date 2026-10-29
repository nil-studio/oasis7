# Local Provider vs 内置 Agent parity 聚合结论模板

审计轮次: 2

适用范围: `PRD-WORLD_SIMULATOR-038` 的 `T2/T4/T5`，用于汇总自动 benchmark、trace 统计和 QA/producer 评分卡结论。

---

## 一、批次信息
- benchmark_run_id:
- parity_tier:
- provider_version:
- adapter_version:
- protocol_version:
- agent_profile:
- 执行日期:
- 执行人:

## 二、样本覆盖
| Scenario-ID | provider | sample_count | valid_samples | invalid_fixture | benchmark_status |
| --- | --- | --- | --- | --- | --- |
| P0-001 | builtin |  |  |  |  |
| P0-001 | provider_loopback_http |  |  |  |  |

## 三、核心指标并排对比
| 指标 | builtin | Local Provider | gap / 备注 | 是否达标 |
| --- | --- | --- | --- | --- |
| completion_rate |  |  |  | [ ] |
| invalid_action_rate |  |  |  | [ ] |
| illegal_schema_rate |  |  | `illegal_schema_count / decision_steps`; missing or inconsistent => blocked | [ ] |
| timeout_rate |  |  |  | [ ] |
| median_extra_wait_ms |  |  |  | [ ] |
| p95_extra_wait_ms |  |  |  | [ ] |
| relative_wait_gap_median_ms |  |  |  | [ ] |
| relative_wait_gap_p95_ms |  |  |  | [ ] |
| latency_class |  |  | `A/B/C` | [ ] |
| trace_completeness |  |  |  | [ ] |
| recoverable_error_resolution_rate |  |  |  | [ ] |
| context_drift_count |  |  |  | [ ] |

`illegal_schema_rate` 必须从每个 sample 的 `illegal_schema_count / decision_steps` 聚合，且
仅对应 `invalid_action_schema`。sample 与聚合 summary 必须保留
`numerator/denominator/value/zero_case/gate_status` object；缺少字段、计数超过
`decision_steps`、或与 `error_counts.invalid_action_schema` 不一致时标记为 `blocked`，不得按缺失为零。

时延字段按[唯一时延合同](provider-agent-parity-benchmark-protocol.md#parity-latency-contract)填写：`p95_extra_wait_ms` 是最坏样本 p95，median 是各样本中位数的中位数；保留实际 latency 数量、producer 计时边界与缺失项。时延证据不完整时，不能因零值或存在 summary 就填“达标”。其他 metric object/zero-case 规则以[主 PRD](../../llm/provider-agent-experience-parity.prd.md#parity-metric-object-contract)为准。

## 四、失败签名汇总
| error_code | builtin count | Local Provider count | 是否阻断 | 备注 |
| --- | --- | --- | --- | --- |
| provider_unreachable |  |  | [ ] |  |
| timeout |  |  | [ ] |  |
| invalid_action_schema |  |  | [ ] |  |
| context_drift |  |  | [ ] |  |
| session_cross_talk |  |  | [ ] |  |

## 五、主观评分关联
- QA 评分卡路径:
- Producer 评分卡路径:
- 关键截图/trace 证据路径:
- 自动 benchmark 证据路径:

## 六、行为等价结论
- [ ] 行为等价硬门禁全部通过
- [ ] completion gap 超线
- [ ] invalid_action_rate / illegal_schema_rate / timeout_rate 超线
- [ ] `relative_wait_gap` 超线
- [ ] trace 不完整导致无法诊断
- [ ] 记忆连续性明显漂移
- [ ] 会话串线 / provider session 污染
- 备注:

## 七、发布 / 默认启用结论
- latency_class: [ ]A(default-candidate) [ ]B(experimental-only) [ ]C(blocked)
- [ ] 可进入默认启用候选
- [ ] 仅允许保持 experimental / 受限试点
- [ ] 必须阻断
- 备注:

## 八、最终结论
- 行为等价结论: [ ]blocked [ ]failed [ ]conditional_pass [ ]pass
- 发布 / 默认启用结论: [ ]blocked [ ]experimental_only [ ]default_candidate
- 建议状态: [ ]保持 experimental [ ]进入下一层 parity [ ]允许默认启用
- 必修项:
- 建议优化项:
- 备注:
