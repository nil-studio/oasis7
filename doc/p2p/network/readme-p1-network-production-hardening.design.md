# README P1 缺口收口：分布式网络主路径生产化设计

- 对应需求文档: `doc/p2p/network/readme-p1-network-production-hardening.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project


## 目标、上游约束与适用范围

本设计的专业输入是配对 [PRD request 路由](readme-p1-network-production-hardening.prd.md#1-libp2p-request-路由)、[共识 topic](readme-p1-network-production-hardening.prd.md#2-共识-topiclibp2p) 与[目标平台约束](readme-p1-network-production-hardening.prd.md#4-oasis7net-与目标平台边界)。P2P/runtime owner 维护 transport/request 与 node bridge；共识 authority 决定消息是否有效，Viewer 只消费观测结果，QA 核对 required 回归和拓扑长跑。

本轮仅归整已有路由、默认拒绝和兼容边界，未修改运行行为；当前实现/验收以实际候选测试为准。PRD 已记载 wasm32 package-level check 的依赖阻断，本设计不把 API/stub 目标改写为已通过。

## 1. 设计定位
定义分布式网络 P1 主路径生产化方案：把请求层升级为 libp2p 多 peer 轮换重试，并把共识消息切到 libp2p pubsub 主链路。

## 2. 设计结构
- 请求路由层：`libp2p request/response` 支持多 peer 轮换重试；远端 `ErrorResponse` 计为当前 peer 请求失败并继续轮换，耗尽后返回 `NetworkRequestFailed`；无 peer 时按开关决定是否回退本地 handler。
- 共识主链路层：proposal / attestation / commit 使用 libp2p pubsub 主题作为生产默认主路径。
- 兼容兜底层：UDP gossip 继续保留为兼容链路，与 libp2p ingest 并存。
- 目标平台层：native `oasis7_node` 单向依赖 `oasis7_net`；wasm32 通过 target cfg 使用保持 API 形状的 unavailable stub，不承诺完整 node/libp2p runtime。
- 稳定性回归层：围绕 node 网络桥接与共识广播路径维持 required-tier 回归稳定。

<a name="3-关键接口-入口"></a>

## 3. 关键接口 / 入口
- `Libp2pReplicationNetworkConfig::allow_local_handler_fallback_when_no_peers`
- `aw.<world_id>.consensus.proposal`
- `aw.<world_id>.consensus.attestation`
- `aw.<world_id>.consensus.commit`
- `crates/oasis7_node/src/libp2p_replication_network.rs`
- `crates/oasis7_node/src/network_bridge.rs`
- `WorldError::NetworkProtocolUnavailable`

## 4. 约束与边界
- 生产默认链路优先走 libp2p，UDP 仅作兼容兜底。
- 无 peer 时默认拒绝 request，只有显式开关开启才允许本地 handler 回退。
- 双路径并存阶段必须依赖既有幂等守卫避免重复消息副作用。
- `oasis7_net` 不得反向依赖 node；wasm stub 的所有网络动作必须结构化失败，不能静默转成本地成功路径。

<a id="typed-failure-contract"></a>
### 4.1 Net → Node typed failure 合同（目标，尚未实现验收）

Trace ID: O7-REQ-9b6f5d9f-b86b-4b30-957a-6a7a5fe301c0

本专业要求限定 replication 请求错误的内部跨层语义：诊断文案不拥有 fallback、重试或拒绝策略。Net/request producer 保留真实失败来源，Node bridge 传递事实，replication policy 消费事实；不得仅把旧 reason 包装成 enum。产品层不另立要求：本条是 [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) 可用性反馈的内部工程边界，不新增玩家可用性承诺。以下为迁移目标，不表示现有 `network_error_classification.rs` 或 `error_mapping.rs` 已消除文本分类。

内部 outcome 至少区分以下信息；这些是语义职责，不是新增 wire 字段或 Rust 类型名的承诺：

- **来源与证据等级**：本地执行/接收路径赋值的 origin（transport、remote response、DHT、provider routing、publish、decode/validation）和 local observation / peer claim / legacy interpretation。远端不得自行指定本地 origin。
- **请求上下文**：本地持有的 requested protocol、操作种类和适用 peer/route；未知字段保留 unknown，不从错误文案猜造已验证上下文。请求了 fetch-blob 不证明失败原因为“不支持 fetch-blob”。
- **语义原因**：已知的 connection gap、timeout、missing handler/protocol、not found、busy/rate limit、业务拒绝、validation rejection 或 unknown，并保留其来源证据。code、retryable 和 cause 分开；generic `ErrNotAvailable` 或 `retryable=true` 不足以推导 connection gap。
- **诊断**：原 code/message/retryable、底层错误与链路上下文保留；已确认的机器字段与纯诊断字段分开，不据 peer 文案升级为本地 transport 事实、验证结果或 authority。已提取事实后的纯诊断文案改变不得改变策略。

<a id="typed-failure-source-matrix"></a>
#### 来源、映射与策略边界

| producer / 本地确定的 origin | 可携带的事实与未知项 | Node 消费边界 |
| --- | --- | --- |
| 本地 transport / outbound request | 本地请求协议、连接/拨号/超时结果；底层 typed 原因可用则直接保留，不可用则由有界 legacy adapter 提取或 unknown | 仅已确认的对应原因进入原有 connection-gap/timeout 分支；不把所有 outbound 失败视为可用性缺口 |
| 收到远端 `ErrorResponse` | 本地只确认收到某 peer 对本次请求的错误响应；其 code/message/retryable 及解释出的 cause 均为 peer claim | 区分远端 missing-handler 声明与 business-unsupported；同 `ErrUnsupported`、同 requested protocol 不能合并。允许原策略的兼容 fallback 不代表信任远端业务/权限判断 |
| DHT 查询/记录处理 | 本地查询完成、超时或记录缺失，与收到的记录解析/验证拒绝分开；未确认 DHT 原因保持 unknown | 保留既有 DHT 路径策略，查询不可用不能掩盖 malformed record、签名或 authority 拒绝 |
| provider 路由选择/耗尽 | 本地候选集、路由/尝试预算耗尽与先前请求结果；保留当前操作和先前 timeout | provider gap 不自动成为 connection gap；后续 provider-gap 不覆盖原 timeout 优先序，换 provider 不重置累计预算 |
| publish | 本地发布动作失败、topic 与可用底层原因；发布不等同 request | 不因 publish 文案含协议或 peers 字样进入 request fallback；不推导已传播或已提交 |
| response decode / validation | 本地解码失败或 head/root/hash/world/authority 验证拒绝；保留响应来源但不接受其 cause 自报 | 不转换成 availability/fallback。未知 wire code 按现行解码拒绝链处理；内部 unknown 不是旧 wire 可接受未知 enum 的声明 |

**保守消费。**unknown 或互相矛盾且未能可靠解释的事实不新增 fallback/重试资格，不转为成功；保留原失败及诊断。权限、状态、hash、authority 与 validation 拒绝不能被 availability 标签覆盖。远端“缺 handler”即使允许既有替代路径，替代结果仍须通过原 head/root/hash/authority 验证，不能绕开本地安全拒绝。

<a id="typed-failure-migration"></a>
#### 兼容迁移与失败恢复

1. 优先保持现有 wire 不变，只迁移 producer → bridge → policy 的内部 outcome。`DistributedErrorCode` 是 closed enum；保留现有 `ErrOverloaded` → `ErrBusy` 的适用 wire 映射。任何 wire 扩展必须另有明确兼容设计及旧→新/新→旧 fixtures（未知 code、缺失和新增字段），不能以内部 unknown 代替该工作。
2. 旧 wire/message 尚承载必要语义时，文本解释只留在明确的 legacy adapter，记录具体 producer、受支持输入形状、可提取事实及不足。adapter 不把宽泛包含协议的文字解释成“本地已证实不支持”；无法提取时保留 unknown。退出条件是相应 producer 已提供等价 typed 事实且该输入范围的新旧对照通过，不能删除仍被合法旧输入依赖的兼容语义。
3. 合法既有输入维持各操作原策略；刻意收紧矛盾 code/message 或恶意宽匹配的案例单列为行为修正，由 P2P/runtime 与安全/兼容评审核对，不同时声称这些案例与旧逻辑完全等价。所有操作保留原 timeout、retry、provider-route budget、日志和安全验证，不扩大 fallback。
4. 错误分类不是操作撤销或提交确认。耗尽、超时、解码失败均不证明请求未产生效果；沿原操作身份执行既有查询/恢复流程，不因类型迁移生成新的业务请求或放宽幂等边界。

<a id="typed-failure-verification"></a>
#### 验证目标与未覆盖项

迁移验收须在相同候选、native/libp2p 配置下做参数化新旧策略对照，分别覆盖 get-head、fetch-commit、fetch-blob 和上述全部来源：missing handler 与 business unsupported 的同 code/同协议反例、generic `ErrNotAvailable`、预算耗尽、timeout 后 provider gap、publish、malformed response、未知 wire code、拒绝与 availability 文案冲突。合法输入记录旧/新 fallback、wait/retry、错误优先序及验证结论一致；有意硬化案例单列期望差异和理由。事实提取完成后单独替换纯诊断文案，断言策略不变；不能对仍承载兼容语义的原 message 假定该性质。

现有源码锚点为 `crates/oasis7_node/src/replication_probe_gate.rs` 的 `provider_aware_fallback_does_not_hide_business_unsupported`、`provider_aware_fallback_treats_fetch_blob_unsupported_as_retryable_without_debug_text`，以及 `crates/oasis7_net/src/libp2p_net/error_mapping.rs`、`crates/oasis7_node/src/network_error_classification.rs` 的分类测试。它们仅提供旧行为对照起点，不证明全来源矩阵或新的 typed 合同；完整参数化对照仍待新增及实际执行，不以本设计宣称实现或通过。

## 5. 设计演进计划
- 先冻结设计与执行计划。
- 再完成 request 路由升级与共识 topic 接线。
- 最后执行 node 定向回归并收口文档与日志。

## 需求承接与验证范围

承接表只列上述既有设计实际承担的子义务，不改变产品功能状态，也不宣称覆盖产品要求的全部验收。验证表是方法与证据目标，不是本次运行结果。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005)；Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b | 无 peer 默认结构化拒绝；仅显式配置可本地回退，网络不可用不能静默包装为远端成功 | [设计条款](#4-约束与边界) | P2P request layer；runtime/消费者仍负责产品可用性反馈 | 只承接 transport 子边界，不覆盖完整四态反馈、pending 或 receipt |
| [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005)；Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b | 远端 ErrorResponse 触发 peer 轮换，耗尽结构化失败；重试不把本地 handler 当默认成功 | [设计条款](#2-设计结构) | P2P request routing；消费者解释错误 | 不证明应用层幂等或已提交请求结果确认 |
| [REQ-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001)；Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39 | 共识 topic 传递 proposal/attestation/commit，消息传播本身不获得最终性权威；生产默认 libp2p 与 UDP 兼容 ingest 保持既有幂等守卫 | [设计条款](#3-关键接口-入口) | node bridge 与共识 authority；QA 拓扑测试 | 本条只支持传播与一致性回归，不覆盖完整提交证明或持久性 |
| [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005)；Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b | 内部错误保留因果来源、证据等级和语义原因；未知保守拒绝新增资格 | [来源矩阵](#typed-failure-source-matrix) | Net/request producer、Node bridge、replication policy 与安全/兼容评审 | 仅承接既有可用性反馈的失败分类子边界；不修改 wire、不证明实现或完整四态反馈 |
| [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005)；Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b | 合法输入保持原策略；有意硬化单列；保持 wire 与预算，legacy adapter 有界迁移 | [兼容迁移](#typed-failure-migration) | Net/request producer、Node bridge、replication policy 与安全/兼容评审 | 仅承接既有可用性反馈的失败分类子边界；不修改 wire、不证明实现或完整四态反馈 |

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) | [设计条款](#4-约束与边界) | 无 peer 默认结构化拒绝；仅显式配置可本地回退，网络不可用不能静默包装为远端成功 | [定向测试选择](../../../testing-manual.md#testing-default-selection-tree)；源码 `crates/oasis7_node/src/libp2p_replication_network/tests.rs`：同候选 native/libp2p 下执行 `libp2p_replication_network_request_rejects_without_connected_peers_by_default` 和 `libp2p_replication_network_request_falls_back_to_local_handler_when_enabled` | 定向 cargo test 输出及 feature/target 配置 | 只承接 transport 子边界，不覆盖完整四态反馈、pending 或 receipt；本轮未运行行为测试 |
| [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) | [设计条款](#2-设计结构) | 远端 ErrorResponse 触发 peer 轮换，耗尽结构化失败；重试不把本地 handler 当默认成功 | [定向测试选择](../../../testing-manual.md#testing-default-selection-tree)；源码 `crates/oasis7_node/src/libp2p_replication_network/tests.rs`：同候选 native/libp2p 执行 `libp2p_replication_network_request_round_robins_across_connected_peers`（两个成功 peer 的轮换）、`libp2p_replication_network_request_retries_next_peer_when_remote_handler_fails`（一失败、一成功 peer，最终返回成功），以及 `libp2p_replication_network_preserves_remote_unsupported_error_code`（单 peer 连续两次请求均断言 `NetworkRequestFailed` 的 `ErrUnsupported` code） | 定向测试输出与上述分支断言；不代签世界效果 | 多 peer 全部返回 ErrorResponse 后耗尽及错误选择组合仍未证明；单 peer 测试未断言 retryable/message，不能外推多 peer 耗尽；不证明应用层幂等或已提交请求结果确认；本轮未运行行为测试 |
| [REQ-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001) | [设计条款](#3-关键接口-入口) | 共识 topic 传递 proposal/attestation/commit，消息传播本身不获得最终性权威；生产默认 libp2p 与 UDP 兼容 ingest 保持既有幂等守卫 | [testing-manual S9](../../../testing-manual.md#testing-p2p-storage-consensus-long-running)；同候选 triad/triad_distributed 使用 S9 对应 profile，核对 consensus_hash_consistent 与 mismatch count；完整最终性仍按 P2P authority 验收 | longrun summary、拓扑配置及 consensus mismatch artifact | 本条只支持传播与一致性回归，不覆盖完整提交证明或持久性；本轮未运行行为测试 |
| [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005)；Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b | [来源矩阵](#typed-failure-source-matrix) | 来源不可伪造，missing-handler 不等于业务拒绝，诊断不拥有策略 | [回归方法](../../../testing-manual.md#regression-oracle)；现有分类 test source `../../../crates/oasis7_node/src/replication_probe_gate.rs` 与 §4.1 参数化对照目标；相同候选 native/libp2p，覆盖三种请求与全部来源；完整矩阵待新增及执行 | 每例旧/新 outcome、策略和预算结果、诊断变体、硬化差异及候选/配置/实际命令日志 | 仅承接失败分类子边界；现有测试不证明全矩阵，本设计不代表已实现或行为测试通过 |
| [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005)；Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b | [兼容迁移](#typed-failure-migration) | 合法输入等价与有意硬化分开，wire 及预算不变；legacy adapter 退出须对照通过 | [回归方法](../../../testing-manual.md#regression-oracle)；现有分类 test source `../../../crates/oasis7_node/src/replication_probe_gate.rs` 与 §4.1 参数化对照目标；相同候选 native/libp2p，覆盖三种请求与全部来源；完整矩阵待新增及执行 | 每例旧/新 outcome、策略和预算结果、诊断变体、硬化差异及候选/配置/实际命令日志 | 仅承接失败分类子边界；现有测试不证明全矩阵，本设计不代表已实现或行为测试通过 |

## 决策、风险与未覆盖范围

保留 UDP 兼容路径降低迁移风险，但双 ingest 必须依赖既有高度/哈希幂等守卫；libp2p 默认路径不改变共识权威。多 peer 重试仍有 pending 迁移和时序风险，runtime/P2P owner 在请求失败分类、topic 或目标平台依赖变更时复核回归。wasm stub 只承诺 API 形状与结构化 unavailable；完整 wasm32 编译和跨平台行为按 PRD 记录的阻断继续单独核验。这里没有新增部署、回滚命令或性能阈值，S9 成功也不单独证明生产 readiness。
