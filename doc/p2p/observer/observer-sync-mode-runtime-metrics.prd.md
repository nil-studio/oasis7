# oasis7 Runtime：Observer 同步源运行态统计

- 对应设计文档: `doc/p2p/observer/observer-sync-mode-runtime-metrics.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 5
## 专业权威口径
- 本文件是 Observer metrics 历史技术合同及当前状态边界权威，不是 active API 或运行态指标清单。`observer.rs` 与 `observer_metrics.rs` 已从当前 tracked source tree 删除，历史实现和同文件测试仅可从 Git history 追溯；当前 crate 根 `crates/oasis7_net/src/lib.rs` 未声明这些模块或从 facade 导出这些 API，因此它们不在当前编译模块图或 crate test targets 中。
- 下文的报告、计数、桥接和接口仅定义未来重新激活时的合同，不表示当前可调用能力、已观察指标或已经通过的测试。重新激活须由单独的 runtime 变更接回模块图和 facade，并以对应编译测试及 S9A 分层证据验证；本文件不声称任何当前 S9A tier 已通过。
- 原统计桥接 `PRD-P2P-MIG-106-001` 与策略可观测性 `PRD-P2P-MIG-107-001` 的有效设计语义与任务追踪已合并到本三件套；这项文档整理不代表 Rust source 接入 crate，也不代表测试已编译或通过。旧文档来源可从 Git history 与 GitHub task evidence 追溯。

## 1. Executive Summary
- Problem Statement: 保留 `ObserverClient` 模式化可观测报告与运行态统计的历史设计，同时避免把 Git history 中的实现当成已交付的运行能力。
- Proposed Solution: 记录未来重新激活时非 DHT 与 DHT 组合模式的核心计数合同：`total`、`applied`、`fallback`；当前 `oasis7_net` facade 不提供这些 metrics API。
- Success Criteria:
  - SC-1: 当前文档明确区分历史设计与 active facade；只有在单独重新实现并满足对应测试要求后，才可把新增结构与接口视为已提供的运行能力。

## 2. User Experience & Functionality
- User Personas: Observer 历史合同维护者、未来重新激活实现者与可观测性复核者。
- User Scenarios & Frequency: 核对当前 facade 与历史合同边界；若另行重新激活，在单轮/follow 产出报告与读取快照时。
- User Stories: 作为可观测性维护者，需要按真实同步报告计数并区分回退路径，同时不把已删除模块写成当前运行能力。
- Critical User Flows: 当前先核对模块/facade 不提供本 API；若未来重新激活：执行模式化同步 -> 成功产出 observed report -> 对应 record_* 一次 -> snapshot 供上层读取；follow 按轮记录，原聚合与 max_rounds 终止语义不变。
- Functional Specification Matrix（仅承接下文既有专业合同；不新增实现或验收结论）:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 当前状态 | module graph、facade | 核对历史模块缺失和重激活前置 | 未激活；不是当前 metrics 能力 | 旧测试不在当前 crate targets | runtime 另行变更及分层验收 |
| 模式报告计数（重新激活） | mode、report、fallback_used | 每次成功报告记录一次 | total +1；applied/fallback 条件计数 | 按报告次数，不按 head 条目 | 调用方持有 &mut metrics，无隐式全局状态 |
| follow 与快照（重新激活） | max_rounds、HeadFollowReport、snapshot | 按轮记录并读取内存快照 | 保持原聚合与终止规则 | 非 DHT/DHT 各自维度 | 不新增持久化/exporter/告警 |
- Acceptance Criteria:
  - AC-1: 若未来重新激活，在 `oasis7_net` 接入 observer 运行态统计模块（内存计数）并明确 facade 暴露边界。
  - AC-2: 若未来重新激活，提供针对 `HeadSyncModeReport` 与 `HeadSyncModeWithDhtReport` 的记录接口。
  - AC-3: 若未来重新激活，提供快照读取接口，供上层 runtime/面板周期拉取并展示。
  - AC-4: 若未来重新激活，补充并通过单元测试，覆盖各模式计数正确性与回退计数；历史 source 内同文件测试不属于当前 `oasis7_net` test targets。
  - AC-5: 若未来重新激活，非 DHT 与 DHT 模式都必须提供包含 `mode`、原同步报告和 `fallback_used` 的 observed report，且不破坏既有 `HeadSyncReport`。
  - AC-6: 若未来重新激活，提供单轮与 follow 自动记录桥接；每个成功产生的 observed report 记录一次，follow 保持原 `max_rounds` 与 `HeadFollowReport` 聚合语义。
- Non-Goals:
  - Prometheus/OpenTelemetry exporter。
  - 跨进程或落盘持久化统计。
  - 告警规则引擎与阈值策略。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本专题不涉及 AI 模型能力改造）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 历史 observed report、调用方持有的内存计数器及单轮/follow 桥接构成重新激活目标；当前模块状态与 S9A 边界见下文。
- Integration Points:
  - `doc/p2p/observer/observer-sync-mode-runtime-metrics.prd.md`
  - GitHub Issue / GitHub Project
  - `testing-manual.md`
- Edge Cases & Error Handling: fallback_used 依真实执行路径计算；无成功 observed report 不能凭空计数。不得破坏 HeadSyncReport；follow 不改变 max_rounds。历史 source 测试不代表当前编译或通过。
- Non-Functional Requirements: 按报告次数计数，保持原报告兼容与调用方状态所有权；各 S9A tier 独立取证，不以文档检查代签运行指标。
- Security & Privacy: 不引入敏感信息与本地绝对路径。

<a id="observer-metrics-dormant-status-requirement"></a>
### 当前模块与测试状态
- `observer.rs` 与 `observer_metrics.rs` 已从当前 tracked source tree 删除，历史实现和同文件测试仅可从 Git history 追溯；它们不在当前 `crates/oasis7_net/src/lib.rs` 模块图中，也不会由当前 crate 编译。
- 重新激活前须由 runtime 变更明确接回模块图及所需 facade，并运行能实际编译这些模块的定向测试。测试结果必须与要求的 S9A tier 分别记录；`test_tier_required` 只表示未来激活前的验证义务，不表示本文件记录了测试执行或通过。
- 本文不报告 `module_required`、`module_full`、`integration_required` 或 `release_full` 的当前通过状态。S9A 各 tier 的覆盖和证据边界仍以 [多节点状态同步闭环设计](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md) 为准。

### 原文技术约束（重新激活时适用）
#### 接口 / 数据
### 计数维度
- `total`: 收到一次模式化同步报告即 +1。
- `applied`: 报告 `report.applied.is_some()` 时 +1。
- `fallback`: 报告 `fallback_used == true` 时 +1。

### 新增结构（重新激活目标）
- `ObserverModeCounters`
  - `total: u64`
  - `applied: u64`
  - `fallback: u64`
- `ObserverModeRuntimeMetricsSnapshot`
  - `network_only: ObserverModeCounters`
  - `path_index_only: ObserverModeCounters`
  - `network_then_path_index: ObserverModeCounters`
- `ObserverModeWithDhtRuntimeMetricsSnapshot`
  - `network_with_dht_only: ObserverModeCounters`
  - `path_index_only: ObserverModeCounters`
  - `network_with_dht_then_path_index: ObserverModeCounters`
- `ObserverRuntimeMetricsSnapshot`
  - `mode: ObserverModeRuntimeMetricsSnapshot`
  - `mode_with_dht: ObserverModeWithDhtRuntimeMetricsSnapshot`

### 新增接口（重新激活目标）
- `ObserverRuntimeMetrics::record_mode_report(&HeadSyncModeReport)`
- `ObserverRuntimeMetrics::record_mode_with_dht_report(&HeadSyncModeWithDhtReport)`
- `ObserverRuntimeMetrics::snapshot() -> ObserverRuntimeMetricsSnapshot`

<a id="observer-sync-mode-observability-report"></a>

### 模式可观测报告（重新激活目标）
- `HeadSyncModeReport` 与 `HeadSyncModeWithDhtReport` 均包含 `mode`、既有 `HeadSyncReport` 和 `fallback_used`。
- `sync_heads_with_mode_observed_report` 与 `sync_heads_with_dht_mode_observed_report` 分别覆盖非 DHT/DHT 路径。
- `fallback_used` 必须在模式分发层按真实执行路径计算；不得修改既有 `HeadSyncReport` 字段来制造兼容性破坏。

<a id="observer-sync-mode-automatic-record-bridge"></a>

### 自动记录桥接（重新激活目标）
- 单轮桥接：`sync_heads_with_mode_observed_report_and_record`、`sync_heads_with_dht_mode_observed_report_and_record`。
- follow 桥接：`follow_heads_with_mode_and_metrics`、`follow_heads_with_dht_mode_and_metrics`。
- 每次成功产出 observed report 后调用对应 `record_*`；follow 按轮记录，但最终 `HeadFollowReport` 聚合与 `max_rounds` 终止规则保持不变。
- metrics 由调用方以 `&mut ObserverRuntimeMetrics` 持有，不引入隐式全局状态。

## 5. Risks & Roadmap
- Phased Rollout:
  - 下列阶段是重新激活的历史计划顺序；本文件不声明任何实现阶段当前已启动或完成。
  - OSRM-1：设计文档与GitHub Issue/Project task truth落地。
  - OSRM-2：实现运行态统计结构与导出接口。
  - OSRM-3：补齐单元测试并完成 `oasis7_net` 回归。
  - OSRM-4：回写状态文档与 devlog 收口。
- Technical Risks:
  - 统计语义若与调用方预期不一致（例如 `total` 是否按轮次或按 head 条目），会导致面板误判；需在文档中固定“按报告次数计数”。
  - 后续扩展更多模式可能导致字段膨胀；需保持结构可扩展且向后兼容。

## 6. Validation & Decision Record
<a id="observer-metrics-reactivation-test-plan"></a>
- Test Plan & Traceability:
  - 下表中的 `test_tier_required` 是需求标签。对 `PRD-P2P-MIG-106-001` 和 `PRD-P2P-MIG-107-001`，它表示相应 source 未来重新激活前必须取得的测试证据；当前模块未进入 `oasis7_net` 编译图，因此此标签不证明测试已编译、执行或通过。
  - 重新激活时按 S9A 的适用 tier 分别声明并记录证据；任一 tier 的结果只支持其自身定义范围。代理环境、其他 tier 或文档检查结果不能推导公开网络可达性、testnet readiness、完整游戏体验或 release readiness。

| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| 原 AC 与 §4 专业合同 | 对应业务变更的 PR / 实际验证记录 | `test_tier_required` | AC-1..6 与下文重新激活合同：先验证模块图/facade 接入，再分别覆盖非 DHT/DHT、applied/fallback 计数、单轮/follow 自动记录及 max_rounds；当前未激活、不声明测试已执行，通过状态按适用 S9A tier 独立记录。 | 既有专业行为及负向边界；不代签当前运行通过 |
| PRD-P2P-MIG-106-001 | observer-metrics-bridge-reactivation | `test_tier_required` | 重新激活后执行单轮/follow 自动记录与计数回归；当前无编译或通过证据 | Observer metrics bridge |
| PRD-P2P-MIG-107-001 | observer-mode-observability-reactivation | `test_tier_required` | 重新激活后执行非 DHT/DHT observed report 与 fallback 回归；当前无编译或通过证据 | Observer mode observability |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-PRD-P2P-MIG-108-001 | 历史文档迁移：逐篇阅读后人工重写 | 直接重命名 | 保证语义保真和可审计性。 |

## 原文约束点映射（内容保真）
- 原“目标” -> 第 1 章。
- 原“范围” -> 第 2 章。
- 原“接口/数据、里程碑、风险” -> 第 4~6 章。

## 历史 strictification 迁移记录（不属于业务流程或验收）

以下保留迁移时的原流程、矩阵、结构说明和验证来源，仅供 Git history / 原 GitHub task evidence 追溯；`draft -> active -> done` 指文档迁移，不是运行状态。正文流程与 §6 现在承接本文原 AC/技术合同，未改变经济参数、权限、实现状态或重新激活前置。

- User Personas: 协议维护者、任务执行者、质量复核者。
- User Scenarios & Frequency: 每次专题改动前后执行需求核对、测试回归与状态回写。
- User Stories: As a 维护者, I want oasis7 Runtime：Observer 同步源运行态统计 的需求结构化, so that implementation is auditable.
- Critical User Flows: `阅读旧文档 -> 重写为 strict PRD -> 回写项目文档 -> 校验提交`。
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 专题迁移 | 需求/任务/依赖/状态/测试层级 | 逐篇重写并校验 | `draft -> active -> done` | 以原文约束点映射为主线 | 维护者写入，复核者抽检 |
- Architecture Overview: 保持原文技术边界，按 strict PRD 结构重排。
- Edge Cases & Error Handling: 命名不一致、章节缺失、引用断链需在同提交修复。
- Non-Functional Requirements: PRD-ID/任务映射完整；治理检查通过。

原迁移验证行（仅文档结构，不代签业务验收）：
| PRD-P2P-MIG-108-001 | T0~Tn | `test_tier_required` | 文档治理检查 + 章节完整性核验 | 专题文档可维护性 |

原迁移决定：
| DEC-PRD-P2P-MIG-108-001 | 逐篇阅读后人工重写 | 直接重命名 | 保证语义保真和可审计性。 |
