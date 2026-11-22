# oasis7 Runtime：节点贡献积分激励

- 对应设计文档: `doc/p2p/node/node-contribution-points.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 5
## 专业权威口径
- 本文件是节点贡献积分产品与专业规则的当前主入口。
- 本三件套是节点贡献积分规则的当前专业权威。runtime closure 草案仍保留在 [world-runtime integration draft](../../world-runtime/integration/node-contribution-points-runtime-closure.md) 作为技术设计与溯源材料；其采样器和 epoch 闭环细节尚未完整吸收到当前合同，也不构成当前实现或 readiness 证据。multi-node closure test、存储系统奖励池与基础在线时长奖励增量文档的有效语义已合并到本三件套，相关源文件已删除；历史过程由 Git history 与对应 GitHub task evidence 追溯。
- 本文件定义专业积分与结算合同，不承诺玩家可用性、当前数值平衡、公开网络经济安全或 release readiness。

## 1. Executive Summary
- Problem Statement: 在 oasis7 的区块链 + P2P FS 闭环内，引入可审计的节点积分激励（Node Points）。
- Proposed Solution: 明确“基础义务”和“额外贡献”的边界：
- Success Criteria:
  - SC-1: 为自身 Agent 提供模拟计算属于基础义务，不直接奖励；
  - SC-2: 为离线节点代跑模拟、执行世界维护任务属于额外计算，应获得奖励。
  - SC-3: 为长期在线且提供更多有效存储的节点提供额外收益。

## 2. User Experience & Functionality
- User Personas: 节点贡献结算维护者、runtime 实现者与积分审计复核者。
- User Scenarios & Frequency: epoch 结算、重复 tick、重启/replay 及贡献画像回归时。
- User Stories: 作为结算维护者，需要把合格额外贡献确定性地计入对应 epoch，区分基础义务、采样与奖励台账。
- Critical User Flows: 采集贡献快照 -> epoch 边界检查基础义务、挑战资格与原计分公式 -> 按主池/独立存储池合格正得分分配 -> 产出 EpochSettlementReport 并幂等更新累计积分；重复 tick 或恢复重放仅复现同一结果。
- Functional Specification Matrix（仅承接下文既有专业合同；不新增实现或验收结论）:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 贡献资格 | NodeContributionSample | 区分自身模拟义务、代跑/维护、存储与在线 | 采样不直接更新奖励台账 | 沿 §4 原公式和阈值 | 不以启发式输入证明公网真实性 |
| 双池结算 | epoch、主池/存储池、settlements | 固定预算确定性分配，允许未分配余额 | epoch 结算才更新累计积分 | 独立存储池启用时主池排除存储权重 | 不调整权重、阈值或预算 |
| 恢复与资产路径 | collector snapshot、epoch report | 重放幂等；资产路径独立核 gate | 同 epoch 不重复入账；不可读 collector 可观测降级 | NodePoints 非自动兑换 | PowerCredit 与 Token bridge 各保留原治理边界 |
- Acceptance Criteria:
  - AC-1: 新增节点积分结算引擎（epoch 级）。
  - AC-2: 贡献维度：
  - AC-3: `delegated_sim_compute_units`（代跑离线节点）；
  - AC-4: `world_maintenance_compute_units`（世界维护任务）；
  - AC-5: `effective_storage_bytes`（有效存储）；
  - AC-6: `uptime_seconds`（在线时长）；
  - AC-7: runtime 必须把贡献快照确定性地结算到对应 epoch，重复执行或恢复重放不得重复入账，普通快照更新不得静默改变奖励台账。
  - AC-8: 多节点闭环至少覆盖 3 个节点、连续 2 个 epoch，并同时验证计算、存储、在线、可靠性与惩罚输入。
  - AC-9: 固定 epoch 积分池不得超发；贡献排序、惩罚效果与累计积分单调性必须有可重复断言。
- Non-Goals:
  - 链上可交易代币、真实经济清算。
  - 完整质押/罚没资产系统（仅保留积分惩罚入口）。
  - 复杂证明协议（PoRep/PoSt/ZK）的真实网络接线。
  - 用本地多节点夹具冒充真实公网证明、生产 readiness 或经济安全结论。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本专题不涉及 AI 模型能力改造）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 贡献快照、epoch 计分/双池分配与累计台账分层；runtime closure 草案仍按本文件专业权威口径保留其未完整吸收边界。
- Integration Points:
  - `doc/p2p/node/node-contribution-points.prd.md`
  - GitHub Issue / GitHub Project
  - `testing-manual.md`
- Edge Cases & Error Handling: 义务不足施加原惩罚，负总分归零；存储资格及封顶沿原规则。重复 tick/重启/replay 不增发；collector 不可读仅可观测降级，不证明已结算资产恢复。
- Non-Functional Requirements: 固定池不超发、排序与惩罚可重复、累计积分不回退；不将本地夹具升级为生产经济安全证明。
- Security & Privacy: 不引入敏感信息与本地绝对路径。

### 原文技术约束（保真）
#### 接口 / 数据
### 核心配置（草案）
```rust
NodePointsConfig {
  epoch_duration_seconds: u64,
  epoch_pool_points: u64,
  min_self_sim_compute_units: u64,
  storage_pool_points: u64,
  min_uptime_challenge_pass_ratio: f64,
  min_storage_challenge_pass_ratio: f64,
  min_storage_challenge_checks: u64,
  max_rewardable_storage_to_staked_ratio: f64,
  delegated_compute_multiplier: f64,
  maintenance_compute_multiplier: f64,
  weight_compute: f64,
  weight_storage: f64,
  weight_uptime: f64,
  weight_reliability: f64,
  obligation_penalty_points: f64,
}
```

### 节点贡献输入（草案）
```rust
NodeContributionSample {
  node_id: String,
  self_sim_compute_units: u64,
  delegated_sim_compute_units: u64,
  world_maintenance_compute_units: u64,
  effective_storage_bytes: u64,
  uptime_seconds: u64,
  uptime_valid_checks: u64,
  uptime_total_checks: u64,
  storage_valid_checks: u64,
  storage_total_checks: u64,
  staked_storage_bytes: u64,
  verify_pass_ratio: f64,
  availability_ratio: f64,
  explicit_penalty_points: f64,
}
```

### 结算输出（草案）
```rust
NodeSettlement {
  node_id: String,
  obligation_met: bool,
  compute_score: f64,
  storage_score: f64,
  uptime_score: f64,
  reliability_score: f64,
  storage_reward_score: f64,
  rewardable_storage_bytes: u64,
  penalty_score: f64,
  total_score: f64,
  main_awarded_points: u64,
  storage_awarded_points: u64,
  awarded_points: u64,
  cumulative_points: u64,
}

EpochSettlementReport {
  epoch_index: u64,
  pool_points: u64,
  storage_pool_points: u64,
  distributed_points: u64,
  storage_distributed_points: u64,
  total_distributed_points: u64,
  settlements: Vec<NodeSettlement>,
}
```

### 计分公式（MVP）
- 额外计算分：
  - `compute_units = delegated * delegated_multiplier + maintenance * maintenance_multiplier`
  - `compute_score = compute_units * verify_pass_ratio`
- 存储分：
  - `storage_gib = effective_storage_bytes / 1024^3`
  - `storage_score = sqrt(storage_gib) * availability_ratio`
- 独立存储奖励池：
  - `epoch_pool_points` 与 `storage_pool_points` 是两个独立的固定 epoch 积分预算；各自只向正的合格得分分配，允许出现未分配余额，不承诺自动结转。
  - 当 `storage_pool_points > 0` 时，主池归一化权重会把 `weight_storage` 置零，避免同一存储贡献同时从主池和存储池重复获奖。
  - 只有 `storage_total_checks >= min_storage_challenge_checks` 且通过率严格高于 `min_storage_challenge_pass_ratio` 时，存储奖励得分才为正；挑战质量按阈值以上区间归一化。
  - `max_rewardable_storage_to_staked_ratio > 0` 时，可奖励存储不超过 `staked_storage_bytes * ratio`；启用封顶但质押为零时可奖励量为零。零值或非有限 ratio 表示不启用该封顶。
  - `storage_reward_score = sqrt(rewardable_storage_gib) * normalized_challenge_quality * availability_ratio`。
- 在线分：
  - epoch 内存在挑战记录时，以 `uptime_valid_checks / uptime_total_checks` 为原始在线率；没有挑战记录时才回退到 `uptime_seconds / epoch_duration_seconds`。
  - 原始在线率在 `min_uptime_challenge_pass_ratio` 及以下得分为零，超过阈值后在剩余区间线性归一化到 `[0, 1]`。
- 可靠性分：
  - `reliability_score = (verify_pass_ratio + availability_ratio) / 2`
- 总分：
  - `total = w_c*compute + w_s*storage + w_u*uptime + w_r*reliability - penalty`
  - `total < 0` 则按 `0` 处理。
- 基础义务惩罚：
  - 当 `self_sim_compute_units < min_self_sim_compute_units` 时，额外加罚 `obligation_penalty_points`。
- 结算合并：
  - `awarded_points = main_awarded_points + storage_awarded_points`；累计积分只在 epoch 结算中更新，采样或快照写入不得直接增发。

### 积分与资产结算边界
- NodePoints 可进入既有 PowerCredit 路径，但仍须通过身份、签名、预算、reserve、nonce 与 replay gate；积分不是自动流动价值，也不证明 custody、市场或兑付 readiness。
- 主链 Token 的 NodePoints bridge 是另一条独立路径：它从同 epoch 的 `node_service_reward` 发行预算按 `awarded_points` 确定性分配，不是存储池或主池本身，也不是积分自动兑换。
- 同时启用多条激励路径存在重复奖励风险。调整池权重、阈值、兑换比例或发行分桶必须另行经过经济治理和 runtime 复核。

### Runtime 与多节点闭环约束
- runtime 以 epoch 边界消费节点贡献快照并产出 `EpochSettlementReport`；同一 epoch 的 settlement 必须具备幂等键或等价去重语义。
- 恢复、重放或重复 tick 只能复现同一结算结果，不得增加 `awarded_points` 或 `cumulative_points`。
- 快照采集与奖励结算分层：更新 compute/storage/uptime/reliability 样本本身不得提前写入积分台账。
- `NodePointsRuntimeCollectorSnapshot` 是采样器实现状态，不是结算授权：它持久化 ledger、heuristics、epoch 起点、每节点 cursor 与当前 epoch accumulator。`oasis7_chain_runtime` 在采样前恢复它，以保持 epoch 幂等；重复 tick、重启或 replay 不得因恢复而新增 `awarded_points` 或 `cumulative_points`。
- collector 状态由 `reward-runtime-state.json` 原子写入。文件不可读时 runtime 会显式发出 warning/metric 并以新 collector 启动；这不是资产对账、已结算状态恢复或生产 custody 成功的证明。
- 多节点验证最小拓扑为 3 个具有可区分贡献画像的节点，跨越至少 2 个 epoch；验证固定池守恒、贡献更高者排序不反转、显式或义务惩罚会降低得分，以及每个节点累计积分不回退。
- 该夹具证明确定性业务闭环，不证明采样真实性、复杂证明协议、真实传输或生产网络 readiness。

## 5. Risks & Roadmap
- Phased Rollout:
  - NCP-1：设计文档 + GitHub Issue/Project task truth。
  - NCP-2：节点积分引擎核心实现（计算/存储/在线/惩罚 + 台账）。
  - NCP-3：测试与导出接线（test_tier_required 口径）。
  - NCP-4：文档状态回写与 devlog 收口。
- Technical Risks:
  - 参数不当可能导致单一资源（大存储或大算力）垄断积分，需要通过 `sqrt(storage)` 与权重平衡缓解。
  - 若没有真实证明接线，`verify_pass_ratio/availability_ratio` 的真实性依赖上层采样器，后续需替换为链路证明数据。
  - 积分池固定时，低活跃 epoch 可能出现“有效贡献过少”，需在后续迭代加入最小活跃阈值与回收池机制。
  - collector 状态文件损坏会丢失未结算采样上下文；它必须以可观测降级处理，不能被表述为已结算资产的安全恢复。
  - uptime/storage challenge 与 collector 字段只是启发式采样输入，不是 PoRep/PoSt、VRF、多观察点证明、抵押罚没、公开网络真实性或生产经济安全证明。
  - 挑战频率过低会放大在线率或存储资格波动；存储封顶、挑战阈值和多激励路径配置不当会压制真实贡献或造成重复激励。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| 原 AC 与 §4 专业合同 | 对应业务变更的 PR / 实际验证记录 | `test_tier_required` | AC-7/8/9 与 §4 Runtime 与多节点闭环约束：至少 3 节点、连续 2 epoch，覆盖计算/存储/在线/可靠性/惩罚、池守恒、排序、累计单调和重复 tick/恢复不重复入账；积分进入资产路径另核原 gate。 | 既有专业行为及负向边界；不代签当前运行通过 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-PRD-P2P-MIG-091-001 | 历史文档迁移：逐篇阅读后人工重写 | 直接重命名 | 保证语义保真和可审计性。 |

## 原文约束点映射（内容保真）
- 原“目标” -> 第 1 章。
- 原“范围” -> 第 2 章。
- 原“接口/数据、里程碑、风险” -> 第 4~6 章。


## 既有经济结果的产品接收

以下仅把本文件既有义务准确接收到世界规则产品根，不修改原参数、角色、权限、状态、历史或执行真值。每项只引用根唯一身份，原 SC/AC/DEC 与 §6 验收仍有效；本节不是实施/发放批准。

<a id="economic-contract-points-eligibility"></a>
### 基础义务与合格额外贡献分离的既有专业接受关系

Trace refs: O7-REQ-118e8c96-669b-4b11-bbec-a32831803e16；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-points-eligibility)。

- 原义务：SC-1..3、贡献输入与计分公式；为自身 Agent 模拟是基础义务，不直接获奖励；代跑离线节点、世界维护、有效存储和合格在线仅按原专业规则进入积分计算。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[基础义务与合格额外贡献分离设计](node-contribution-points.design.md#des-economic-points-eligibility)；不能推出所有玩家获益、实际采样可信或公开网络安全。

<a id="economic-contract-points-allocation"></a>
### 固定双池的确定性且不重复分配的既有专业接受关系

Trace refs: O7-REQ-1bd30efb-22b7-403e-94f6-fe0a2efde4a2；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-points-allocation)。

- 原义务：AC-9、独立存储奖励池与结算合并规则；两个固定 epoch 池分别按原合格正得分分配，独立存储池启用时主池排除存储权重；允许未分配余额，保持守恒、排序、惩罚效果和累计单调。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[固定双池的确定性且不重复分配设计](node-contribution-points.design.md#des-economic-points-allocation)；不调整预算/权重/阈值，不证明实际数值平衡。

<a id="economic-contract-points-recovery"></a>
### epoch结算与恢复不重复入账的既有专业接受关系

Trace refs: O7-REQ-54617172-fa9b-4efe-a058-b327ce348407；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-points-recovery)。

- 原义务：AC-7、Runtime 与多节点闭环约束；同一 epoch/node 结算幂等；采样或快照写入不直接增加奖励，恢复、重复tick与replay只能复现结果；不可读collector状态须可观测降级，不是资产对账成功。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[epoch结算与恢复不重复入账设计](node-contribution-points.design.md#des-economic-points-recovery)；runtime closure草案未完整吸收；不证明已结算资产安全恢复或生产custody。

<a id="economic-contract-points-asset-gates"></a>
### 积分与独立资产路径保持门禁的既有专业接受关系

Trace refs: O7-REQ-45b12bd9-9d07-4bbf-86bc-2a85af07d5f5；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-points-asset-gates)。

- 原义务：积分与资产结算边界；积分进入PowerCredit与主链Token bridge是两条独立受治理路径，不是自动兑换或同一池；多激励并用仍需独立审查重复奖励。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[积分与独立资产路径保持门禁设计](node-contribution-points.design.md#des-economic-points-asset-gates)；不启用资产，不调整兑换比例或发行分桶，不证明兑付readiness。

## 历史 strictification 迁移记录（不属于业务流程或验收）

以下保留迁移时的原流程、矩阵、结构说明和验证来源，仅供 Git history / 原 GitHub task evidence 追溯；`draft -> active -> done` 指文档迁移，不是运行状态。正文流程与 §6 现在承接本文原 AC/技术合同，未改变经济参数、权限、实现状态或重新激活前置。

- User Personas: 协议维护者、任务执行者、质量复核者。
- User Scenarios & Frequency: 每次专题改动前后执行需求核对、测试回归与状态回写。
- User Stories: As a 维护者, I want oasis7 Runtime：节点贡献积分激励 的需求结构化, so that implementation is auditable.
- Critical User Flows: `阅读旧文档 -> 重写为 strict PRD -> 回写项目文档 -> 校验提交`。
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 专题迁移 | 需求/任务/依赖/状态/测试层级 | 逐篇重写并校验 | `draft -> active -> done` | 以原文约束点映射为主线 | 维护者写入，复核者抽检 |
- Architecture Overview: 保持原文技术边界，按 strict PRD 结构重排。
- Edge Cases & Error Handling: 命名不一致、章节缺失、引用断链需在同提交修复。
- Non-Functional Requirements: PRD-ID/任务映射完整；治理检查通过。

原迁移验证行（仅文档结构，不代签业务验收）：
| PRD-P2P-MIG-091-001 | T0~Tn | `test_tier_required` | 文档治理检查 + 章节完整性核验 | 专题文档可维护性 |

原迁移决定：
| DEC-PRD-P2P-MIG-091-001 | 逐篇阅读后人工重写 | 直接重命名 | 保证语义保真和可审计性。 |
