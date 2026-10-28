# P2P 移动轻客户端权威状态设计

- 对应需求文档: `doc/p2p/network/p2p-mobile-light-client-authoritative-state.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project


## 目标、上游约束与适用范围

目标是让移动客户端仅提交 intent、消费可验证的权威结果，并在挑战、重组或换钥时保持拒绝与恢复边界。专业规则来自配对 [PRD 专业合同](p2p-mobile-light-client-authoritative-state.prd.md#专业合同)。runtime/P2P 拥有执行与最终性，gateway 拥有会话入口，Viewer/移动客户端消费状态，QA 核对同候选组合证据。

当前正文是协议与恢复合同，不是移动公开交付或生产运行报告；下列序列化回归仅定位可复用的验证子范围。未核实的行为测试与部署证据仍是缺口。本轮整理不修改 wire schema、状态机或权限规则，也不替代 PRD 的完整验证责任。

## 分层与数据流

1. 移动客户端持有短期 session key，提交带签名的 intent；gateway/relay 负责鉴权、限流与重放保护，但不成为权威状态来源。
2. sequencer 依 tick/seq 排序，authoritative simulator 产出 delta、`state_root` 与 `data_root`；客户端只渲染、插值预测和按权威结果纠偏。
3. 批次承诺进入 `pending`，在满足确认条件且无未解决 challenge 时进入 `confirmed`；最终化前保留 watcher 复算、challenge 与 resolve 的 fail-closed 分支。
4. 断线客户端以最近稳定 `snapshot_hash` 和 `log_cursor` 追平。hash/cursor 不连续或 reorg epoch 变化时，丢弃分叉视图，回退 stable batch 后重新取得快照与增量。
5. session revoke/rotate 改变写入授权；旧 key 的所有 intent/控制请求在 gateway 与权威入口均被拒绝。

## 状态与故障边界

- `pending/confirmed/final` 的展示和消费必须绑定同一批次状态；任何缺失的根、签名、challenge 结果或恢复证据都保持非 final，而不是乐观升级。
- challenge、resolve 和处罚语义由对应 runtime/chain authority 实现；本设计只要求移动路径按其结果 fail closed，不另造仲裁或 finality 真值。
- snapshot 或日志损坏是恢复失败信号，必须进入重新获取/人工诊断路径；不能以进程重启、复用旧 cursor 或忽略 hash mismatch 伪造追平成功。
- session key、node identity、consensus/finality signer 和 governance signer 必须保持逻辑分离。移动端或 relay 的 session key 不得取得节点治理或出块权。

## 运维与观测边界

- 可观测项至少包括 intent reject/idempotent 结果、批次根校验、challenge 状态、reorg epoch、snapshot/cursor 校验与 session revoke/rotate 拒绝原因；它们是诊断输入，不能单独推出网络健康或发布就绪。
- 真实节点、拓扑、服务合同、状态采样、restore/rollback drill 与 rollout 由 P2P node/network runbook、runtime 和 ops evidence 维护。本专题不定义生产 inventory、主备切换、SLA 或恢复演练已完成。

## 需求承接与验证范围

承接表只列上述既有设计实际承担的子义务，不改变产品功能状态，也不宣称覆盖产品要求的全部验收。验证表是方法与证据目标，不是本次运行结果。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001)；Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39 | 客户端不得把缺根、缺签名或 challenge 未关闭的批次升级为 final；非 final 不得供资产结算或排行 | [设计条款](#状态与故障边界) | runtime/chain finality 与 challenge authority；客户端消费同一批次状态 | 不证明当前移动路径完成 finality、仲裁或结算集成 |
| [REQ-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001)；Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39 | gateway/relay 不成为权威；challenge/resolve 阻断错误根的最终化并按权威结果反馈 | [设计条款](#分层与数据流) | gateway、sequencer、simulator、watcher 与 chain authority | 不证明根重算、处罚和重复 challenge/resolve 的运行时幂等实现 |
| [REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)；Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4 | 重组或损坏恢复必须核对 snapshot/cursor 并回退稳定批次；观测和重启不能代签恢复成功 | [设计条款](#分层与数据流) | runtime 恢复 authority；blockchain ops 与 QA 提供恢复窗口 | 不覆盖移动专有 cursor、session revoke/rotate 或断线恢复的完整行为验证 |

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001) | [设计条款](#状态与故障边界) | 客户端不得把缺根、缺签名或 challenge 未关闭的批次升级为 final；非 final 不得供资产结算或排行 | [定向测试选择](../../../testing-manual.md#testing-default-selection-tree)；源码 `crates/oasis7_proto/src/viewer/authoritative_batch_roundtrip_tests.rs` 的 `viewer_response_round_trip_authoritative_batch`：同候选 proto 单元回归核对 confirmed、根、challenge_open 与 settlement/ranking flags 往返；不能替代行为拒绝测试 | 单元测试输出；finality 拒绝与结算组合证据仍须由 runtime/QA 提供 | 不证明当前移动路径完成 finality、仲裁或结算集成；本轮未运行行为测试 |
| [REQ-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001) | [设计条款](#分层与数据流) | gateway/relay 不成为权威；challenge/resolve 阻断错误根的最终化并按权威结果反馈 | [定向测试选择](../../../testing-manual.md#testing-default-selection-tree)；源码 `crates/oasis7_proto/src/viewer/authoritative_challenge_roundtrip_tests.rs` 的 `viewer_response_round_trip_authoritative_challenge_ack`：同候选 proto 单元回归核对 challenge/batch/status/slash 字段往返；实际仲裁分支另验 | 单元测试输出；实际 challenge/recompute/resolve 证据未核实 | 不证明根重算、处罚和重复 challenge/resolve 的运行时幂等实现；本轮未运行行为测试 |
| [REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) | [设计条款](#分层与数据流) | 重组或损坏恢复必须核对 snapshot/cursor 并回退稳定批次；观测和重启不能代签恢复成功 | [testing-manual S9](../../../testing-manual.md#testing-p2p-storage-consensus-long-running)；按 S9 漂移定位/回滚演练运行 `runtime_authoritative_recovery_rollback` 定向测试，核对回滚授权拒绝不改变 world/journal/batch/reorg epoch；只覆盖共享恢复子范围 | 回滚定向测试输出与实际候选/环境；移动 snapshot/cursor 组合证据仍缺 | 不覆盖移动专有 cursor、session revoke/rotate 或断线恢复的完整行为验证；本轮未运行行为测试 |

## 决策、风险与未决边界

客户端保持轻量，签名 intent 与权威 delta 分离，避免本地预测成为世界真值；这依赖权威根、挑战结果、稳定快照与当前有效 session key 均可验证。proto 往返只能证明字段保真，不能证明签名、nonce、幂等、challenge 状态迁移或 session 吊销生效。runtime/gateway owner 在这些入口变化时须补齐配对 PRD 的行为验证；QA 在声称移动交付前核对 reorg、snapshot/cursor 和 revoke/rotate 的同候选证据。缺证时继续使用本 PRD 的非承诺边界，不把通用回滚或协议往返扩成移动端到端通过。
