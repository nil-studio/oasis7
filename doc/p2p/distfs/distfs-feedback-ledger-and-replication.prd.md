# DistFS 反馈账本与复制

- 对应设计文档: `doc/p2p/distfs/distfs-feedback-ledger-and-replication.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

## 目标

定义公开反馈账本及其有界 P2P 复制的当前专业合同：反馈以 append-only 根记录和事件保存，作者签名控制 create/append/tombstone；节点以轻量 announce 提示、按 content hash 拉取 blob 并在校验后复制入库。

本文件收敛三个 2026-03 feedback 源专题为当前入口。源三件套已在语义回填和活跃引用修复后删除；完成态与审计 provenance 仅保留在 Git 与 `.pm` task evidence，且不再作为 active authority。

## 范围

本专题覆盖 feedback store、签名与基础 anti-abuse、announce/fetch 复制及其受限 NodeRuntime 接线；不改变共识、finality、state-sync 或恢复机制。

<a id="feedback-interface-data"></a>
## 接口 / 数据

- 账本布局为 `feedback/records/<feedback_id>/root.json`、`events/<event_id>.json`、`feedback/nonces/<pubkey>/<nonce>.json` 和 `feedback/audit/<audit_id>.json`。root 和 event 只追加；删除写 tombstone 事件，不物理删除历史记录。
- create、append 与 tombstone 使用 Ed25519 签名；canonical payload 绑定 action、feedback ID、内容或 reason hash、nonce、timestamp 和 expiry。过期、重复 nonce 或无法验证的签名必须拒绝。
- 公开读视图可列举或取得记录，并对 tombstone 返回 tombstoned 状态和原因；本专题不提供分级读取。
- 审计记录是基础 IP/public-key 时间窗限流的来源，并约束内容、附件数量和附件大小。复制入库保留签名和 append-only 校验，但不把本地提交 IP/pubkey 限流误用于远端已验证记录。
- announce topic 为 `aw.<world_id>.feedback.announce`。announce 只含版本、world、feedback/event/action、actor、blob reference 与发出时间；blob reference 包含路径、BLAKE3 content hash 和大小。
- ingest 必须先 fetch blob，再验证 BLAKE3 hash，随后按 create 或 append/tombstone 解析并复制入库。`feedback_id + event_id` 重复 announce 必须幂等。
- 启用 `feedback_p2p` 的节点必须同时具备 replication config、replication network 和 BlobState lane 的 publish+subscribe 权限；不满足时启动拒绝，而不是静默退化为局部复制。
- `oasis7_chain_runtime` 二进制必须在 `runtime.start()` 前挂载默认 replication network 与 maintenance DHT handle。默认 listen 是 loopback ephemeral 地址；有效 bootstrap-peer 列表为空时，才允许 no-peer 本地 handler fallback 维持单机开发/测试闭环。
- 有效 bootstrap peer 可来自显式 CLI 或 network-tier manifest。只要有效列表非空，就必须禁用本地 fallback；没有 connected/admissible peer 时返回可诊断的 `NetworkProtocolUnavailable`，不得把显式多节点拓扑静默处理为单机成功。
- NodeRuntime 每 tick 按配置上限 drain 入站 announce 和发布本地 outbox；fetch 或单条 ingest/publish 失败记录为局部 runtime/replication 错误，不应阻断主 tick。outbox 有界，饱和时提交失败而不无限积压。

## 里程碑

- M1：账本、签名、nonce、audit 和公开读取的历史完成范围保留并成为当前合同。
- M2：announce/blob hash/fetch/replicated ingest 的历史完成范围收敛为当前复制合同。
- M3：NodeRuntime 配置、lane 前提和每 tick 有界接线纳入当前专业 authority；后续变更以本三件套为首读入口。

## 风险与非就绪边界

- 不把反馈事件绑定到共识、finality、state sync 或任何链上执行收据；该复制路径是最终一致的存储层能力，不保证实时全节点一致。
- 不承诺生产拓扑、公开服务、内容审核、隐私分级、查询 HTTP 网关、多源调度、SLA、灾备恢复或 release/readiness。反馈复制通过的测试不能单独升级为 DistFS、网络或公共测试网就绪结论。
- 默认 loopback/no-peer fallback 只证明本地 wiring；它不是远端 replication、peer reachability、state-sync、checkpoint/restore 或真实网络恢复证据。
- replication fetch 的签名、allowlist 和网络权限仍由 replication/network 合同拥有；本专题不另造鉴权真值。公开可读不等于公开可写、匿名可写或适合承载敏感内容。

## 验证与追溯

- 回归至少覆盖：签名/expiry/nonce、append/tombstone 和公开读取、内容 hash、重复 announce 幂等、缺 replication 或 lane 权限拒绝启动、默认 network 在 start 前挂载、effective-no-bootstrap 才允许本地 fallback、显式 topology 无 peer 时 fail closed、每 tick 上限/队列饱和，以及单条失败不阻断 tick。
- 当前实现入口：`crates/oasis7_distfs/src/{feedback.rs,feedback/replication.rs,feedback_p2p.rs}` 与 `crates/oasis7_node/src/{feedback_runtime.rs,node_runtime_core.rs,lib.rs,types.rs}`。
- 具体历史完成范围、测试入口和 provenance 见GitHub task issue evidence；当前修改应同时评估 `oasis7_distfs` 与 `oasis7_node` 的受影响回归，不能把模块回归当成发布判定。
