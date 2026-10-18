# README 生产级缺口收口（二次）：默认 WASM 执行 + Replication RR + 分布式 Triad + 玩家节点身份（设计文档）

- 历史设计承接：[本文件的保留边界与现行去向](#historical-retention-and-routing)；原同名 design 仅作兼容跳转。
- 执行记录: Git、PR、实际 CI 和评审记录；Issue 与 Project 按需使用

审计轮次: 4

- 当前操作请使用本页“历史保留边界与现行去向”的专业入口；本文件不提供现行启动命令。

<a id="historical-retention-and-routing"></a>
## 历史保留边界与现行去向

本文件保留原专题的需求、数值、兼容和失败约束，不是当前节点部署或 Viewer 参数手册。
下方原有“目标/范围/接口/里程碑”均属于原方案窗口；不因其使用现在时就重新成为当前实施排期。
特别是 Viewer Live 的 `--topology`、`--node-*`、历史 UDP 自动学习和旧 Triad 操作不能按本文复制执行；
当前节点职责由 chain runtime 及其正式启动入口承担，具体参数回到现行专业文档及源码。

| 原内容 | 处置 | 现行专业去向／未接收范围 |
| --- | --- | --- |
| 共识算法与阈值归属 | 保留原要求；不重做旧迁移 | [共识整合](../../p2p/consensus/consensus-code-consolidation-to-oasis7-consensus.prd.md) |
| 复制请求/handler、重试与玩家节点身份 | 保留原合同与失败条件 | [身份与复制](../../p2p/node/node-identity-replication-contract.prd.md)；逐条机制验收仍须对应实现证据 |
| 多样本阈值、拓扑观测 | 保留阈值和原采样窗口；旧 Viewer CLI 不再适用 | [Triad 运维与观测](../../p2p/node/node-triad-operations-observability.prd.md)；采样不代签正式拓扑就绪 |
| DistFS 审计字段及恢复失败 | 原字段、失败和回退记录原样保留 | [DistFS 加固](../../p2p/distfs/distfs-production-hardening.prd.md)；未逐项接收者继续留在本文 |
| 默认 WASM、兼容与回归 | 不按旧 crate/版本重做一次性迁移 | [WASM 执行器](../../world-runtime/wasm/wasm-executor.prd.md) |

同名旧设计的独有约束在此接收：统一术语与主路径，不扩大对外承诺，不在本次文档治理中
重构实现；README 不承载实现级配置，也不增加拓扑角色。现行专业去向是阅读路由，
不是“所有历史要求均已实现或验收”的声明。缺少逐项接收或当前证据时继续保留，不取消目标。

长 PRD、原任务、数值与历史失败不删除；旧 design URL 暂留兼容。进一步删除须先核清全部引用、
配对、动态/外部分发与 corpus 消费者。原版本见
[固定 Git 来源](https://github.com/eng-cc/oasis7/blob/2adfc08706f1905c131adc0597a9ae5e99ceb2dd/doc/readme/production/readme-prod-gap1245-wasm-repl-topology-player.prd.md)。

## 目标
- 收口缺口 1：默认构建启用真实 WASM 执行路径（`wasmtime`），避免默认环境下模块执行退化为 `SandboxUnavailable`。
- 收口缺口 2：补齐 `oasis7_node` libp2p replication 的 request/handler 通道，实现双向请求-响应能力。
- 收口缺口 4：提供跨主机部署的 triad 拓扑模式，避免仅支持本机嵌入式三节点预设。
- 收口缺口 5：将玩家-节点身份模型下沉到 node/consensus 层，实现配置、动作、消息、快照的一致身份语义。

## 范围
- In scope
  - `crates/oasis7/Cargo.toml`
    - 默认 feature 启用 `wasmtime`。
  - `crates/oasis7_node/src/libp2p_replication_network.rs`
    - gossipsub + request_response 双通道行为。
    - `request` 与 `register_handler` 的真实实现与错误映射。
  - `crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs`
  - `crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs`
    - 新增跨主机 triad 拓扑模式（单进程单节点角色，三角色静态组网）。
    - 保持现有 `triad`（本机嵌入式）与 `single`（手工）兼容。
  - `crates/oasis7_node/src/types.rs`
  - `crates/oasis7_consensus/src/node_consensus_action.rs`
  - `crates/oasis7_node/src/gossip_udp.rs`
  - `crates/oasis7_node/src/lib.rs`
  - `crates/oasis7_viewer_runtime/src/viewer/live/consensus_bridge.rs`
    - 新增玩家身份字段与校验链路。
  - 覆盖以上变更的 required-tier 测试。
- Out of scope
  - Web 节点去中心化（wasm32 节点网络实现不在本轮范围）。
  - 共识算法升级（PoS 阈值与投票规则不重写）。
  - 自动服务发现与自动运维编排（仅静态地址配置）。

## 接口 / 数据
### 1) 默认 WASM 执行开启
- `oasis7` crate default features 改为包含 `wasmtime`。
- 保留 `--no-default-features` 的显式降级路径用于最小构建。

### 2) Replication request/handler 通道
- 为 `Libp2pReplicationNetwork` 引入 request-response 协议行为。
- 新增内部请求/响应载荷：
  - 请求：`protocol + payload`
  - 响应：`ok + payload + error`
- `request(protocol, payload)`：
  - 有可达 peer 时走远端请求；
  - 无 peer 时回退本地 handler（便于单机链路与测试）。
- `register_handler(protocol, handler)`：
  - 写入 handler 表，支持远端调用。

### 3) 分布式 triad 拓扑
- `--topology` 新增 `triad_distributed`。
- `triad_distributed` 语义：
  - 每个进程启动一个节点角色（`--node-role`）。
  - 三个角色节点 ID 固定派生：`<node-id>-sequencer|storage|observer`。
  - 通过显式 gossip 地址参数声明三角色地址，按角色自动计算 bind/peers。
  - validator 集合固定为 triad 三角色（34/33/33）。
  - 允许结合 `--node-repl-*` 挂接 libp2p replication。

### 4) 玩家节点身份模型
- `NodeConfig` 新增 `player_id`。
- `NodeSnapshot` 新增 `player_id`，用于观测与审计。
- `NodeConsensusAction` 新增 `submitter_player_id`，并纳入校验与 action root 计算。
- `NodeRuntime` 增加按玩家提交接口，提交时强校验与本节点 `player_id` 一致。
- gossip proposal/attestation/commit 消息新增 `player_id`，并在 ingest 过程中进行一致性校验。
- `NodePosConfig` 新增 validator 到 player 的绑定映射，要求 validator 与玩家一一绑定，禁止同玩家重复占位。

## 里程碑
- M1：T0 文档冻结（设计文档 + GitHub Issue/Project task truth）。
- M2：T1 默认 WASM 执行开启并通过 required-tier 相关测试。
- M3：T2 replication request/handler 实现与测试通过。
- M4：T3 分布式 triad 拓扑实现与 CLI/启动测试通过。
- M5：T4 玩家节点身份链路贯通（配置/动作/消息/快照）与测试通过。
- M6：T5 回归（`cargo check` + required tests）与文档/devlog 收口。

## 风险
- 编译负担风险：默认启用 `wasmtime` 增加构建时长，需要保持 CI 绿并评估本地门槛。
- 网络复杂度风险：request-response 引入连接状态与超时路径，需限制错误传播避免 runtime 噪音。
- 拓扑误配风险：`triad_distributed` 参数较多，必须在 CLI 阶段做完整互斥与必填校验。
- 身份兼容风险：动作与消息结构扩展涉及序列化兼容，需要 `serde(default)` 与回放容错保证平滑升级。

## 原文约束点映射（内容保真）
- 约束-1（目标与问题定义）：沿用原“目标”章节约束，不改变问题定义与解决方向。
- 约束-2（范围边界）：沿用原“范围”章节的 In Scope/Out of Scope 语义，不扩散到新增范围。
- 约束-3（接口/里程碑/风险）：沿用原接口字段、阶段节奏与风险口径，并保持可追溯。
