# P2P 移动轻客户端权威状态

- 对应设计文档: `doc/p2p/network/p2p-mobile-light-client-authoritative-state.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

## 目标与 authority

本三件套是移动轻客户端的当前专业 authority：客户端仅提交签名 intent 并消费权威 delta；权威执行批次以 `state_root`/`data_root` 承诺，并以 challenge、reorg recovery 与 session-key 生命周期约束可见状态。

它收敛 2026-03-06 日期型三件套的稳定语义。旧文件已退役；完成记录从 Git 与 GitHub task evidence 追溯，不得以历史完成叙述推导当前部署、QA 放行、公开可用性、finality 或 release 结论。

## 专业合同

- 客户端只发送 `intent(player_id, session_pubkey, tick, seq, action, payload_hash, sig)`，不上传权威位置、血量或世界状态，也不承担本地权威模拟。`(player_id, agent_id, seq)` 的同载荷重试可返回幂等 ACK；同序号不同载荷必须拒绝，且 `seq` 与签名 nonce 一致。
- 权威批次至少绑定 `batch_id/state_root/data_root`。缺字段、格式非法或与本地批次根不一致时，批次不得被确认。权威 delta 使用 `from_tick/to_tick/batch_id/patches/state_root/authority_sig`，只接受受信权威签名与单调 tick 范围。
- 批次状态为 `pending -> confirmed -> final`：`pending` 尚未达到 confirm 条件；`confirmed` 已达到确认条件且未被 challenge 阻断；只有达到 final 条件且挑战窗口关闭时才可标为 `final`。非 `final` 数据不得驱动资产结算或排行；链重组是唯一允许撤回已见状态的恢复路径。
- challenge 使用 `challenge_id/batch_id/recomputed_state_root/recomputed_data_root/slash_record`。窗口内的有效 challenge 进入 `challenged -> resolved`；根不一致必须阻断 `final` 并留下仲裁/处罚记录，根一致不得处罚。重复 challenge 或 resolve 必须幂等拒绝。
- 恢复请求携带 `snapshot_hash/log_cursor/stable_batch_id/reorg_epoch`。游标不连续、快照 hash 校验失败或发生 reorg 时，必须回退至最近稳定批次、重建 cursor 并强制重拉可验证快照；不得沿被重组分叉继续确认。
- session key 以 `player_id/session_pubkey/session_epoch/revoked_at_tick/replaced_by_pubkey/revoke_reason` 管理。session epoch 单调；被吊销 key 不得再次激活，吊销后旧 key 的 intent 与控制请求必须拒绝，换钥后仅当前有效 key 可写入。

## 接口 / 数据

本专题的稳定接口为签名 intent、权威 batch/delta、challenge/resolve、snapshot recovery 与 session revoke/rotate；字段、状态转换和权限边界以“专业合同”所列为准。具体 wire encoding、runtime handler 与客户端投影仍由各自专业 authority 维护。

## 里程碑

- M1：将日期型专题的稳定协议、状态和恢复语义收敛为本三件套的当前入口。
- M2：后续变更持续以本合同核对 intent、根绑定、challenge、恢复与会话权限边界，并取得相应实现证据。
- M3（已完成）：引用审计与 deletion-readiness 已完成，日期型源三件套已退役；本稳定 authority 不以历史迁移替代任何实现或发布门槛。

## 风险

- 未经当前实现、测试和运行面证据验证的合同，不能被解释为已交付移动功能、网络 finality 或发布就绪。
- reorg、快照可用性、challenge 仲裁和 session-key 管理跨越 runtime、客户端与运维边界；任一层的缺口都可能使恢复或状态展示不成立。

## 范围与非承诺

本专题定义协议、状态和恢复边界；不取代 runtime、共识、链上仲裁、客户端 UI、部署或测试证据的专业 authority。它不承诺移动端公开提供、持续可用、任何吞吐/时延 SLA、全量轻客户端安全、mainnet-grade finality、任意状态证明、公开 validator 准入、release/readiness 或当前生产完成态。`final` 是本合同内、证据充分时才可显示的状态，不是对外网络 finality 或结算可用性的独立宣告。

## 验证责任

- 协议与 runtime 变更至少验证 intent 签名/nonce/幂等、批次根绑定、finality 单调、challenge/resolve 分支、reorg 回退、快照校验及 session revoke/rotate。
- 实现、测试命令与历史完成范围以GitHub task issue evidence、GitHub task evidence 和对应 runtime/viewer/QA 证据为准；文档合同本身不替代这些证据。
- 涉及实际网络、恢复演练或发布判断时，必须分别取得 runtime、blockchain ops 与 QA 的当前证据；不得以重启替代配置、实现或部署根因修复。
