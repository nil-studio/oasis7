# oasis7 Runtime：Observer 同步源运行态统计设计

- 对应需求文档: `doc/p2p/observer/observer-sync-mode-runtime-metrics.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

<a id="observer-metrics-dormant-status-contract"></a>
## 1. 设计定位
本文件记录 Observer 同步源运行态统计的历史设计合同及 dormant 状态，不表示当前 crate 提供这些运行时指标。`observer.rs` 与 `observer_metrics.rs` 已从当前 tracked source tree 删除；历史实现和同文件测试仅可从 Git history 追溯，也未进入 `oasis7_net` 当前编译模块图或 test targets。当前 `lib.rs` 未声明这些模块或从 crate facade 导出本设计所列 API。

同步源选择、追平进度与回退状态只有在 runtime 变更重新实现并接入模块图和所需 facade 后，才可成为可调用运行能力。Git history、设计描述或 `test_tier_required` 标签均不构成当前编译、执行或测试通过证据；本文不声称任何当前 S9A tier 已通过。


## 当前基线、目标与风险

- Owner role：`runtime_engineer`；`qa_engineer` 负责验证范围与证据判读。
- 源码审读基线：`edf61e801e4888087c2e65a402011088bd1c50b2`（2026-10-15）。当前状态是 fixed-tree / facade 静态事实，不是 Cargo、S9A 或运行验收。
- 问题与目标：保留旧模式合同的完整义务，同时让维护者能区分 dormant 接口与仍在使用的基础 flow；只有对应实现重新接线且取得实际证据后才更新能力说明。
- 完整 tracked tree 中，`crates/oasis7_net/src/` 下旧 `execution_storage.rs`、`head_follow.rs`、`head_validation.rs`、`observer.rs`、`observer_metrics.rs`、`observer_replay.rs` 与 `bootstrap.rs` 均不存在；此结论不是由稀疏目录缺文件推得。
- 当前 [lib.rs](../../../crates/oasis7_net/src/lib.rs) 仍经 `observer_flow` 导出 `HeadFollowReport`、`HeadSyncReport`、`HeadSyncResult`、`compose_head_sync_report` 与 `follow_head_sync`，经 `observer_replay_flow` 导出 `load_manifest_and_segments`。[head_sync.rs](../../../crates/oasis7_net/src/head_sync.rs) 的基础报告/循环与 [replay_flow.rs](../../../crates/oasis7_net/src/replay_flow.rs) 的加载流程仍在模块图中；它们不提供本文旧模式/metrics 接口，也不能据此声称所有 Observer 或恢复功能已删除。
- [Cargo.toml](../../../crates/oasis7_net/Cargo.toml) 的 `runtime_bridge = []` 是兼容 feature；feature 名称或基础 flow 可编译不能证明旧模式重接、fallback 或 metrics 测试已执行。
- 产品适用性：N/A；理由：本次承接的是 dormant 旧接口的专业维护合同和静态能力边界，本身不新增玩家承诺；范围：本文旧模式/metrics 接口的保存与重新激活映射。真实世界恢复、共识连续性和玩家可用性继续由现行 P2P/runtime 与产品 authority 承担，本 N/A 不否认或免除它们。
- 剩余风险：旧报告名称与 active flow 同名部分可能造成误读；未来接线须重新核对兼容、调用方持有状态及错误传播，不能把本轮文档检查当运行完成。

## 2. 设计结构
- 状态采集层（重新激活目标）：采集同步源选择、追平高度和延迟。
- 模式报告层（重新激活目标）：非 DHT/DHT observed report 同时保留 mode、原始同步报告与真实 `fallback_used`，不得破坏既有 `HeadSyncReport` 字段与兼容性；这与 follow 的 `HeadFollowReport` 聚合语义分别核对。
- 自动记录桥接层（重新激活目标）：单轮成功报告记录一次，follow 按轮记录且复用原聚合/终止语义；metrics 继续由调用方持有。
- 指标输出层（重新激活目标）：沉淀 runtime metrics 与聚合字段。
- 告警信号层（重新激活目标）：为异常 lag、切源与失败建立告警基础。
- 回归验证层（重新激活门槛）：通过已编译并执行的定向测试校验指标准确性，再按 S9A 所需 tier 取得独立证据；当前没有此类通过声明。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [模块与测试边界](observer-sync-mode-runtime-metrics.prd.md#observer-metrics-dormant-status-requirement) | 旧 metrics 模块/同文件测试不在当前编译图；基础 flow 仍存在。 | [本地合同](#observer-metrics-dormant-status-contract) | runtime / blockchain ops / QA；上游各自 owning module | 不证明当前旧 metrics API、实际计数、Cargo 或 S9A pass；不推导公网、testnet 或 release readiness。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |
| [计数维度](observer-sync-mode-runtime-metrics.prd.md#计数维度) | 每报告 total +1，applied 仅在 report.applied.is_some() 时加一，fallback 按真实路径；调用方持有内存 metrics。 | [本地合同](#4-约束与边界) | runtime / blockchain ops / QA；上游各自 owning module | 不证明当前旧 metrics API、实际计数、Cargo 或 S9A pass；不推导公网、testnet 或 release readiness。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |
| [模式报告](observer-sync-mode-runtime-metrics.prd.md#observer-sync-mode-observability-report) | mode、原报告、fallback_used 同时保留，不改 HeadSyncReport 兼容。 | [本地合同](#2-设计结构) | runtime / blockchain ops / QA；上游各自 owning module | 不证明当前旧 metrics API、实际计数、Cargo 或 S9A pass；不推导公网、testnet 或 release readiness。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |
| [自动记录桥接](observer-sync-mode-runtime-metrics.prd.md#observer-sync-mode-automatic-record-bridge) | 每次成功报告只记一次；follow 保留 max_rounds 与 HeadFollowReport 聚合，无隐式全局状态。 | [本地合同](#2-设计结构) | runtime / blockchain ops / QA；上游各自 owning module | 不证明当前旧 metrics API、实际计数、Cargo 或 S9A pass；不推导公网、testnet 或 release readiness。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |
| [重新激活测试](observer-sync-mode-runtime-metrics.prd.md#observer-metrics-reactivation-test-plan) | 新测试须进入实际 Cargo target；S9A 各 tier 证据独立。 | [本地合同](#4-约束与边界) | runtime / blockchain ops / QA；上游各自 owning module | 不证明当前旧 metrics API、实际计数、Cargo 或 S9A pass；不推导公网、testnet 或 release readiness。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |

## 3. 关键接口 / 入口
- 以下名称描述 Git history 中实现的设计目标，不是当前 crate 可调用接口：
  - 同步源运行时状态
  - `HeadSyncModeReport` / `HeadSyncModeWithDhtReport`
  - 单轮/follow metrics bridge
  - runtime metrics 字段
  - 告警聚合入口
  - 指标回归场景

## 4. 约束与边界
- 指标必须来源于真实运行态，不得手工拼装。每个 observed report 令 total +1；仅 report.applied.is_some() 令 applied +1，仅 fallback_used 为 true 令 fallback +1。这些是重新激活合同，不是当前运行读数。
- 字段命名需与 observer 其他文档保持一致。
- fallback 标识必须在模式分发层计算，桥接不得引入隐式全局 metrics 或改变 `HeadFollowReport`。
- 不在本专题定义完整告警策略。
- 重新激活须由单独的 runtime 变更重新实现并接入所需模块与 facade，并让新补测试通过 `oasis7_net` 的受支持目标实际编译。历史 colocated tests 不属于当前 crate test targets。
- PRD 中的 `test_tier_required` 表示未来重新激活前的验证义务，不是运行结果。`module_required`、`module_full`、`integration_required` 与 `release_full` 的范围及证据边界以 [S9A 多节点状态同步闭环设计](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md) 为准；其他 tier、代理或测试绿灯不能替代相应证据，也不证明公开可达性、testnet readiness 或完整游戏体验。

## 5. 设计演进计划
以下均为未来重新激活后的顺序目标，本文不声称当前已交付：
- 先由 runtime 变更接回模块图/facade，并验证运行态字段。
- 再接 metrics 输出。
- 最后补告警口径与定向回归，并按 S9A 适用 tier 单独记录结果。

## 11. 验证与证据
### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 验证方法 / 精确来源 / candidate 与环境 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [模块与测试边界](observer-sync-mode-runtime-metrics.prd.md#observer-metrics-dormant-status-requirement) | [本地合同](#observer-metrics-dormant-status-contract) | 旧 metrics 模块/同文件测试不在当前编译图；基础 flow 仍存在。 | 核对固定完整 tree、lib.rs、Cargo.toml 与当前基线；不将当前缺少旧行为测试解释为不能做文档/源码静态验证。  验证选择按 [testing manual](../../../testing-manual.md#s9a-gwsc-qa-receiver)；源码审读与未来实际执行分别记录。 | 同一候选的静态核对记录；未来实际命令、退出码、断言与适用环境分项记录 | 不证明当前旧 metrics API、实际计数、Cargo 或 S9A pass；不推导公网、testnet 或 release readiness。 |
| [计数维度](observer-sync-mode-runtime-metrics.prd.md#计数维度) | [本地合同](#4-约束与边界) | 每报告 total +1，applied 仅在 report.applied.is_some() 时加一，fallback 按真实路径；调用方持有内存 metrics。 | 现在逐字段核对 PRD 计数与快照；重新接线后针对非 DHT/DHT 三种模式分别验证 total/applied/fallback 的零次、多次及回退情况。  验证选择按 [testing manual](../../../testing-manual.md#s9a-gwsc-qa-receiver)；源码审读与未来实际执行分别记录。 | 同一候选的静态核对记录；未来实际命令、退出码、断言与适用环境分项记录 | 不证明当前旧 metrics API、实际计数、Cargo 或 S9A pass；不推导公网、testnet 或 release readiness。 |
| [模式报告](observer-sync-mode-runtime-metrics.prd.md#observer-sync-mode-observability-report) | [本地合同](#2-设计结构) | mode、原报告、fallback_used 同时保留，不改 HeadSyncReport 兼容。 | 现在核对原报告定义与未来 observed report 字段；重新实现后验证真实分发路径与报告的一致性及旧消费者兼容。  验证选择按 [testing manual](../../../testing-manual.md#s9a-gwsc-qa-receiver)；源码审读与未来实际执行分别记录。 | 同一候选的静态核对记录；未来实际命令、退出码、断言与适用环境分项记录 | 不证明当前旧 metrics API、实际计数、Cargo 或 S9A pass；不推导公网、testnet 或 release readiness。 |
| [自动记录桥接](observer-sync-mode-runtime-metrics.prd.md#observer-sync-mode-automatic-record-bridge) | [本地合同](#2-设计结构) | 每次成功报告只记一次；follow 保留 max_rounds 与 HeadFollowReport 聚合，无隐式全局状态。 | 现在对照 PRD 与 head_sync.rs 基础循环；未来补单轮/follow 零轮、停止、成功/失败和重复记录负例，实际编译测试后分别取证。  验证选择按 [testing manual](../../../testing-manual.md#s9a-gwsc-qa-receiver)；源码审读与未来实际执行分别记录。 | 同一候选的静态核对记录；未来实际命令、退出码、断言与适用环境分项记录 | 不证明当前旧 metrics API、实际计数、Cargo 或 S9A pass；不推导公网、testnet 或 release readiness。 |
| [重新激活测试](observer-sync-mode-runtime-metrics.prd.md#observer-metrics-reactivation-test-plan) | [本地合同](#4-约束与边界) | 新测试须进入实际 Cargo target；S9A 各 tier 证据独立。 | 按 `../../../testing-manual.md#s9a-gwsc-qa-receiver`（S9A） 与 `../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#2-claim-boundary`（tier 定义） 选择对应环境；当前只核对测试计划，未来记录实际编译目标、断言与每个 tier 结果。  验证选择按 [testing manual](../../../testing-manual.md#s9a-gwsc-qa-receiver)；源码审读与未来实际执行分别记录。 | 同一候选的静态核对记录；未来实际命令、退出码、断言与适用环境分项记录 | 不证明当前旧 metrics API、实际计数、Cargo 或 S9A pass；不推导公网、testnet 或 release readiness。 |
