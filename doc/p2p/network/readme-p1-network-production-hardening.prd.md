# README P1 缺口收口：分布式网络主路径生产化

- 对应设计文档: `doc/p2p/network/readme-p1-network-production-hardening.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 5
## 1. Executive Summary
- Problem Statement: 收口 P1-1：将 Node 共识消息（proposal/attestation/commit）从“仅 UDP gossip”升级为“libp2p pubsub 主路径”，保留 UDP 兼容兜底。
- Proposed Solution: 收口 P1-2：将 libp2p request/response 从“单 peer + 无 peer 本地 handler 回退”升级为“多 peer 轮换重试 + 可控本地回退策略”。
- Success Criteria:
  - SC-1: 保持 node 生产默认拓扑（triad/triad_distributed）可用，并保证 required-tier 回归稳定；viewer 只消费可观测状态，不拥有 node、拓扑或共识 gate。

## 2. User Experience & Functionality
- User Personas: Node 网络维护者、协议实现者与网络回归复核者。
- User Scenarios & Frequency: 发送共识消息、处理 request 或核对无 peer/失败路径时。
- User Stories: 作为 Node 网络维护者，需要按既有路由合同发送共识消息并解释失败，避免把本地回退误当远端成功。
- Critical User Flows: 共识广播优先选择 libp2p endpoint，否则走 UDP；两路消息按既有高度/哈希守卫 ingest。request 按连接 peer 轮换，失败后尝试剩余 peer，耗尽返回结构化错误；无 peer 默认拒绝，只有显式开关才允许本地 handler。
- Functional Specification Matrix（仅承接下文既有专业合同；不新增实现或验收结论）:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 共识传输 | proposal/attestation/commit topic | libp2p 优先、UDP 兼容；两路均可 ingest | 按既有高度/哈希幂等守卫处理 | 既有 topic 与 JSON 载荷 | Node 主循环；viewer 只消费状态 |
| request 路由 | connected peers、fallback flag | 轮换选择 peer，失败重试余下 peer | 成功返回；耗尽返回 NetworkRequestFailed | ErrorResponse 也计为失败 | 无 peer 本地回退默认关闭 |
| 目标平台 | native / wasm32 | native 使用 oasis7_net；wasm stub 保留 API 形状 | wasm 网络调用返回 NetworkProtocolUnavailable | 不得静默成功或隐式本地网络 | 不扩大 wasm 为完整 Node 运行目标 |
- Acceptance Criteria:
  - AC-1: In scope
  - AC-2: `crates/oasis7_node/src/libp2p_replication_network.rs`
  - AC-3: request 路由升级为多 peer 轮换 + 失败重试。
  - AC-4: 增加“无 peer 时本地 handler 回退”开关，默认关闭。
  - AC-5: `crates/oasis7_node/src/network_bridge.rs`
  - AC-6: 新增共识 topic endpoint（proposal/attestation/commit）。
- Non-Goals:
  - 不扩展超出原文边界的新需求。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本专题不涉及 AI 模型能力改造）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 按下文既有 request 路由、共识 topic、Node 主循环优先级及目标平台边界组织网络合同。
- Integration Points:
  - `doc/p2p/network/readme-p1-network-production-hardening.prd.md`
  - GitHub Issue / GitHub Project
  - `testing-manual.md`
- Edge Cases & Error Handling: 远端 ErrorResponse 与 outbound 失败继续轮换，peer 耗尽返回结构化错误；无 peer 默认 NetworkProtocolUnavailable。双路径重复消息沿既有幂等守卫处理；wasm stub 不得静默成功。
- Non-Functional Requirements: 保持既有消息载荷与 API 形状；不把文档检查或目标编译命令当作网络行为通过证据。
- Security & Privacy: 不引入敏感信息与本地绝对路径。

### 原文技术约束（保真）
#### 接口 / 数据
### 1) libp2p request 路由
- `Libp2pReplicationNetworkConfig` 新增：
  - `allow_local_handler_fallback_when_no_peers: bool`（默认 `false`）。
- request 语义：
  - 有连接 peer：按轮换顺序选择 peer，失败自动切下一个，直到成功或耗尽。
  - 远端返回 `ErrorResponse` 时按请求失败分类，继续尝试剩余 peer；全部耗尽后返回结构化 `NetworkRequestFailed`。
  - 无连接 peer：默认返回 `NetworkProtocolUnavailable`；仅在开关开启时允许本地 handler 回退。

### 2) 共识 topic（libp2p）
- 主题命名：
  - `aw.<world_id>.consensus.proposal`
  - `aw.<world_id>.consensus.attestation`
  - `aw.<world_id>.consensus.commit`
- 载荷：复用既有 `GossipProposalMessage/GossipAttestationMessage/GossipCommitMessage` JSON 编码。

### 3) Node 主循环广播/消费优先级
- 广播优先级：
  - 有 libp2p 共识 endpoint：走 libp2p。
  - 否则走 UDP gossip。
- 消费优先级：
  - 两路均可 ingest；libp2p 路径用于生产默认主链路，UDP 作为兼容链路。

<a name="4-oasis7net-与目标平台边界"></a>

### 4) `oasis7_net` 与目标平台边界
- `oasis7_node` 在 native target 依赖 `oasis7_net`，复用其网络实现与公开 API；不得形成反向依赖、依赖环或额外 runtime bridge。
- `wasm32` 不是完整 node/libp2p 运行目标。target cfg 必须选择 native 实现或 wasm stub，同时保留 `Libp2pReplicationNetwork`、配置结构和 peer identity/key derivation 的 API 形状，避免上层条件编译漂移。
- wasm stub 的网络操作必须返回结构化 `WorldError::NetworkProtocolUnavailable`，并明确指出 `wasm32` 不支持 node libp2p；不得静默成功或退化为隐式本地网络。
- 编译护栏命令：`env -u RUSTC_WRAPPER cargo check -p oasis7_node --target wasm32-unknown-unknown --features libp2p`。该命令是目标态验收入口；2026-07-27 新鲜验证仍会先被依赖闭包中的 `getrandom 0.2` 缺少 `js` feature 阻断，因此当前只能证明 target cfg/API 约束已定义，不能声明 package-level wasm32 编译门禁已绿。

## 5. Risks & Roadmap
- Phased Rollout:
  - M1：T0 文档冻结（设计 + 项管）。
  - M2：T1 libp2p request 路由升级。
  - M3：T2 node 共识消息 libp2p 主路径接线。
  - M4：T3 测试回归、文档和 devlog 收口。
- Technical Risks:
  - 网络行为风险：多 peer 重试引入请求状态机复杂度，需严格处理 response/outbound-failure 的 pending 迁移。
  - 兼容风险：共识消息双路径（libp2p/UDP）并存阶段可能出现重复消息，需依赖现有高度/哈希幂等守卫。
  - 稳定性风险：topic ingest 新增后测试时序更敏感，需避免 flaky 断言。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| 原 AC 与 §4 专业合同 | 对应业务变更的 PR / 实际验证记录 | `test_tier_required` | AC-3/4/6 与 §4 request、topic、主循环合同：验证多 peer 成功/失败轮换、ErrorResponse、peer 耗尽、无 peer 开关及 libp2p/UDP 路径；wasm 检查沿 §4 既有命令和已记录依赖阻断，不宣称已绿。 | 既有专业行为及负向边界；不代签当前运行通过 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-PRD-P2P-MIG-086-001 | 历史文档迁移：逐篇阅读后人工重写 | 直接重命名 | 保证语义保真和可审计性。 |

## 原文约束点映射（内容保真）
- 原“目标” -> 第 1 章。
- 原“范围” -> 第 2 章。
- 原“接口/数据、里程碑、风险” -> 第 4~6 章。

## 历史 strictification 迁移记录（不属于业务流程或验收）

以下保留迁移时的原流程、矩阵、结构说明和验证来源，仅供 Git history / 原 GitHub task evidence 追溯；`draft -> active -> done` 指文档迁移，不是运行状态。正文流程与 §6 现在承接本文原 AC/技术合同，未改变经济参数、权限、实现状态或重新激活前置。

- User Personas: 协议维护者、任务执行者、质量复核者。
- User Scenarios & Frequency: 每次专题改动前后执行需求核对、测试回归与状态回写。
- User Stories: As a 维护者, I want README P1 缺口收口：分布式网络主路径生产化 的需求结构化, so that implementation is auditable.
- Critical User Flows: `阅读旧文档 -> 重写为 strict PRD -> 回写项目文档 -> 校验提交`。
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 专题迁移 | 需求/任务/依赖/状态/测试层级 | 逐篇重写并校验 | `draft -> active -> done` | 以原文约束点映射为主线 | 维护者写入，复核者抽检 |
- Architecture Overview: 保持原文技术边界，按 strict PRD 结构重排。
- Edge Cases & Error Handling: 命名不一致、章节缺失、引用断链需在同提交修复。
- Non-Functional Requirements: PRD-ID/任务映射完整；治理检查通过。

原迁移验证行（仅文档结构，不代签业务验收）：
| PRD-P2P-MIG-086-001 | T0~Tn | `test_tier_required` | 文档治理检查 + 章节完整性核验 | 专题文档可维护性 |

原迁移决定：
| DEC-PRD-P2P-MIG-086-001 | 逐篇阅读后人工重写 | 直接重命名 | 保证语义保真和可审计性。 |
