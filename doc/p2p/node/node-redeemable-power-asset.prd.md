# oasis7 Runtime：可兑现节点资产与电力兑换闭环

- 对应设计文档: `doc/p2p/node/node-redeemable-power-asset.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 5
## 专业权威口径
- 本文件是可兑现节点资产、审计和签名治理的当前专业权威。
- 原审计加固 `PRD-P2P-MIG-096-001` 与签名治理 `PRD-P2P-MIG-097-001` 的有效语义与任务追踪已合并到本三件套；源文件删除后只从 Git history 与 GitHub task evidence 追溯。

## 1. Executive Summary
- Problem Statement: 将现有 Node Points 从“统计积分”升级为“可兑现协议资产”，使算力/存储节点可将收益兑换为 Agent 电力。
- Proposed Solution: 在不破坏现有 PoS + DistFS 能力的前提下，补齐“结算上链、兑换执行、守恒风控、可审计回放”最小经济闭环。
- Success Criteria:
  - SC-1: 用最小可运行方案覆盖前序评估的 8 个关键缺口：
  - SC-2: 资产层缺失；
  - SC-3: 结算未上链；

## 2. User Experience & Functionality
- User Personas: 节点资产账本维护者、兑换执行者与签名/恢复审计复核者。
- User Scenarios & Frequency: 积分结算铸造、RedeemPower、签名策略校验与快照恢复时。
- User Stories: 作为资产维护者，需要按原身份与预算条件把积分记为受限 PowerCredit，兑换时保持资产、储备与目标 Agent 电力一致。
- Critical User Flows: EpochSettlementReport -> 按原 points_per_credit 与签名治理检查生成 mint 记录和受限余额 -> RedeemPower 校验余额、nonce、最小量、epoch 储备/上限及签名 -> 成功时扣资产/储备并增加目标电力，失败保留可审计拒绝 -> 快照/replay 复核一致性。
- Functional Specification Matrix（仅承接下文既有专业合同；不新增实现或验收结论）:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 结算铸造 | NodeRewardMintRecord、NodeAssetBalance | 按结算与签名策略形成余额/累计统计 | 积分结算进入链状态与快照 | floor(awarded_points / points_per_credit) | PowerCredit 不可直接转账 |
| 电力兑换 | RedeemPower、ProtocolPowerReserve | 先验余额/nonce/最小量/储备与上限 | 成功产生 PowerRedeemed；失败为拒绝 | 沿原比例与 epoch 限额 | 遵守既有签名与 node 绑定 |
| 审计恢复 | snapshot、RewardAssetInvariantReport | 回放复核余额/储备与签名策略 | 异常检测，不自动修复或对账 | v1 摘要不等于私钥签名 | 策略无法满足时 fail closed |
- Acceptance Criteria:
  - AC-1: **RPA-1：可兑现资产层（PowerCredit）**
  - AC-2: 新增协议资产 `PowerCredit`（不可直接转账，先支持系统铸造与兑换燃烧）。
  - AC-3: 资产账本按 `node_id` 维护余额与累计统计（mint/burn/redeem）。
  - AC-4: **RPA-2：Node Points 结算上链**
  - AC-5: 将 `EpochSettlementReport` 产出的奖励从“内存结果”写入链状态。
  - AC-6: 增加结算记录结构（epoch -> node mint 明细）并纳入状态快照/回放。
- Non-Goals:
  - 可自由转账的通用代币经济系统（交易市场、AMM、税率、复杂货币政策）。
  - 完整 PoRep/PoSt/VRF 网络协议与多观察点拜占庭证明。
  - 完整链上仲裁与法律化争议处理流程。
  - 全量生产级存储安全能力（ACL、全链路加密、跨地域复制策略）。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本专题不涉及 AI 模型能力改造）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 按下文资产/储备账本、结算 mint 记录、兑换动作事件与签名治理分层；不改变资产比例或兑现范围。
- Integration Points:
  - `doc/p2p/node/node-redeemable-power-asset.prd.md`
  - GitHub Issue / GitHub Project
  - `testing-manual.md`
- Edge Cases & Error Handling: 余额/储备不足、nonce 重放、最小量或 epoch 上限不满足时拒绝；缺少所需签名或绑定 fail closed。旧快照可反序列化不免除策略与 invariant report 复核。
- Non-Functional Requirements: 铸造/燃烧/兑换守恒和快照回放一致；审计检测不自动修复，不证明 custody 或生产 readiness。
- Security & Privacy: 不引入敏感信息与本地绝对路径。

### 原文技术约束（保真）
#### 接口 / 数据
### 1) 资产与储备账本（草案）
```rust
RewardAssetConfig {
  // NodePoints -> PowerCredit 转换比例
  points_per_credit: u64,
  // PowerCredit -> 电力单位转换比例
  credits_per_power_unit: u64,
  // 每 epoch 最多可兑付电力
  max_redeem_power_per_epoch: i64,
  // 最小兑换电力单位
  min_redeem_power_unit: i64,
}

NodeAssetBalance {
  node_id: String,
  power_credit_balance: u64,
  total_minted_credits: u64,
  total_burned_credits: u64,
}

ProtocolPowerReserve {
  epoch_index: u64,
  available_power_units: i64,
  redeemed_power_units: i64,
}
```

### 2) 结算上链记录（草案）
```rust
NodeRewardMintRecord {
  epoch_index: u64,
  node_id: String,
  source_awarded_points: u64,
  minted_power_credits: u64,
  settlement_hash: String,
  signer_node_id: String,
  signature: String,
}
```

### 3) 兑换动作与事件（草案）
```rust
Action::RedeemPower {
  node_id: String,
  target_agent_id: String,
  redeem_credits: u64,
  nonce: u64,
}

DomainEvent::PowerRedeemed {
  node_id: String,
  target_agent_id: String,
  burned_credits: u64,
  granted_power_units: i64,
  reserve_remaining: i64,
}

DomainEvent::PowerRedeemRejected {
  node_id: String,
  target_agent_id: String,
  redeem_credits: u64,
  reason: String,
}
```

### 4) 关键规则
- `minted_power_credits = floor(awarded_points / points_per_credit)`。
- 兑换前检查：
  - `redeem_credits > 0`；
  - 节点余额充足；
  - `nonce` 严格单调；
  - `granted_power_units >= min_redeem_power_unit`；
  - 目标 epoch 储备池与上限允许。
- 兑换执行：
  - `NodeAssetBalance.power_credit_balance -= redeem_credits`；
  - `ProtocolPowerReserve.available_power_units -= granted_power_units`；
  - `Agent.resource.electricity += granted_power_units`。

### 5) 状态与回放一致性
- 将 `NodeAssetBalance`、`ProtocolPowerReserve`、`NodeRewardMintRecord` 纳入快照。
- 事件重放必须重建相同余额与储备结果；不一致视为状态错误。

### 6) 审计与签名治理演进
- `mintsig:v1:<sha256_hex>` 是覆盖 epoch、node、积分、铸造量、settlement hash 与 signer 绑定字段的可重算历史摘要，不等同真实私钥签名。
- `mintsig:v2:<signature_hex>` 使用 Ed25519 对版本化结算载荷签名；`redeemsig:v1:<signature_hex>` 对 node、目标 Agent、credits、nonce 与 signer 绑定字段签名。
- `verify_reward_mint_record_signature` 在兼容期支持 v1/v2；当前 chain-runtime reward worker 的安全默认要求 v2、禁用 v1 fallback、要求兑换签名，并要求 `signer_node_id == node_id`。
- `RewardSignatureGovernancePolicy` 显式控制 `require_mintsig_v2`、`allow_mintsig_v1_fallback` 与 `require_redeem_signature`；策略要求无法满足时必须 fail closed。
- `RewardAssetInvariantReport` 汇总 mint/burn/balance/record 并列出异常；它用于检测和审计，不自动修复或对账。旧快照即使可通过 serde 默认值反序列化，恢复/同步验收仍必须复核签名策略与 invariant report。
- 当前 operator 证据入口是 chain-runtime CLI、runtime root 与 report artifacts；历史 `oasis7_viewer_live` 参数只作为发布 provenance，不是现行 runbook。
- 本地 collector 重置或配置中的 signer 绑定不构成跨组织 custody、HSM/KMS 治理、mainnet readiness 或资产对账结论；这些边界不得由本专题扩大。

#### 测试策略
- `test_tier_required`：
  - 资产铸造守恒（points -> credits）；
  - 兑换扣账与电力增加一致；
  - 储备池不足拒绝；
  - nonce 重放拒绝；
  - 快照恢复后余额一致。
- `test_tier_full`：
  - 多节点多 epoch 结算与兑换压力场景；
  - 节点重启 + 回放一致性；
  - libp2p 复制下的资产状态一致性抽样。

## 5. Risks & Roadmap
- Phased Rollout:
  - **RPA-M0**：设计文档与GitHub Issue/Project task truth。
  - **RPA-M1**：资产账本 + 配置 + 快照持久化。
  - **RPA-M2**：Node Points 结算上链铸造接线。
  - **RPA-M3**：`RedeemPower` 动作与事件闭环。
  - **RPA-M4**：守恒与风控（储备池、额度、nonce）。
  - **RPA-M5**：运行时接线（含 `oasis7_viewer_live` 开关）。
  - **RPA-M6**：最小需求侧支付入口与回归。
  - **RPA-M7**：身份签名治理最小收口。
  - **RPA-M8**：DistFS 证明字段增强与文档/devlog 收口。
- Technical Risks:
  - 参数配置风险：`points_per_credit` 与 `credits_per_power_unit` 配置不当会导致通胀或激励不足。
  - 守恒风险：若储备池更新与兑换动作不是原子处理，可能出现短时超发。
  - 治理风险：身份绑定策略未完全统一前，需默认严格校验签名与 node_id 映射。
  - 演进风险：MVP 需求侧支付入口仍偏系统计划分配，后续需逐步演进到真实订单市场。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| 原 AC 与 §4 专业合同 | 对应业务变更的 PR / 实际验证记录 | `test_tier_required` + `test_tier_full` | AC-1..6 与 §4 测试策略：required 验证 points->credits 守恒、兑换扣账/电力一致、储备不足/nonce 重放拒绝和快照余额；full 保留多节点多 epoch、重启/replay、libp2p 状态一致性抽样；MIG-096/097 签名/审计验证独立保留。 | 既有专业行为及负向边界；不代签当前运行通过 |
| PRD-P2P-MIG-096-001 | redeemable-asset-audit-authority | `test_tier_required` + `test_tier_full` | v1 摘要、签名校验、invariant report 与恢复一致性 | Reward asset audit |
| PRD-P2P-MIG-097-001 | redeemable-asset-signature-authority | `test_tier_required` + `test_tier_full` | v2/redeem 签名、治理拒绝路径与快照策略回归 | Reward signature governance |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-PRD-P2P-MIG-098-001 | 历史文档迁移：逐篇阅读后人工重写 | 直接重命名 | 保证语义保真和可审计性。 |

## 原文约束点映射（内容保真）
- 原“目标” -> 第 1 章。
- 原“范围” -> 第 2 章。
- 原“接口/数据、里程碑、风险” -> 第 4~6 章。


## 既有经济结果的产品接收

以下仅把本文件既有义务准确接收到世界规则产品根，不修改原参数、角色、权限、状态、历史或执行真值。每项只引用根唯一身份，原 SC/AC/DEC 与 §6 验收仍有效；本节不是实施/发放批准。

<a id="economic-contract-power-ledger"></a>
### 贡献形成受限协议资产账本的既有专业接受关系

Trace refs: O7-REQ-c0f136a1-3baf-497e-a0ec-622e745024d0；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-power-ledger)。

- 原义务：AC-1..6、资产与储备账本和结算记录；PowerCredit保持不可直接转账的受限协议资产；按原积分换算和mint记录形成可追溯余额，不扩展通用交易市场。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[贡献形成受限协议资产账本设计](node-redeemable-power-asset.design.md#des-economic-power-ledger)；不更改兑换率，不新增转账/市场或当前可兑现承诺。

<a id="economic-contract-power-redemption"></a>
### 全部条件成立才燃烧资产并增加目标电力的既有专业接受关系

Trace refs: O7-REQ-a061488d-5b14-489a-af77-ed3f6f75f1bb；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-power-redemption)。

- 原义务：兑换动作与事件、关键规则；兑换沿原余额、nonce、最小量、epoch储备与上限条件；成功须扣资产、扣储备与增加指定Agent电力一致，失败不得伪装兑付成功。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[全部条件成立才燃烧资产并增加目标电力设计](node-redeemable-power-asset.design.md#des-economic-power-redemption)；不改额度、目标、权限或承诺当前服务可用。

<a id="economic-contract-power-signature"></a>
### 签名治理条件不足必须拒绝的既有专业接受关系

Trace refs: O7-REQ-85c5baeb-e05a-4924-9024-e05f1903bd2a；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-power-signature)。

- 原义务：审计与签名治理演进、MIG-097验收；v1仅历史可重算摘要；v2与兑换签名依既有治理策略和绑定执行，缺少密钥、绑定或所需策略条件时fail closed。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[签名治理条件不足必须拒绝设计](node-redeemable-power-asset.design.md#des-economic-power-signature)；不接触密钥，不证明HSM/KMS或跨组织custody。

<a id="economic-contract-power-audit"></a>
### 审计与恢复检测异常而不自动修复的既有专业接受关系

Trace refs: O7-REQ-9804d9d7-2c75-475f-b321-5c4ed36276b5；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-power-audit)。

- 原义务：状态与回放一致性、审计与签名治理演进、MIG-096验收；mint/burn/balance/record与快照/replay保持一致；invariant report用于检测而非自动修复，旧快照可读取不免除签名策略与报告复核。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[审计与恢复检测异常而不自动修复设计](node-redeemable-power-asset.design.md#des-economic-power-audit)；不证明mainnet或生产恢复，不恢复历史viewer flags入口。

## 历史 strictification 迁移记录（不属于业务流程或验收）

以下保留迁移时的原流程、矩阵、结构说明和验证来源，仅供 Git history / 原 GitHub task evidence 追溯；`draft -> active -> done` 指文档迁移，不是运行状态。正文流程与 §6 现在承接本文原 AC/技术合同，未改变经济参数、权限、实现状态或重新激活前置。

- User Personas: 协议维护者、任务执行者、质量复核者。
- User Scenarios & Frequency: 每次专题改动前后执行需求核对、测试回归与状态回写。
- User Stories: As a 维护者, I want oasis7 Runtime：可兑现节点资产与电力兑换闭环 的需求结构化, so that implementation is auditable.
- Critical User Flows: `阅读旧文档 -> 重写为 strict PRD -> 回写项目文档 -> 校验提交`。
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 专题迁移 | 需求/任务/依赖/状态/测试层级 | 逐篇重写并校验 | `draft -> active -> done` | 以原文约束点映射为主线 | 维护者写入，复核者抽检 |
- Architecture Overview: 保持原文技术边界，按 strict PRD 结构重排。
- Edge Cases & Error Handling: 命名不一致、章节缺失、引用断链需在同提交修复。
- Non-Functional Requirements: PRD-ID/任务映射完整；治理检查通过。

原迁移验证行（仅文档结构，不代签业务验收）：
| PRD-P2P-MIG-098-001 | T0~Tn | `test_tier_required` | 文档治理检查 + 章节完整性核验 | 专题文档可维护性 |

原迁移决定：
| DEC-PRD-P2P-MIG-098-001 | 逐篇阅读后人工重写 | 直接重命名 | 保证语义保真和可审计性。 |
