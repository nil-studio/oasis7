# 离线工程遥测 fixture

`input.json` 是 `sample_kind=synthetic` 的准备型 v2 输入，不是 GitHub API 原始响应，也不证明性能收益。五个 job 的合成 DAG 用于独立复算：反馈 21 秒、job 执行合计 25 秒、执行最长链 19 秒、未归因等待 2 秒、含等待观测链 21 秒、首次启动延迟 0 秒。

从仓库根目录运行：

```sh
python3 scripts/engineering_telemetry.py collect scripts/tests/fixtures/engineering-telemetry/input.json /tmp/telemetry-snapshot.json
# 完整 v2 合同返回 0；partial 或采集失败返回 1。
python3 scripts/engineering_telemetry.py report /tmp/telemetry-snapshot.json /tmp/telemetry-report.json /tmp/telemetry-report.md
python3 -m unittest discover -s scripts/tests -p 'test_engineering_telemetry*.py'
```

## 来源与版本边界

v2 输入 schema 为 `oasis7-engineering-telemetry-input/v2`，snapshot/report 使用对应 v2。`sources` 包含 `reference / kind / source_schema / payload`；来源 schema 是合同定义的适配标签，不能把准备型 payload 当作已认证的外部原始事实。contexts/records 的每个事实字段有 `field_sources`：known 必须给出解析到相同类型和值的 reference/JSON Pointer；unknown 或 not_applicable 必须给出 reason，字段值为 null。输出只保留允许字段、evidence 中的来源内容 SHA-256 和字段引用，不复制 raw payload。每个输出字段的来源是列表；同身份同值的重复事实合并 alias 和来源，不同值保留 conflict。

DAG 仅分析完整 GitHub context 的 job 层，不重复累计 step/phase/test。必要 job、终态、身份、分页、条件和依赖均须完整，图无环且 gate 覆盖必要上游；缺失或冲突使受影响指标 unavailable，已知子集单列 partial。attempt 和矩阵 job ID 独立；producer 不冒充 consumer。本地 invocation 与测量时 dirty 输入身份缺失不能事后补签。完整失败/取消终态可计算时间，但不改写原结论；false 条件的 skipped 节点不等于 pass。

`analysis.formula_version` 为 `oasis7-engineering-dag/v1`，`analysis.contexts[].metrics` 含 status/value/unit/formula/input_ids，完整图的 `job_timings` 展示未归因等待；图不完整时留空，missing_reasons 列明缺口，known_job_execution_seconds 对非空可证子集以 partial 和输入列表单列，完全没有可证区间时为 status=unavailable、value=null。时间保留 UTC 精度，不跨进程相减单调计时，也不从 start + elapsed 推导精确跨 job 端点。

原 v1 准备型输入保留在测试内，原 schema 始终 partial，analysis unavailable。`collect-source KIND INPUT OUTPUT --reference RELATIVE_PATH --collection-id ID --collected-at UTC --origin {github,local}` 继续支持 `actions-jobs`、`process-jsonl`、`build-manifest`、`compile-metrics`。它投影单份离线输入、记录原始字节 SHA-256，写出 partial snapshot 后返回 1，不验证引用真实性或把 artifact 绑定到 run。v1 准备型输入摘要针对规范化记录列表的 canonical JSON；v2 来源摘要针对各 payload 的 canonical JSON。

仅使用整理过的非敏感输入。允许字段和受限标识符不是秘密检测器；输入限制为 32 MiB，完整请求/时间预算、live collector 和真实 CI/本地核验仍待实施。报告只支持复算，不签发 GitHub check。report 返回 0 只代表成功渲染，原测试退出码、失败/取消及清理结果不变。
