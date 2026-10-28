# Runtime 核心边界验收清单

审计轮次: 4

## 目的
- 为 `TASK-WORLD_RUNTIME-002` 提供 runtime 核心边界（确定性 / WASM / 治理）的统一验收清单。
- 供 `producer_system_designer`、`runtime_engineer`、`qa_engineer` 在发布前用同一清单判断 runtime 是否达到最小可放行状态。

## 使用说明
- 每次 runtime 关键边界改动后至少抽样执行一次本清单。
- 任一 `阻断` 项失败时，不得给出 runtime 维度的 `go` 结论。
- 若当前仅能部分验证，必须标记 `blocked` 并写明缺失证据或环境约束。

## 一、确定性边界
| 验收项 | 目标 | 最小验证 | 阻断条件 |
| --- | --- | --- | --- |
| Action/Event 回放一致性 | 同一输入得到一致状态与事件链 | replay / snapshot / root 对比日志 | 出现 replay mismatch 或 `execution_state_root` 不一致 |
| 外部非确定性收敛 | 时间 / 随机 / 外部效果必须被 receipt 或 canonical log 约束 | receipt / external effect 引用检查 | 存在未被记录的外部非确定性输入 |
| 快照 / checkpoint 恢复 | latest state 可恢复，且恢复后状态根可验证 | checkpoint + canonical log 恢复验证 | 无法 latest-state 恢复或恢复后 root 不一致 |
| 数值语义稳定性 | 高风险数值路径在边界值下语义稳定 | 定向数值 / tick / 经济回归 | 溢出、漂移、边界条件语义变化未记录 |

## 二、WASM 边界
| 验收项 | 目标 | 最小验证 | 阻断条件 |
| --- | --- | --- | --- |
| 宿主接口兼容性 | ABI / wire type 变更可追溯、可兼容或显式破坏升级 | SDK / executor / host 接口抽样回归 | 未经记录的 ABI 破坏或 wire type 漂移 |
| 生命周期治理 | register / activate / deactivate / upgrade 有明确审计链 | 生命周期治理流抽样验证 | 未授权模块可激活，或升级路径缺审计 |
| 沙箱约束 | 模块不能绕过资源 / 权限 / 环境限制 | sandbox / host capability 抽样检查 | 可访问未授权能力，或资源限制失效 |
| 工件与哈希可信性 | WASM 工件、hash、版本、来源可追溯 | 工件元数据 / hash / registry 对账 | hash 漂移、来源不明或 registry 状态不一致 |

## 三、治理边界
| 验收项 | 目标 | 最小验证 | 阻断条件 |
| --- | --- | --- | --- |
| 提案到应用链路 | `propose -> shadow -> approve -> apply` 有完整治理闭环 | 治理状态机与审计事件抽样验证 | 任一关键状态可绕过或缺少审计 |
| 权限与白名单 | 仅授权主体可触发关键治理动作 | 权限 / 白名单回归 | 非授权主体可修改 runtime 关键状态 |
| 拒绝路径明确 | 冲突、阈值不足、非法模块等拒绝原因可见 | 失败路径日志 / 事件校验 | 失败但无明确拒绝原因 |
| 审计可追溯 | 治理决策、receipt、module/version 绑定可追溯 | 决策 / receipt / version 引用检查 | 无法回溯当前状态来源 |

## 四、结论记录模板
| 维度 | 当前状态 (`ready` / `fail` / `blocked`) | 证据路径 | 备注 |
| --- | --- | --- | --- |
| 确定性 |  |  |  |
| WASM |  |  |  |
| 治理 |  |  |  |

## 五、最小审查清单
- 是否三大边界（确定性 / WASM / 治理）均已填写状态与证据路径。
- 是否所有 `fail` / `blocked` 项都写明原因。
- 是否已把验收结果、风险与下一步回写对应 PR、实际 CI 与评审记录（Issue 按需），并同步现行 PRD/design（如技术合同变化）。
- 是否已标明哪些项仅完成 `test_tier_required`，哪些需后续 `test_tier_full`。
