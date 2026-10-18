# oasis7 主链 Token 分配与发行机制（已实现）设计

- 对应需求文档: `doc/p2p/token/mainchain-token-allocation-mechanism.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

## 1. 设计定位
定义 oasis7 主链 Token 分配与发行机制主设计，统一总量、分配对象、发行节奏与治理约束。

## 2. 设计结构
- 发行模型层：定义 token 总量、发行节奏与基础约束。
- 分配策略层：明确节点、治理、生态等对象的分配规则，以及 node-to-main-token 内部地址绑定。
- 执行落账层：把分配结果映射到主链状态与发放流程；二期 treasury 分发只允许 staking/ecosystem/security 三个 bucket。
- 治理审计层：策略更新和 treasury 分发必须绑定 Approved/Applied proposal；以 `distribution_id` 记录幂等审计。

## 3. 关键接口 / 入口
- token 发行模型
- 分配对象与比例
- `World::node_main_token_account` / `World::bind_node_main_token_account`
- `Action::DistributeMainTokenTreasury` 与 `DomainEvent::MainTokenTreasuryDistributed`
- `main_token_treasury_distribution_records` 分配审计记录

## 4. 约束与边界
- 总量与分配比例必须可追溯、可审计。
- 分配执行不得绕过治理约束；策略更新与 treasury 分发均要求已批准或已应用 proposal。
- distribution 会改变 recipient liquid balance 与 circulating supply，但不改变 total supply。
- 地址绑定是 runtime 内部字符串模型，不扩展为外部钱包/跨链协议，也不改变经济参数。

## 5. 设计演进计划
- 先冻结主链 token 主模型。
- 再细化受治理的地址绑定和 treasury 分发执行路径。
- 保持历史二期实施过程在 Git history 与 GitHub task evidence 可追溯。

## 追踪范围与未验收风险

本次只追踪配对 PRD 已有的冻结专业合同，不重开经济或安全决定，不改变原有状态描述。源码审读固定 main `5b743ca0c3269eb39dffbdb5ffc4329b87984a69`；静态定义与已有测试断言仅是验证定位，本次未执行 Rust、UI、签名、真实交易、费用、部署或恢复测试。共享 runtime、publication、persistence、launcher 与 CI 在途变更不作为已验收证据；原文“已实现/complete”不因本次追踪新增而获得新验证。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [主配置与边界](mainchain-token-allocation-mechanism.prd.md#tam-config) | 原配置边界与 issuance_split 总和 10000 bps | [原配置边界与 issuance_split 总和 10000 bps](#tam-config-design) | Runtime Token 配置校验主责；`runtime/main_token.rs::validate_main_token_config_bounds` | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=原配置边界与 issuance_split 总和 10000 bps，不覆盖端到端产品验收 |
| [治理更新延迟](mainchain-token-allocation-mechanism.prd.md#tam-policy-delay) | 策略更新按既有两 epoch delay 生效 | [策略更新按既有两 epoch delay 生效](#tam-delay-design) | Runtime Token policy 调度主责；`runtime/world/event_processing/main_token.rs::evaluate_update_main_token_policy_action` 与 `MAIN_TOKEN_POLICY_UPDATE_DELAY_EPOCHS` | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=策略更新按既有两 epoch delay 生效，不覆盖端到端产品验收 |
| [策略批准条件](mainchain-token-allocation-mechanism.prd.md#tam-policy-approval) | UpdateMainTokenPolicy 仅接受 Approved/Applied proposal | [UpdateMainTokenPolicy 仅接受 Approved/Applied proposal](#tam-approval-design) | Runtime Token policy / governance proposal 状态主责；`runtime/world/event_processing/main_token.rs::evaluate_update_main_token_policy_action` | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=UpdateMainTokenPolicy 仅接受 Approved/Applied proposal，不覆盖端到端产品验收 |
| [三 bucket 分发](mainchain-token-allocation-mechanism.prd.md#tam-distribution) | treasury 分发限 staking_reward_pool/ecosystem_pool/security_reserve | [treasury 分发限 staking_reward_pool/ecosystem_pool/security_reserve](#tam-buckets-design) | Runtime treasury action / monetary publication 主责；`runtime/world/event_processing/action_to_event_core_main_token.rs::evaluate_distribute_main_token_treasury_action` | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=treasury 分发限 staking_reward_pool/ecosystem_pool/security_reserve，不覆盖端到端产品验收 |
| [distribution_id 审计幂等](mainchain-token-allocation-mechanism.prd.md#tam-distribution) | distribution_id 幂等及审计记录/事件 | [distribution_id 幂等及审计记录/事件](#tam-idempotency-design) | Runtime treasury action / distribution record 主责；同一 `evaluate_distribute_main_token_treasury_action` 拒绝已用 ID，落账恢复由 publication 主责另验 | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=distribution_id 幂等及审计记录/事件，不覆盖端到端产品验收 |
| [节点内部账户绑定](mainchain-token-allocation-mechanism.prd.md#tam-internal-binding) | NodePoints 桥接优先 node_main_token_account_bindings | [NodePoints 桥接优先 node_main_token_account_bindings](#tam-binding-design) | Runtime NodePoints settlement / Token account binding 主责；`runtime/world/event_processing/main_token.rs::resolve_main_token_bridge_account_id_for_node` | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=NodePoints 桥接优先 node_main_token_account_bindings，不覆盖端到端产品验收 |
| [二期快照默认兼容](mainchain-token-allocation-mechanism.prd.md#tam-snapshot-defaults) | 缺少二期字段时 serde(default) | [缺少二期字段时 serde(default)](#tam-defaults-design) | Runtime state schema / snapshot 主责；`runtime/state.rs` 二期字段默认与 `runtime/tests/main_token_core_tests.rs` 的 Token snapshot 验证 | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=缺少二期字段时 serde(default)，不覆盖端到端产品验收 |

## 冻结设计条款与验证边界

<a id="tam-config-design"></a>
### 原配置边界与 issuance_split 总和 10000 bps

承接 [主配置与边界](mainchain-token-allocation-mechanism.prd.md#tam-config)。保留 burn 上限、rate 次序、epochs_per_year>0、max_supply 与 initial_supply 边界；不选取新参数。验证限界：`main_token_policy_update_rejects_out_of_bounds_configuration` 断言 split sum must be 10000 且 scheduled queue 为空；其余配置边界仅静态核对定义，未逐项补证。

<a id="tam-delay-design"></a>
### 策略更新按既有两 epoch delay 生效

承接 [治理更新延迟](mainchain-token-allocation-mechanism.prd.md#tam-policy-delay)。目标 epoch 有效配置计算不变；不签收共享 publication/recovery 的当前候选。验证限界：`main_token_policy_update_is_delayed_and_audited_before_it_affects_issuance` 断 effective_epoch > base，并比较生效前后 rate；恰为2须以常量/调度实现静态核对，测试名不替代断言。

<a id="tam-approval-design"></a>
### UpdateMainTokenPolicy 仅接受 Approved/Applied proposal

承接 [策略批准条件](mainchain-token-allocation-mechanism.prd.md#tam-policy-approval)。保留 proposal 存在、参数边界与批准状态，不增加签名主体或治理权限。验证限界：`main_token_policy_update_requires_approved_or_applied_governance_proposal` 断 missing/unapproved/Approved 分支；Applied 正例不由测试名推出，须独立补证。

<a id="tam-buckets-design"></a>
### treasury 分发限 staking_reward_pool/ecosystem_pool/security_reserve

承接 [三 bucket 分发](mainchain-token-allocation-mechanism.prd.md#tam-distribution)。保留既有治理批准、余额与供应变动语义；不扩展 node_service_reward_pool 或其他 bucket。验证限界：`main_token_treasury_distribution_applies_closed_loop_and_records_audit` 与 `main_token_treasury_distribution_rejects_unsupported_bucket_and_duplicate_distribution_id` 的对应断言；total_supply 不变依原合同及静态更新路径，不能称已有测试单独断言。

<a id="tam-idempotency-design"></a>
### distribution_id 幂等及审计记录/事件

承接 [distribution_id 审计幂等](mainchain-token-allocation-mechanism.prd.md#tam-distribution)。既有拒绝重复记录；不据此签收所有拒绝路径、崩溃恢复或跨入口原子性。验证限界：`main_token_treasury_distribution_rejects_unsupported_bucket_and_duplicate_distribution_id` 断拒绝文本；不推出完整拒绝前后所有状态不变或重启恢复矩阵。

<a id="tam-binding-design"></a>
### NodePoints 桥接优先 node_main_token_account_bindings

承接 [节点内部账户绑定](mainchain-token-allocation-mechanism.prd.md#tam-internal-binding)。内部字符串绑定查询/写入；不升级为外部钱包、custody、跨链或公开领取协议。验证限界：`reward_asset_settlement_action_applies_signed_records_via_step` 断 node_main_token_account != node_id、映射账户 liquid=5、circulating=5 及 bridge record/event 账户一致；显式 override 优先级只核对 resolver 源码，未定位直接测试。

<a id="tam-defaults-design"></a>
### 缺少二期字段时 serde(default)

承接 [二期快照默认兼容](mainchain-token-allocation-mechanism.prd.md#tam-snapshot-defaults)。保留旧快照默认兼容合同，不推定完整 journal/checkpoint/publication 恢复正确。验证限界：`main_token_snapshot_roundtrip_persists_state` 仅检查选定现有状态的 roundtrip；`state.rs` 二期字段 serde(default) 是静态定义；缺少二期字段的 decode/默认值直接断言尚未定位，需该快照验证主责补证，不能以现有 roundtrip 或 era-fields 兼容测试代替。

### 11.1 验证映射表

| 上游 requirement / professional acceptance | 本设计条款 | 验证义务 | 验证 source / 方法 | 本次结果与证据边界 | 未覆盖范围 |
| --- | --- | --- | --- | --- | --- |
| [主配置与边界](mainchain-token-allocation-mechanism.prd.md#tam-config) | [原配置边界与 issuance_split 总和 10000 bps](#tam-config-design) | 原配置边界与 issuance_split 总和 10000 bps | [静态核对定位](../../../crates/oasis7_runtime/src/tests/main_token_governance_tests.rs)；`main_token_policy_update_rejects_out_of_bounds_configuration` 断言 split sum must be 10000 且 scheduled queue 为空；其余配置边界仅静态核对定义，未逐项补证 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
| [治理更新延迟](mainchain-token-allocation-mechanism.prd.md#tam-policy-delay) | [策略更新按既有两 epoch delay 生效](#tam-delay-design) | 策略更新按既有两 epoch delay 生效 | [静态核对定位](../../../crates/oasis7_runtime/src/tests/main_token_governance_tests.rs)；`main_token_policy_update_is_delayed_and_audited_before_it_affects_issuance` 断 effective_epoch > base，并比较生效前后 rate；恰为2须以常量/调度实现静态核对，测试名不替代断言 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
| [策略批准条件](mainchain-token-allocation-mechanism.prd.md#tam-policy-approval) | [UpdateMainTokenPolicy 仅接受 Approved/Applied proposal](#tam-approval-design) | UpdateMainTokenPolicy 仅接受 Approved/Applied proposal | [静态核对定位](../../../crates/oasis7_runtime/src/tests/main_token_governance_tests.rs)；`main_token_policy_update_requires_approved_or_applied_governance_proposal` 断 missing/unapproved/Approved 分支；Applied 正例不由测试名推出，须独立补证 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
| [三 bucket 分发](mainchain-token-allocation-mechanism.prd.md#tam-distribution) | [treasury 分发限 staking_reward_pool/ecosystem_pool/security_reserve](#tam-buckets-design) | treasury 分发限 staking_reward_pool/ecosystem_pool/security_reserve | [静态核对定位](../../../crates/oasis7_runtime/src/tests/main_token_governance_tests.rs)；`main_token_treasury_distribution_applies_closed_loop_and_records_audit` 与 `main_token_treasury_distribution_rejects_unsupported_bucket_and_duplicate_distribution_id` 的对应断言；total_supply 不变依原合同及静态更新路径，不能称已有测试单独断言 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
| [distribution_id 审计幂等](mainchain-token-allocation-mechanism.prd.md#tam-distribution) | [distribution_id 幂等及审计记录/事件](#tam-idempotency-design) | distribution_id 幂等及审计记录/事件 | [静态核对定位](../../../crates/oasis7_runtime/src/tests/main_token_governance_tests.rs)；`main_token_treasury_distribution_rejects_unsupported_bucket_and_duplicate_distribution_id` 断拒绝文本；不推出完整拒绝前后所有状态不变或重启恢复矩阵 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
| [节点内部账户绑定](mainchain-token-allocation-mechanism.prd.md#tam-internal-binding) | [NodePoints 桥接优先 node_main_token_account_bindings](#tam-binding-design) | NodePoints 桥接优先 node_main_token_account_bindings | [静态核对定位](../../../crates/oasis7_runtime/src/tests/reward_asset_settlement_action.rs)；`reward_asset_settlement_action_applies_signed_records_via_step` 断 node_main_token_account != node_id、映射账户 liquid=5、circulating=5 及 bridge record/event 账户一致；显式 override 优先级只核对 resolver 源码，未定位直接测试 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
| [二期快照默认兼容](mainchain-token-allocation-mechanism.prd.md#tam-snapshot-defaults) | [缺少二期字段时 serde(default)](#tam-defaults-design) | 缺少二期字段时 serde(default) | [静态核对定位](../../../crates/oasis7_runtime/src/tests/main_token_core_tests.rs)；`main_token_snapshot_roundtrip_persists_state` 仅检查选定现有状态的 roundtrip；`state.rs` 二期字段 serde(default) 是静态定义；缺少二期字段的 decode/默认值直接断言尚未定位，需该快照验证主责补证，不能以现有 roundtrip 或 era-fields 兼容测试代替 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
