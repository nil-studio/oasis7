# DistFS 分布式韧性设计

## 1. 问题、目标与非目标

在异构provider中严守分布式读取假设，以有界repair/rebalance和optional非阻塞poll维护副本，避免无索引fallback隐藏缺失。

Owner role：runtime_engineer；canonical repository：eng-cc/oasis7；固定源码审读基线 f97f2253cdcb163bf984e3c90d59d792fe1c9b19，2026-10-15。本设计消费下表有限关系；旧章节保留作兼容详细条款，新增DES细化原义务而不改算法/schema/默认值/产品承诺。任务实际source/integration/tested tree/config/environment/window/结果由GitHub Issue/Project evidence维护，本文不伪造候选/通过结果。

非目标：不以文档active/合并、local fixture、历史MIG或CLI可编译宣称运行、BFT、部署、SLA或release。剩余215-object required set及root SC1..10/full组合不变。

## 2. 上游约束与相关角色

runtime/P2P拥有机制；ops拥有同窗口部署事实；QA拥有组合证据判定；producer拥有产品含义；消费者和WASM拥有各自schema/manifest/行为。professional_acceptance与product_requirement是关系类型，本文不新增机器trace schema或activation。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [professional_acceptance: dr-provider-read](distfs-distributed-resilience.prd.md#dr-provider-read) | None兼容/中性，stable rank/dedupe/tie/max candidates | [des-dr-selection](#des-dr-selection) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-provider-read](distfs-distributed-resilience.prd.md#dr-provider-read) | indexed定向retry；empty/exhausted明确error，无无provider fallback | [des-dr-read](#des-dr-read) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-distribution](distfs-distributed-resilience.prd.md#dr-distribution) | batch读取前min2及no single provider all necessary blobs审计 | [des-dr-distribution](#des-dr-distribution) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-maintenance](distfs-distributed-resilience.prd.md#dr-maintenance) | target3/32/32/850/450受quota；task identity与repair/rebalance分开 | [des-dr-plan](#des-dr-plan) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-maintenance](distfs-distributed-resilience.prd.md#dr-maintenance) | success transfer才target index，failed_tasks不污染且可retry | [des-dr-transfer-publish](#des-dr-transfer-publish) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-runtime-poll](distfs-distributed-resilience.prd.md#dr-runtime-poll) | 到期plan/report后推进last_polled，即使含failed_tasks；未到期None，非法策略error | [des-dr-due](#des-dr-due) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-runtime-poll](distfs-distributed-resilience.prd.md#dr-runtime-poll) | 依赖missing/no sample skip保time，error last_error不阻tick | [des-dr-runtime-poll](#des-dr-runtime-poll) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-runtime-poll](distfs-distributed-resilience.prd.md#dr-runtime-poll) | 仅self target、指定source found+payload+hash才CAS | [des-dr-local-executor](#des-dr-local-executor) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[AC-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) | 仅指定source响应found、有payload且BLAKE3等于任务hash后写CAS；只承接hash-bound材料校验子范围 | [材料校验子范围](#des-dr-local-executor) | P2P/runtime拥有提交与权限，QA拥有组合判定 | 仅材料供应子范围；不覆盖DCS-002恢复、DCS-005可服务或完整AC |

## 3. 当前状态、目标状态与差距

| 对象 | 当前专业合同（冻结source） | 目标 / 差距 | 实现 / 执行证据 |
| --- | --- | --- | --- |
| 本专题现行规则 | 配对PRD与保留旧章节的有界接口/失败合同 | staleDHT、画像失衡/注册不足、multi-node重复plan、resource争用；无全局最优。部署/真实传输/全自愈需外部证据，本设计local plan不关闭DCS2。 | 本次source阅读，未运行behavior tests；具体测试定义存在不等于覆盖或通过 |
| 跨节点组合能力 | local documented contract，历史完成是provenance | 依赖root/runtime/ops/consumer与full同candidate验证 | 运行/部署/发行未验证；不能由文档推导 |

## 4. 边界与结构

provider画像/DHT排名/覆盖审计由net；transfer执行器只本节点target并从指定source读；NodeRuntime仅在依赖齐全时最佳努力触发；DHT不是全局调度仲裁，CAS幂等不是全局去重。

### des-dr-selection

旧 ProviderRecord 的六个 optional 能力字段缺失时保留 None；评分对未知画像使用中性值。`ProviderSelectionPolicy` 按 freshness、uptime、challenge、capacity、load、latency 排序；同 provider 去重、平局顺序和候选限制由 `provider_selection.rs` 的比较器保持。`max_candidates=0` 的既有含义是保留全部唯一 provider，不能将其误写成零候选或全局有界。

产品适用性：N/A；reason=画像评分与候选选择是专业算法，不独立规定玩家结果；scope=None兼容、排序、去重、平局和候选数机制。


### des-dr-read

`client.rs::fetch_blob_from_dht` 读取 DHT provider，按排序后的每个 provider 定向尝试；空列表明确失败，耗尽时返回最后错误或验证错误，不调用无 provider 的默认 fetch。该函数的定向选择保证不能替代调用方的内容身份/世界权威校验。

产品适用性：N/A；reason=定向尝试与失败选择是读取机制，不新增玩家可用性承诺；scope=DHT候选读取及重试顺序。


### des-dr-distribution

`provider_distribution.rs` 先对 hash 和 provider ID 去重；默认每 hash 至少 2 个 provider，多个必要 hash 时禁止同一 provider 全覆盖。`fetch_blobs_from_dht_with_distribution` 在首次读取前完成审计；不足或全覆盖返回验证失败。DHT 声明不是这些副本当下可达、独立故障域或完整恢复材料的证明。

产品适用性：N/A；reason=provider集合覆盖审计不独立决定同世界恢复或可服务状态；scope=默认min2及多blob反全覆盖预检。


### des-dr-plan

`replica_maintenance.rs::plan_replica_maintenance` 规范化 hash，查询并去重 provider 后分别规划 repair 与 rebalance。既有默认目标副本数 3、两类每轮上限 32/32、源/目标负载阈值 850‰/450‰不变。任务携带 hash/source/target/kind；无目标候选保留 warning。DHT 滞后和多节点同时规划不具全局仲裁保证；配额限制本轮任务数，不等于真实网络时延上界。

产品适用性：N/A；reason=repair/rebalance计划是局部调度，不构成玩家恢复验收；scope=任务生成、默认阈值与两类轮次配额。


### des-dr-transfer-publish

`execute_replica_task` 先调用 transfer，成功才调用 DHT target publish；transfer 失败不会进入 publish。publish 自身失败也记入 `failed_tasks`，不能推断此前 CAS 已回滚。报告逐项累计 attempted/succeeded/failed；failed task 可用于后续重新规划或重试，但这里没有持久重试队列或自动成功保证。

产品适用性：N/A；reason=传输后索引发布是专业顺序规则，不独立证明最终性；scope=逐任务transfer/publish和失败报告。


### des-dr-due

`run_replica_maintenance_poll` 先校验 polling policy；未到期返回 None 且不推进时间。到期后 plan 成功并取得逐项 report，才写 `last_polled_at_ms=now_ms`。report 可以含 failed_tasks，因此推进时间不要求全部 transfer 成功；策略或 plan 返回错误时不会到达写时间步骤。该时间只是轮询节拍。

产品适用性：N/A；reason=轮询时钟是局部调度观测，不是恢复完成边界；scope=到期判定、轮次报告及last_polled更新。


### des-dr-runtime-poll

`replica_maintenance_support.rs` 在配置启用且 DHT、replication runtime、network 和采样 hash 齐全时调用维护；缺任一依赖或空样本返回原时间。wasm32 分支保持原时间。NodeRuntime 主循环将 helper 的 Err 写入 last_error；不能把 net 层 report 中的每项失败等同于 helper Err，也不能由 last_error 为空推导所有任务成功。

产品适用性：N/A；reason=最佳努力调用与错误观察不独立定义世界服务等级；scope=依赖检查、skip和runtime错误接线。


### des-dr-local-executor

`RuntimeReplicaMaintenanceTransferExecutor` 只接受 target 为本节点的任务；向 task 指定 source 定向 fetch，要求 found、有 payload 且 BLAKE3 等于任务 hash，之后才 `store_blob_by_hash`。错误不转为默认无 provider 请求。此处只提供 hash-bound CAS 材料；材料的世界身份、提交证明和后续消费由下述材料/权限边界约束。

产品适用性：N/A；reason=本地执行器调度部分不新增玩家承诺；hash-bound与权限产品义务另由本页材料边界承接；scope=self-target及定向请求机制，排除hash-bound材料的产品边界。


### des-dr-material-authority

本页只供应 hash-bound 材料和局部诊断；[AC-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) 的世界状态结论仍要求合法 profile 提交证明及对应历史/root。[AC-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-003) 禁止存储服务、provider、缓存或 probe 借读取/发布成功取得最终写入权。现有 P2P [receipt/finality](../design.md#des-p2p-receipt-finality) 与[角色隔离](../design.md#des-p2p-layer-authority)拥有组合边界；本页不新增投票、激活、恢复或服务闸门。

上面各机制的产品 N/A 只限定其局部调度/I/O义务，不排除此材料与权限关系。产品卡现有部分覆盖和未核实范围不因本页规范化提升；恢复链、真实故障域、正向续跑和消费者可服务仍须独立验证。

## 5. 关键运行流程

compat ProviderRecord None→stable rank/dedup/limit→distribution audit→按indexed provider定向retry→缺副本/负载plan→按quota逐task→source found+payload+hash→localCAS→transfer success后DHT publish。poll未到期None；非法策略error；缺依赖/无数据skip保留时间；到期且plan/report完成后推进last_polled，report可含failed_tasks；未到期或plan错误不推进。

拒绝保留可定位原因；本页的候选数、轮次或检查配额不证明统一timeout、自动retry次数或时延上界。并发冲突遵循原guard/precondition/共识authority，不能因接线或重连获得新权限。取消/替代只有外部专业合同明确支持时成立，本设计不新建取消或补偿schema。

## 6. 接口与数据合同

ProviderSelectionPolicy六权重/max_candidates；ProviderDistributionPolicy min2与多blob反全覆盖；ReplicaMaintenancePolicy目标3、repair/rebalance32/32、850‰/450‰；ReplicaTransferTask hash/source/target/kind。NodeReplicaMaintenanceConfig enabled/sample cap/quotas/threshold/interval受校验。未知旧画像None中性，zero candidate limit沿现有policy不改含义。

| producer → consumer | 输入身份/版本 | 顺序与幂等 | success / error | 兼容与资源边界 |
| --- | --- | --- | --- | --- |
| DHT/provider/transfer → NodeRuntime/CAS | 原PRD列明identity/hash/version，不复制schema | 上述flow及DES验证顺序；local幂等不推导global exactly-once | 完成相应validation/apply/persist/publish后才声明该结果；失败不伪进展 | 原policy/defaults及旧data兼容；不加未知deadline/阈值 |

## 7. 状态、事务与持久化

plan不是完成；每task success/failure隔离报告，failed_tasks可下轮retry；仅transfer成功后publish，失败不污染DHT。local CAS重复写可幂等但无全局arbitration。last_polled/last_error仅运行观察，不是持久maintenance audit/checkpoint/replay recovery；error不阻主tick。

accepted、applied、persisted、published必须分别具备各自证据；workflow done/文档active不属于运行状态。外部receipt/journal/state root生命周期和recovery由runtime authority；本地观察不能提供更强保证。

## 8. 部署、安全与运行约束

max_candidates>0时限制候选数，0保留全部唯一provider；sample/round quotas只限制各自采样/轮次任务数；非local target拒绝；指定source定向失败不走default请求；BLAKE3校验后才CAS。严格DHT fetch禁止未索引fetch；与root node storage-challenge有界availability fallback不同。无跨DC/PoSt/autoscale/controlplane/SLA承诺。

设计只定义受控输入和失败条件；操作留在现行runbook。当前governed live网络raw-copy禁令和signed-V2边界只适用于该runbook环境，不否定历史/offline local事实。任何真实fleet/health/restore claim需固定inventory/manifest/package/config与同窗口证据，不从triad标签或本地成功推导。

## 9. 质量与容量

| 环境/刺激 | 预期响应 | 判定指标/阈值来源 | 验证入口 | 当前证明范围 |
| --- | --- | --- | --- | --- |
| 同冻结候选的local unit/fixture，逐DES负例与旧数据兼容 | 精确拒绝/no false mutation；正确路径满足原顺序 | 配对PRD常量/default/policy；未定义容量/latency不新增数字 | §11.1逐场景的source/手册 | source定义/计划，SN1未执行 |
| 多节点/恢复/服务角色组合，固定world/version/config/window | 同history/root或明确隔离/只读，不产生无证世界效果 | root与testing-manual S9A；root历史lag≤50、DistFS ratio≤0.1、mismatch0及insufficient_data阻断口径保留 | [testing-manual](../../../testing-manual.md) S9A/S9/S10；按原场景选择tier | 无真实规模、窗口或容量/SLA执行证明 |

## 10. 兼容、迁移与回滚

旧ProviderRecord无画像继续None；optionalDHT缺省skip。配置/策略不合法显式错误，不invent新default。禁用maintenance不撤销已CAS/已发布副本；故障保留failed task，不能回滚canonical世界或把poll时间当恢复点。

本文保留原path/旧章节anchors与历史ID，不替换task truth；添加的接受/DES只细化旧合同。新输入、消费者或语义变更需重新绑定/专业审读。package/config回退不能复原已删chain state或撤回committed效果；破坏性操作依原runbook与runtime恢复合同。

## 11. 验证设计与可追溯性

下表是可复用计划：existing test source、planned场景与executed evidence分开。每条只消费准确upstream/DES；执行时须同可比较candidate/config/入口/环境，local fixture不升级为真实网络。未定位完整测试写planned，owner为runtime/QA；缺证阻断该组合结论，不能从required局部降低root full。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [professional_acceptance: dr-provider-read](distfs-distributed-resilience.prd.md#dr-provider-read) | [des-dr-selection](#des-dr-selection) | None兼容/中性，stable rank/dedupe/tie/max candidates | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_net/src/provider_selection.rs`；existing rank_providers_supports_legacy_records_without_capabilities、rank_providers_preserves_dedupe_tie_breaks_and_candidate_limit、rank_providers_zero_candidate_limit_keeps_all_unique_providers；local fixture。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-provider-read](distfs-distributed-resilience.prd.md#dr-provider-read) | [des-dr-read](#des-dr-read) | indexed定向retry；empty/exhausted明确error，无无provider fallback | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_net/src/tests.rs`；existing client_fetch_blob_from_dht_fails_when_no_providers、client_fetch_blob_from_dht_fails_after_ranked_provider_failures、client_fetch_blob_from_dht_retries_ranked_providers_until_success；local scripted provider。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-distribution](distfs-distributed-resilience.prd.md#dr-distribution) | [des-dr-distribution](#des-dr-distribution) | batch读取前min2及no single provider all necessary blobs审计 | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_net/src/tests.rs`；existing provider_distribution_rejects_insufficient_replicas、provider_distribution_rejects_single_provider_full_coverage、client_fetch_blobs_from_dht_with_distribution_prevents_single_provider_full_coverage；fixture不证明真实DA。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-maintenance](distfs-distributed-resilience.prd.md#dr-maintenance) | [des-dr-plan](#des-dr-plan) | target3/32/32/850/450受quota；task identity与repair/rebalance分开 | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_net/src/replica_maintenance.rs`；existing plan_replica_maintenance_creates_repair_tasks_for_under_replicated_blob、plan_replica_maintenance_creates_rebalance_tasks_for_overloaded_provider、plan_replica_maintenance_preserves_rebalance_tie_breaks；现有fixture核repair/rebalance任务、target和tie；完整quota边界仍planned。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-maintenance](distfs-distributed-resilience.prd.md#dr-maintenance) | [des-dr-transfer-publish](#des-dr-transfer-publish) | success transfer才target index，failed_tasks不污染且可retry | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_net/src/replica_maintenance.rs`；existing execute_replica_maintenance_plan_publishes_target_provider_on_success、execute_replica_maintenance_plan_does_not_publish_on_transfer_failure；StaticProvidersDht/ScriptedTransferExecutor只local oracle。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-runtime-poll](distfs-distributed-resilience.prd.md#dr-runtime-poll) | [des-dr-due](#des-dr-due) | 到期plan/report后推进last_polled，即使含failed_tasks；未到期None，非法策略error | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_net/src/replica_maintenance.rs`；existing run_replica_maintenance_poll_runs_first_round_and_updates_state、run_replica_maintenance_poll_skips_when_interval_not_elapsed、run_replica_maintenance_poll_rejects_non_positive_interval；local模拟时间，断言首轮时间1000、1050未到期保1000、非法interval时间None；含failed_tasks仍推进的组合断言planned。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-runtime-poll](distfs-distributed-resilience.prd.md#dr-runtime-poll) | [des-dr-runtime-poll](#des-dr-runtime-poll) | 依赖missing/no sample skip保time，error last_error不阻tick | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_node/src/tests_runtime_replica_maintenance.rs`；existing runtime_replica_maintenance_poll_skips_without_dht只断言last_error为空，未断言旧轮询时间；所有依赖permutation/no sample/tick进展oracle planned，existing名不代表覆盖全部。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dr-runtime-poll](distfs-distributed-resilience.prd.md#dr-runtime-poll) | [des-dr-local-executor](#des-dr-local-executor) | 仅self target、指定source found+payload+hash才CAS | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_node/src/tests_runtime_replica_maintenance.rs`；existing runtime_replica_maintenance_poll_executes_local_target_tasks断言commit进展与DHT有本节点发布，使用in-memory network；wrong target/missing payload/hash mismatch/CAS nonmutation完整负例planned；非远端编排。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[AC-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) | [材料校验子范围](#des-dr-local-executor) | 仅指定source响应found、有payload且BLAKE3等于任务hash后写CAS；只承接hash-bound材料校验子范围 | [testing-manual S9A](../../../testing-manual.md)；planned同candidate P2P/runtime消费负例：错误hash/缺提交证明/非权威服务输入拒绝，观察无canonical进展；本页局部fixture只验证材料机制 | source/config/world/profile与真实拒绝结果、canonical前后状态；QA独立判定 | 未运行，local CAS、manifest、probe或DHT成功均不足以证明完整AC |

## 12. 决策、长期风险与未决问题

选择保留现有专业合同并精确分配，拒绝以导航/历史标签免除真实设计，也拒绝将local证明升级为full。staleDHT、画像失衡/注册不足、multi-node重复plan、resource争用；无全局最优。部署/真实传输/全自愈需外部证据，本设计local plan不关闭DCS2。

runtime/P2P负责缺失技术/实现；ops负责环境事实；QA负责计划覆盖和verdict；producer审读产品含义。新算法/schema、权限/默认值变化、额外消费者或claim扩展触发重新评审；未解除项保留在GitHub正式evidence及剩余required set，不能靠未来Issue占位关闭。下列原章节的里程碑/审计轮次是保留provenance，不维护可变进度。



- 对应需求文档: `doc/p2p/distfs/distfs-distributed-resilience.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

## 1. 设计定位

把异构 provider 的兼容读取、分布覆盖校验与有界自愈收敛为一个 P2P/Runtime 合同：先以 DHT provider 索引约束读取，再以受配额控制的 repair/rebalance 补足副本，最后由 NodeRuntime 最佳努力地周期触发。

## 2. 分层与数据流

1. **Provider 数据与选择层**：`ProviderRecord` 保持旧字段可缺失；`ProviderSelectionPolicy` 对候选做确定性排序并限制候选数。
2. **严格读取与覆盖层**：`fetch_blob_from_dht` 只向已索引 provider 定向读取；批量读取先通过 `ProviderDistributionPolicy` 审计副本数及全覆盖集中度。
3. **维护控制层**：维护计划把缺副本和负载倾斜转换为有界 `ReplicaTransferTask`；执行报告隔离每项失败，成功后才发布 provider。
4. **轮询与 runtime 层**：`NodeReplicaMaintenanceConfig` 受校验地提供采样、配额、阈值和 interval；`node_runtime_core` 通过可选 DHT 注入与 `network_bridge` 接入轮询。轮询根据 last-polled 时间及 interval 决定是否运行；NodeRuntime 在依赖齐全时接入，缺依赖/无输入跳过，错误写入 `last_error` 而不影响主 tick。

## 3. 一致性与错误原则

- DHT 没有 provider、所有定向请求失败、策略非法或分布审计不通过，均显式失败；绝不以未知的全网读取隐藏违反分布假设的问题。
- 排序、计划输入、任务迭代和报告必须具有稳定可检查的行为；维护失败不更新 provider 索引。执行器拒绝非本地 target，要求指定 source 的 found payload 通过 BLAKE3 hash 校验后才写入 CAS；重复的本地 CAS 写入可幂等，但不等同于全局调度去重。
- `last_polled_at_ms` 是运行时节拍状态，不是持久化 checkpoint；本设计不声明 replay/recovery 的新合同。

## 4. 部署边界

NodeRuntime 的本地目标执行器不定义远端任务委派、跨节点协调或拓扑调度。配置、DHT、复制 runtime、复制网络和采样内容任一缺失时，维护工作跳过；启用后单轮工作量仍由策略配额控制。

## 5. 验证设计

针对兼容画像、稳定排序、严格失败、覆盖拒绝、计划/执行发布闭环、轮询到期性和 runtime skip/non-blocking 行为建立单元及跨 crate 回归。验证这些行为不构成生产拓扑、容量、SLA 或完整自愈 readiness 证明。
