# P2P / 区块链签名策略与可恢复性硬化设计

- 对应需求文档：`doc/p2p/blockchain/p2p-blockchain-p2pfs-hardening.prd.md`
- 对应GitHub Issue/Project task truth：GitHub Issue / GitHub Project

## 设计定位

本设计把 Phase 2–8 的分阶段 hardening 收为三个运行面：Node PoS state recovery、Action/Head signer policy、membership signer/keyring/recovery policy。当前代码与测试是行为真值；历史阶段文字只保留 provenance，不可覆盖当前 fail-closed 恢复合同。

2026-02-23 的 generic replication security hardening 同样已吸收：guard 在验证和写盘成功前不提交；writer/fetch 鉴权、bounded subscription 与签名优先 membership restore 遵循 PRD 的当前合同。该吸收不将这些局部防护提升为 production custody 或 mainnet readiness。

## 结构与边界

- 恢复层：仅缺失状态可默认初始化；有效陈旧同 world 状态可由 execution latest 前推；损坏、不可读或不可解析状态必须停止并暴露错误。
- Action/Head 层：HMAC 与 ed25519 可双栈存在；ed25519 使用 canonical 格式和 payload；强制动作签名需要 HMAC 或 allowlist 支撑。
- Membership 层：keyring 处理 active/multi-key、key-ID 和 revoked key；allowlist 仅在非空时强制 ed25519 signer 匹配，且不绕过 keyring、key-ID、revocation 或 requester policy。
- 公钥策略层：共享 32-byte hex 规范化、小写比较、重复拒绝和字段级错误，以避免 membership 与 sequencer 漂移。
- Replication guard 层：拒绝空 world/writer 和零 epoch/sequence；同 writer/epoch 的 sequence 严格递增，epoch 或 writer 切换从 sequence 1 开始且不允许 epoch 回退。
- Replay 层：逐条调用与在线路径相同的 `apply_replication_record`；只有 hash 校验和 store 写入成功后才提交 guard，首个失败立即终止并保留失败前的确定状态。
- Commit/execution 层：ordered actions 与 `action_root` 经 commit context 进入 node execution；result 的 block/state root 和持久 snapshot 字段用于恢复兼容。旧 viewer-live driver 与 bridge-default 接线只作历史 provenance。
- Block hash 层：execution bridge 对 canonical-CBOR `oasis7_proto::distributed::WorldBlock` 计算 commitment；现行字段以该类型的 `world_id/height/prev_block_hash/action_root/event_root/state_root/journal_ref/snapshot_ref/receipts_root/proposer_id/timestamp_ms/signature` 为准，旧路线图不能反向新增另一套 payload schema。
- DistFS proof 层：当前 gate 使用 `ReplicationNetworkEndpoint` 做 provider/DHT/fetch-route 探测，并区分 hard failure 与有界 degraded/fallback；历史 Phase C 的专用 request/proof envelope、challenge topics 与 challenge driver 未形成当前合同。

## 约束

- 不改变 `signature` 字段或新增 wire schema；不把兼容窗口误写为算法/协议切换完成。
- 不把 restore/restart 当成对损坏或分歧的自动修复。运维动作必须先保全证据并定位 code/config/deployment 根因。
- 本设计不提供生产 custody、拓扑、部署、rollback 或 release pass；这些分别由现行 runbook、证据与对应专业角色收口。

## 验证入口

- `crates/oasis7_chain_runtime/src/host/startup_reconcile.rs`
- `crates/oasis7_consensus/src/signature.rs`
- `crates/oasis7_consensus/src/ed25519_signer_policy.rs`
- `crates/oasis7_consensus/src/membership_logic.rs`
- `crates/oasis7_consensus/src/sequencer_mainloop.rs`
- `crates/oasis7_distfs/src/replication.rs`

## 历史演进

`MIG-046` 至 `MIG-052` 的详细阶段、决策和完成日期由同名 PRD 的交叉表保存。Phase 8 的共享化取代了 Phase 6/7 的重复实现，不删除其兼容边界或历史验证证据。

`MIG-058`、`MIG-064` 与 `MIG-065` 分别保存 production-grade 路线、Phase B commit-execution 与 Phase C challenge-network 的压缩历史；Phase C 是未形成当前能力的历史目标，而非已上线网络证明。

## 审读基线与当前差距

- Owner role：`runtime_engineer`、`blockchain_ops_engineer`；验证由 `qa_engineer` 独立核对。
- 本次文档/源码审读基线：`4e94163a3dff9ba69f6959b406ca3fcedf8aa097`，日期 `2026-10-15`。当前实现说明与历史阶段记录分别保留；本次不执行签名、部署、恢复或构建，不推导新的运行完成态。
- 产品关系只承接下表明确列出的局部义务；功能整体仍为部分覆盖，现有产品实现状态与原验收层级不变。

## 问题、目标与上游分工

目标是在现有 signature wire 兼容下维护签名、复制与恢复的拒绝边界。专业 PRD §3/5 拥有策略与验证要求；runtime 拥有状态恢复实现；共识模块拥有 signer/keyring；blockchain ops 与 QA 拥有真实恢复 drill 与证据判定。本设计不新增生产身份治理或网络协议。

<a id="hardening-recovery-continuity"></a>
## 恢复流程与状态持久化

本节承接 DCS-002 的本地 Node PoS startup reconciliation 子范围。恢复先核对 execution latest 的 world 与完整 execution binding；只有缺失状态可初始化，有效陈旧同 world 状态可向前更新，损坏/不可读/不可解析状态保持 fail closed。高度不能回退，失败不伪造已恢复。完整 genesis/profile/epoch 激活链、checkpoint/CAS、非空日志正向续跑与重新可服务仍由 P2P/runtime 恢复主合同承担；本地状态文件前推不证明端到端恢复。

<a id="hardening-signature-boundary"></a>
## 签名接口、事务与隔离边界

承接配对 PRD §3 全部运行合同：Action/Head 保留 HMAC 与 ed25519 兼容；require_action_signature 配置必须有 HMAC 或非空 allowlist，公钥按同一规范化/去重规则核对。Membership restore 的任一认证约束（require-signature、require-key-ID、accepted key-ID/public-key allowlist）均要求实际 signature 与适用 signer/keyring，缺失或验签失败在 apply 前拒绝并记录 Rejected，QuorumConsensus 不变。公钥匹配只是认证的附加条件，不代替 keyring、key-ID、revocation 或 requester 校验；默认无约束且无 verifier 只保留显式非认证诊断兼容，不构成正式 BFT authority。复制 guard 只有 hash 校验与 store 写入成功后才提交，replay 使用同一在线路径并在首个失败处停止。远程 writer/fetch 鉴权与 bounded subscription/outbox 不得因恢复或降级跳过。

## 兼容、部署与剩余风险

原有 signature 字段、canonical CBOR 与 WorldBlock 类型继续拥有格式真值，不新增 schema。当前 DistFS route probe 的 hard failure/degraded 边界不表示 Phase C 专用 challenge 协议已经存在。运维先保全损坏/冲突证据再定位，禁止用重启或默认覆盖伪装恢复。全链 BFT、真实故障域、custody 与 mainnet readiness 都未由本专题证明。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)；Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4 | 本地恢复保留同 world、完整 execution binding、向前且 fail-closed 的条件。 | [恢复连续性](#hardening-recovery-continuity) | runtime / blockchain ops / QA | 只覆盖 Node PoS reconciliation；合法 profile 链、checkpoint/replay 与恢复后正向续跑未覆盖。 |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | 保持签名兼容、组合信任检查、guard 写后提交、按序 replay 与请求鉴权。 | [签名与复制边界](#hardening-signature-boundary) | consensus / DistFS / runtime | 仅通过专业 PRD §3 的签名与复制准入支撑角色隔离；不证明完整服务暴露面、validator activation、custody 或主网可用。 |

### 11.1 验证映射表

以下是验证要求与已有入口的对应，不是本次执行结果。源码/测试定义只提供静态核对；实际验收须在同候选、适用 feature/config 与环境下运行，并保留命令、提交、输出、失败样例和未覆盖范围。文档检查通过不证明运行、部署或 release readiness。

| 上游 requirement / acceptance | 本设计条款 | 验证目标 / oracle | 验证方法 / 精确来源 | 证据产物 | 未覆盖边界 / residual risk |
| --- | --- | --- | --- | --- | --- |
| [REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)；Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4 | [恢复连续性](#hardening-recovery-continuity) | 成功只更新可验证前向状态；无可用 latest 不伪造状态，损坏失败。 | 按 [testing manual](../../../testing-manual.md) 的 Rust 验证入口执行 `env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime startup_reconcile -- --nocapture`，已有断言覆盖陈旧同 world、异 world 与缺 node binding；缺失/损坏/不可读输入仍需补同候选负向证据；完整 DCS-002 另做 full 组合。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 只覆盖 Node PoS reconciliation；合法 profile 链、checkpoint/replay 与恢复后正向续跑未覆盖。 |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | [签名与复制边界](#hardening-signature-boundary) | 无 verifier/非法 allowlist/错误签名被拒绝；写入失败 guard 不推进，后续记录不越过失败。 | 依 [testing manual](../../../testing-manual.md) 分别执行 `env -u RUSTC_WRAPPER cargo test -p oasis7_consensus --lib signature`、`env -u RUSTC_WRAPPER cargo test -p oasis7_consensus --lib membership` 与 `env -u RUSTC_WRAPPER cargo test -p oasis7_distfs replication`；记录实际选中测试，分别核对 HMAC/ed25519、allowlist/keyring 与 guard/replay，不能仅由过滤名推断覆盖。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 仅通过专业 PRD §3 的签名与复制准入支撑角色隔离；不证明完整服务暴露面、validator activation、custody 或主网可用。 |

## 精确源码与断言定位

- [startup reconcile](../../../crates/oasis7_chain_runtime/src/host/startup_reconcile.rs)：`startup_reconcile_advances_stale_pos_state_from_execution_latest`、`startup_reconcile_skips_legacy_latest_without_node_block_hash`、`startup_reconcile_ignores_foreign_world_latest`。缺失/损坏/不可读状态的完整运行证据仍缺，不能用这三个断言代签。
- [signature](../../../crates/oasis7_consensus/src/signature.rs)：HMAC/ed25519 action/head roundtrip 与 tampered action；[公钥规范化](../../../crates/oasis7_consensus/src/ed25519_signer_policy.rs) 的重复规范化公钥拒绝；[membership](../../../crates/oasis7_consensus/src/membership_tests.rs) 的 keyring roundtrip、unsigned default restore、unaccepted public key、HMAC-with-required-public-key 拒绝；[restore authentication matrix](../../../crates/oasis7_consensus/src/membership_restore_auth_tests.rs) 的 `membership_restore_authentication_policy_matrix` 组合检查四种认证约束、签名/篡改、缺 verifier/有效 signer/有效或空 keyring，并核对拒绝 audit 与 membership/history 不变。
- [replication](../../../crates/oasis7_distfs/src/replication.rs)：`apply_replication_record_hash_mismatch_does_not_mutate_guard`、`apply_replication_record_write_failure_does_not_mutate_guard`、`replay_replication_records_restores_files_in_order`，以及 epoch/sequence/writer 切换断言。fetch 鉴权、网络饱和与真实故障恢复需要各自额外证据，不由这些局部断言证明。
