# DistFS 分布式韧性

## DistFS 韧性专业接受与设计分配

Owner role：runtime_engineer；canonical repository：eng-cc/oasis7；审读 source baseline：f9d5a552d9af04c1b1398262808198a58e560230（2026-09-26）。本文的 current 指冻结源所记专业合同；本次未独立验证实现、部署或测试通过。target 是规范目标，historical 是 MIG/CCG/TASK 与 dated evidence provenance。GitHub Issue/Project 维护实际任务与候选证据。新增稳定条款是原义务的细化入口；原章节/常量/命令/失败边界仍有效。

<a id="dr-provider-read"></a>
### Provider compatibility and strict read

六个 optional 画像字段 None 中性兼容；freshness/uptime/challenge/capacity/load/latency 权重、stable dedup/tie/candidate bound；indexed targeted retry only，empty/exhausted 明确失败，无无-provider fetch fallback。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="dr-distribution"></a>
### Distribution audit

每 blob 默认至少2副本；多个必要 blob 时拒绝同一 provider 全覆盖；batch fetch 前审计并报告不足 hash/违规 provider。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="dr-maintenance"></a>
### Bounded maintenance

目标3、repair/rebalance32/32、source/target850‰/450‰；task 包含 hash/source/target/Repair或Rebalance。transfer 成功才发布 target DHT；失败进入 failed_tasks 可重试；DHT滞后不保证全局最优/仲裁。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="dr-runtime-poll"></a>
### Optional nonblocking polling

到期成功进入轮次才推进 last_polled；未到期None/非法策略error；enabled+DHT+replication runtime+network+sample齐全才运行，缺依赖/无数据保留时间。仅本地target、指定source found+payload+BLAKE3后写CAS；error进last_error不阻tick。时间/错误不是checkpoint。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。


- 对应设计文档: `doc/p2p/distfs/distfs-distributed-resilience.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

## 目标

DistFS 在异构、波动的 provider 集合中保持可验证的数据可用性与有限的副本维护能力；读取、覆盖审计和维护均不得重新引入“任意单机提供完整执行数据”的隐含假设。

本规格承载历史专题 PRD-P2P-MIG-064、065、077、078、079 的当前合同。面向产品的连续性结果由 `doc/product/` 承载；本文件是 P2P/Runtime 的实现、失败和验证语义权威。

## 范围

本专题覆盖 provider 选择、严格 DHT 读取、分布覆盖审计、有界副本维护以及 NodeRuntime 的最佳努力轮询接线；不改变共识、复制主链路、持久化 checkpoint 或 replay/recovery 合同。

## 接口 / 数据

- `ProviderRecord` 可选能力画像为 `storage_total_bytes`、`storage_available_bytes`、`uptime_ratio_per_mille`、`challenge_pass_ratio_per_mille`、`load_ratio_per_mille` 和 `p50_read_latency_ms`。历史记录缺字段时保持 `None`，以中性分参与排序，不能阻断兼容节点。
- `ProviderSelectionPolicy` 以 freshness、uptime、challenge、capacity、load 与 latency 的可配置权重生成归一化排序；候选数量受 `max_candidates` 约束，排序必须保留稳定的去重/平局处理。
- `DistributedClient::fetch_blob_from_dht` 只能按 DHT provider 列表排序后逐 provider 定向重试。provider 为空时返回 `DistributedValidationFailed`；全部失败时返回最后错误或 `DistributedValidationFailed`。不得回退到无 provider 的 `fetch_blob(content_hash)`。
- `ProviderDistributionPolicy` 默认要求每 blob 至少 2 个副本，并在多个必要 blob 时禁止同一 provider 覆盖全部；批量 DHT 拉取在读取前执行审计，并报告副本不足 hash 或违规 provider。

## 3. 有界副本维护

- `ReplicaMaintenancePolicy` 定义目标副本数、每轮 repair/rebalance 上限以及源/目标负载阈值；默认值为 3、32、32、850‰、450‰。
- `plan_replica_maintenance` 输出 repair、rebalance 与警告；`ReplicaTransferTask` 必须包含 content hash、source、target 与 `Repair`/`Rebalance` 类型。
- `execute_replica_maintenance_plan` 逐项产生报告。只有 transfer 成功后才将 target provider 发布到 DHT；失败项留在 `failed_tasks`，不得污染索引，并可由后续轮次重试。
- DHT provider 信息可能滞后，计划并非全局最优或全局仲裁；每轮配额限制资源占用与局部失败影响。

## 4. 轮询与 NodeRuntime 接线

- `ReplicaMaintenancePollingPolicy` 使用 `poll_interval_ms`；`ReplicaMaintenancePollingState.last_polled_at_ms` 仅在到期且成功进入维护轮次后推进。未到期返回 `None`；非法策略返回验证错误。
- `run_replica_maintenance_poll` 返回带时间、plan 与 report 的轮次结果，供调用者观测执行与失败。
- `NodeReplicaMaintenanceConfig` 包含 `enabled`、`max_content_hash_samples_per_round`、目标副本数、repair/rebalance 上限、源/目标负载阈值和 `poll_interval_ms`。`NodeRuntime::with_replica_maintenance_dht` 可选注入 DHT；未注入时维护轮询跳过。
- Runtime 以 `replica_maintenance_last_polled_at_ms` 观测轮询节拍，并将维护失败写入通用 `last_error`；二者都不是持久化 checkpoint 或恢复状态。
- NodeRuntime 仅在配置启用、DHT、replication runtime、replication network 和候选 content hash 均可用时调用轮询；缺少任一依赖或无采样数据时跳过并保留现有轮询时间。
- 节点执行器只接受 target 为本节点的任务，向指定 source provider 定向请求；响应必须 `found` 且携带 payload，BLAKE3 hash 与 `content_hash` 一致后才写入本地 CAS。定向读取失败显式返回错误，不回退网络默认请求。
- 轮询错误记录为 runtime 错误观测，不阻断主 tick、共识或复制主链路。

## 里程碑

- PRD-P2P-MIG-064/065：完成兼容 provider 选择、严格读取与覆盖审计。
- PRD-P2P-MIG-077/078：完成有界维护计划/执行与轮询模型。
- PRD-P2P-MIG-079：完成 NodeRuntime 最佳努力接线；当前合同合并于本文件。

## 风险与非就绪边界

- 不承诺自动分片放置、跨机真实传输编排、全局任务仲裁、跨 DC/地域拓扑策略、纠删码/PoSt、弹性扩缩容控制面或长期 SLA。
- 不宣称 universal autonomous recovery、生产运维编排完成或持久化维护审计索引；当前轮询状态与错误观测不是 checkpoint、replay 或恢复保证。
- 能力画像缺失、权重失衡、provider 注册不全、DHT 滞后、定向读取失败、多节点并发规划及轮询资源占用都可能降低可用性或产生重复工作；严格失败、候选/轮次配额、索引成功后发布和 CAS 幂等是当前缓解手段。

## 6. 验证与追溯

- 重点回归覆盖：旧 `ProviderRecord` 兼容、排序/候选上限、无 provider 与 provider 耗尽失败、最小副本与单 provider 全覆盖拒绝、repair/rebalance 计划、成功发布与失败不发布、首轮/未到期/非法轮询，以及 NodeRuntime 缺依赖跳过和轮询错误不阻断 tick。
- 实现入口：`crates/oasis7_net/src/{client,provider_selection,provider_distribution,replica_maintenance}.rs` 与 `crates/oasis7_node/src/{lib,replica_maintenance_support,node_runtime_core}.rs`。
- 历史完成项与证据索引见GitHub task issue evidence；历史 review/audit 引用保留为 provenance，不构成当前入口或额外 readiness 声明。
