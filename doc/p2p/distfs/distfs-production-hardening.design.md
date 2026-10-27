# DistFS 生产化硬化设计

## 1. 问题、目标与非目标

本地文件索引/CAS、manifest及reward probe保持可诊断完整性；严格区分权威状态恢复与可丢弃scheduler cursor连续性。

Owner role：runtime_engineer；canonical repository：eng-cc/oasis7；固定源码审读基线 f97f2253cdcb163bf984e3c90d59d792fe1c9b19，2026-10-15。本设计消费下表有限关系；旧章节保留作兼容详细条款，新增DES细化原义务而不改算法/schema/默认值/产品承诺。任务实际source/integration/tested tree/config/environment/window/结果由GitHub Issue/Project evidence维护，本文不伪造候选/通过结果。

非目标：不以文档active/合并、local fixture、历史MIG或CLI可编译宣称运行、BFT、部署、SLA或release。剩余215-object required set及root SC1..10/full组合不变。

## 2. 上游约束与相关角色

runtime/P2P拥有机制；ops拥有同窗口部署事实；QA拥有组合证据判定；producer拥有产品含义；消费者和WASM拥有各自schema/manifest/行为。professional_acceptance与product_requirement是关系类型，本文不新增机器trace schema或activation。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [professional_acceptance: dh-local-index](distfs-production-hardening.prd.md#dh-local-index) | path拒绝/CAS先于atomic index/local FileStore功能 | [des-dh-index](#des-dh-index) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-local-index](distfs-production-hardening.prd.md#dh-local-index) | expected hash在同根目录引用事务中匹配后操作，None兼容、冲突无分布式lock | [des-dh-precondition](#des-dh-precondition) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-local-index](distfs-production-hardening.prd.md#dh-local-index) | audit missing/dangling/orphan；delete只unreferenced且unpinned回收 | [des-dh-audit-gc](#des-dh-audit-gc) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-manifest](distfs-production-hardening.prd.md#dh-manifest) | sorted canonicalCBOR/CAS export，完整validate后atomic import | [des-dh-manifest](#des-dh-manifest) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-local-probe](distfs-production-hardening.prd.md#dh-local-probe) | local challenge integrity/cursor rotation，不是remote attestation | [des-dh-probe](#des-dh-probe) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-local-probe](distfs-production-hardening.prd.md#dh-local-probe) | atomic scheduler persist；missing默认，坏state warning/default不阻main settlement/tick | [des-dh-probe-state](#des-dh-probe-state) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-backoff-report](distfs-production-hardening.prd.md#dh-backoff-report) | budget/backoff/reason multiplier/old defaults兼容、不改算法/公式 | [des-dh-backoff](#des-dh-backoff) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-backoff-report](distfs-production-hardening.prd.md#dh-backoff-report) | chain-runtime CLI严格范围；epoch aggregate-only、详细state local | [des-dh-report](#des-dh-report) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-decompression-bound](distfs-production-hardening.prd.md#dh-decompression-bound) | 缓存填充在产生实际解压字节时限制预算，完整验证后才入缓存 | [des-dh-decompression](#des-dh-decompression) | DistFS decoder/cache 与 runtime/QA | 目标；不改变合法大 blob 的非缓存合同，不证明全局 RSS/CPU/网络防护 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[AC-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) | manifest及其引用blob经get_verified、size/version/path完整校验后替换本地索引；只承接hash-bound材料校验子范围 | [材料校验子范围](#des-dh-manifest) | P2P/runtime拥有提交与权限，QA拥有组合判定 | 仅材料供应子范围；不覆盖DCS-002恢复、DCS-005可服务或完整AC |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[AC-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) | 已 pin 的同 cut 材料不被并发本地回收删除；仅承接本地保留接缝 | [des-dh-local-retention-coordination](#des-dh-local-retention-coordination) | CAS、Runtime、节点复制、完整同步及 QA | 不证明完整历史闭包、远端故障域、掉电耐久性或全部 AC |

## 3. 当前状态、目标状态与差距

| 对象 | 当前专业合同（冻结source） | 目标 / 差距 | 实现 / 执行证据 |
| --- | --- | --- | --- |
| 本专题现行规则 | 配对PRD与保留旧章节的有界接口/失败合同 | manifest import可覆盖映射；probe坏state默认丢连续性但非权威恢复；shared refs/pins和failed-import nonmutation需独立验证。无remote challenge/PoRep/PoSt/ACL/lease/encryption/global恢复承诺。 | 本次source阅读，未运行behavior tests；具体测试定义存在不等于覆盖或通过 |
| 缓存解压输出 | read_to_end后才比raw_len，失败前可插入空entry/淘汰旧项 | §13预算及失败缓存语义待实现/验证，其他codec不代签 | compressed_range_cache.rs静态审读；未运行 |
| 跨节点组合能力 | local documented contract，历史完成是provenance | 依赖root/runtime/ops/consumer与full同candidate验证 | 运行/部署/发行未验证；不能由文档推导 |

## 4. 边界与结构

DistFS拥有local index/CAS/audit/manifest/probe算法；chain-runtime拥有CLI/config/load/persist/worker/report；调用方拥有冲突/覆盖许可。probe本地self-test不验证remote provider；resilience拥有provider/maintenance，S9A/runbook拥有真实环境。

### des-dh-index

`LocalCasStore` 规范化相对路径并拒绝 empty、absolute 和 ..；`write_file` 先落 CAS，再更新本地 files_index。原子文件替换不是 CAS、索引及调用方之间的跨组件事务；写失败时不能由错误反推不存在已写 CAS。read/stat/list/delete 保持本地 FileStore 抽象。

产品适用性：N/A；reason=本地路径映射与I/O接口不独立构成世界状态；scope=路径校验、CAS到索引的写入顺序。


### des-dh-precondition

`write_file_if_match` / `delete_file_if_match` 在当前索引 hash 匹配时继续操作；None 保留无前置条件兼容路径，非法 expected hash 拒绝。它在同一 CAS 根的 retention 锁内检查并修改索引，防止本地并发调用同时通过同一旧 hash；不提供跨节点锁或一般 FileStore 操作的分布式线性化。调用方处理冲突及重试节流。

产品适用性：N/A；reason=本地precondition不定义玩家版本或共识权威；scope=expected hash检查与None兼容。


### des-dh-audit-gc

`audit_file_index` 报告 missing file blob、dangling pin 与 orphan 集合。delete/overwrite 的旧 blob 只在无其他索引引用且 unpinned 时回收；`prune_orphan_blobs` 也排除被索引或 pin 的 blob。局部 GC 条件不证明世界 checkpoint/replay/archive 冗余，不能据此批准历史 pruning。

产品适用性：N/A；reason=本地引用回收不独立规定世界历史保留策略；scope=missing/dangling/orphan审计及本地引用/pin保护。

#### des-dh-local-retention-coordination

Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39。

同一个本地 CAS 根目录的 pin 更新、文件索引变更/导入及删除回收，共用稳定的 `.retention.lock` OS 独占锁。锁覆盖读取引用快照、检查 blob 存在、发布 pin/索引及删除 blob 的整个操作，不能在取得旧 pin 快照后放锁再 sweep。独立实例和进程使用同一载体；载体不删除、不替换，不用 PID/超时抢锁，进程退出由 OS 释放。符号链接、非普通文件或非空载体拒绝。锁不适用的平台返回错误，不能降级为无锁回收。

成功 pin 返回后，遵守此协议的 prune 和文件删除不得移除该 blob，直到对应 pin 被显式撤销；全局 pin 的 read-modify-write 同样串行，避免并发增加丢失。blob put 也与删除串行，但裸 put 不授予保留资格；put 与后续 pin 之间允许 GC 先行，pin 必须在锁内重查存在性，缺失返回错误，调用者不能发布依赖该 pin 的提交。直接文件系统修改、旧版进程及不支持可靠 OS 锁的文件系统不在本地协调保证内，启用并发维护前须升级全部访问者。

Runtime sidecar、节点复制 orphan/legacy sweep 和 attempt duplicate 回收也必须在同一 guard 下读取 pins 并删除。sidecar 在完成 CAS put/staging 后取得 guard，先重验新 generation 的闭包，再读取当前索引、合并并发布新索引、按该索引 sweep、写回 GC 结果；guard 不能在最后索引写回前释放，旧实例不能用旧快照覆盖新 generation。未取得 guard 前若 staging/blob 已被回收，重验失败且不得发布新索引；此接缝不把尚未发布材料变为已保留根，也不授予一般 GC 对专用 sidecar 根的回收资格。attempt 在资格、双端独立闭包及持久 retire fence 确认后，才由该私有存储的所有者撤销 `execution_bridge_v1` 内与精确 duplicate 输出重合的内部 pin；变更的 shard 和父目录先 fsync。其他 scope、全局 pin、无关内部引用和文件索引均保留；输出若仍被这些引用保护则等待后续维护，不能标为 Reclaimed。损坏成员修复在锁内重算 hash，已经恢复正确的成员不能因调用者的旧观察再次被删；真正损坏的成员可被清除后按原 hash 重新获取，原 pin 不被撤销。

`prune_unpinned` 保留既有按容量淘汰语义：只保护 effective pins，可淘汰有文件索引但未 pin 的 blob，随后 audit 报告 missing；delete/orphan sweep 额外保护索引引用。此区别不改变配额或保留策略。完整同步的耐久 pin 发布器在同一 guard 内执行容量预检、blob fsync 和原子 shard 发布；其 GC 调用前后的容量采样不是并发写入下的全局容量事务，超限仍明确报错。该锁不是网络锁、世界根闭包证明、磁盘掉电耐久性或 checkpoint/历史 GC 授权。

### des-dh-manifest

`manifest.rs` 从排序后的本地索引生成 canonical CBOR 并写 CAS。导入先验证 manifest hash、size、version，再验证每项规范化路径、blob hash/size 与重复路径，全部完成后替换本地索引。导入是受控替换而非 merge；缺 blob 拒绝，不能将此当世界快照或权威恢复。hash-bound 材料仍受本页产品边界约束。

产品适用性：N/A；reason=本地索引导出导入不独立定义世界恢复；材料验证边界另行承接；scope=排序、编码和本地index替换机制。


### des-dh-probe

`challenge_scheduler.rs` 从本地 CAS 集合按 cursor 选样，生成 request、receipt 并验证/聚合；轮转避免本轮重复命中。hash mismatch 等失败计入报告并影响调度状态。该自检没有远端 provider 的 custody 或 attestation 语义，也不是 PoRep/PoSt。

产品适用性：N/A；reason=本地自检不能独立证明生产数据可用性；scope=local CAS采样、轮转、验证与计数。


### des-dh-probe-state

`distfs_probe_runtime.rs` 的 load 对缺文件给默认 cursor，对读取/解析错误返回 Err；`reward_runtime_worker.rs` 捕获后 warning + default。persist 经 atomic helper 写 scheduler state，worker 对失败记 warning 并继续主循环。仅 best-effort 连续性，不能把默认化视作 checkpoint 或成功恢复；完整I/O失败路径的运行证据仍待补。

产品适用性：N/A；reason=可丢弃scheduler连续性不构成世界恢复；scope=probe state加载、默认化和原子持久化调用。


### des-dh-backoff

`StorageChallengeAdaptivePolicy` 约束每轮检查预算、base/max backoff 和各原因 multiplier；失败轮设置 deadline，到期前 skip 并保留累计观察字段。旧快照新增字段采用 serde defaults。reason multiplier 改变本地调度间隔，不改变 challenge 验证、reward 公式或网络协议。

产品适用性：N/A；reason=局部I/O预算与退避不是经济或玩家结算规则；scope=检查数、backoff、原因分类和旧state默认值。


### des-dh-report

chain-runtime CLI 经 `parse_distfs_probe_runtime_option` 检查正整数/非负范围；probe worker 调用 policy 并持久化 cursor。epoch JSON 的 distfs 子对象仅累计 total_checks、failed_checks、failure_ratio；详细 cursor/config/backoff 留在本地 state。scheduler 单测不代签 CLI→worker→epoch 序列化全链。

产品适用性：N/A；reason=专业诊断统计不新增玩家SLA或收费承诺；scope=CLI范围、worker调用及aggregate report。


### des-dh-material-authority

本页只供应 hash-bound 材料和局部诊断；[AC-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) 的世界状态结论仍要求合法 profile 提交证明及对应历史/root。[AC-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-003) 禁止存储服务、provider、缓存或 probe 借读取/发布成功取得最终写入权。现有 P2P [receipt/finality](../design.md#des-p2p-receipt-finality) 与[角色隔离](../design.md#des-p2p-layer-authority)拥有组合边界；本页不新增投票、激活、恢复或服务闸门。

上面各机制的产品 N/A 只限定其局部调度/I/O义务，不排除此材料与权限关系。产品卡现有部分覆盖和未核实范围不因本页规范化提升；恢复链、真实故障域、正向续跑和消费者可服务仍须独立验证。

## 5. 关键运行流程

path校验→CAS写→atomic index；if-match查当前hash再write/delete，None旧兼容；delete仅reclaim无索引ref且unpinned。manifest按canonical排序CBOR导出CAS，导入完整validation→atomic替换；fail保留原映射。probe load missing默认、unreadable/malformed warning默认→cursor轮转/预算/退避→aggregate→atomic scheduler state，不阻settlement主链。

拒绝保留可定位原因；本页的候选数、轮次或检查配额不证明统一timeout、自动retry次数或时延上界。并发冲突遵循原guard/precondition/共识authority，不能因接线或重连获得新权限。取消/替代只有外部专业合同明确支持时成立，本设计不新建取消或补偿schema。

## 6. 接口与数据合同

FileMetadata path/hash/size/updated_at；write/read/stat/list/delete是local抽象；write_file_if_match/delete_file_if_match expected hash在同一CAS根的引用事务内核对，不是跨节点锁。FileIndexAuditReport missing/dangling/orphan；FileIndexManifest/Ref canonicalCBOR/CAS。DistfsProbeRuntimeConfig和reward-distfs-probe/adaptive-multiplier CLI属chain-runtime；旧state defaults兼容。

| producer → consumer | 输入身份/版本 | 顺序与幂等 | success / error | 兼容与资源边界 |
| --- | --- | --- | --- | --- |
| local store/probe → chain-runtime | 原PRD列明identity/hash/version，不复制schema | 上述flow及DES验证顺序；local幂等不推导global exactly-once | 完成相应validation/apply/persist/publish后才声明该结果；失败不伪进展 | 原policy/defaults及旧data兼容；不加未知deadline/阈值 |

## 7. 状态、事务与持久化

CAS先于index atomic；delete不会承诺底blob永不处理。manifest import是受控替换不是merge，完整校验前不apply。probe文件reward-runtime-distfs-probe-state.json只scheduler state；warning/default不能用于PoS/世界recovery。backoff记录连续失败、deadline/skip/时长/reason/multiplier本地，epoch只aggregate checks/failures/ratio。

accepted、applied、persisted、published必须分别具备各自证据；workflow done/文档active不属于运行状态。外部receipt/journal/state root生命周期和recovery由runtime authority；本地观察不能提供更强保证。

## 8. 部署、安全与运行约束

路径禁止empty/absolute/..；delete/orphan GC只处理unreferenced AND unpinned，quota eviction按既有effective pins规则；precondition冲突调用方retry节流和所有权，同根引用事务不提供跨节点线性化。budget/base/max/reason multiplier控I/O，不改变challenge验证/reward公式/network协议。secrets不入证据，保state/log/error纠config/code/deployment根因。

设计只定义受控输入和失败条件；操作留在现行runbook。当前governed live网络raw-copy禁令和signed-V2边界只适用于该runbook环境，不否定历史/offline local事实。任何真实fleet/health/restore claim需固定inventory/manifest/package/config与同窗口证据，不从triad标签或本地成功推导。

## 9. 质量与容量

| 环境/刺激 | 预期响应 | 判定指标/阈值来源 | 验证入口 | 当前证明范围 |
| --- | --- | --- | --- | --- |
| 同冻结候选的local unit/fixture，逐DES负例与旧数据兼容 | 精确拒绝/no false mutation；正确路径满足原顺序 | 配对PRD常量/default/policy；未定义容量/latency不新增数字 | §11.1逐场景的source/手册 | source定义/计划，SN1未执行 |
| 多节点/恢复/服务角色组合，固定world/version/config/window | 同history/root或明确隔离/只读，不产生无证世界效果 | root与testing-manual S9A；root历史lag≤50、DistFS ratio≤0.1、mismatch0及insufficient_data阻断口径保留 | [testing-manual](../../../testing-manual.md) S9A/S9/S10；按原场景选择tier | 无真实规模、窗口或容量/SLA执行证明 |

## 10. 兼容、迁移与回滚

MIG067..075、MIG080仅历史；旧probe state字段default兼容。Phase5细report superseded、Phase8 CLI chain-runtime、Phase9backoff非externaltelemetry；MIG059/060 builtin WASM pipeline外部，feedback ledger不混权限。manifest错误覆盖无法靠重启撤销；known localbackup只按授权offline恢复，不泛化live raw copy。

本文保留原path/旧章节anchors与历史ID，不替换task truth；添加的接受/DES只细化旧合同。新输入、消费者或语义变更需重新绑定/专业审读。package/config回退不能复原已删chain state或撤回committed效果；破坏性操作依原runbook与runtime恢复合同。

## 11. 验证设计与可追溯性

下表是可复用计划：existing test source、planned场景与executed evidence分开。每条只消费准确upstream/DES；执行时须同可比较candidate/config/入口/环境，local fixture不升级为真实网络。未定位完整测试写planned，owner为runtime/QA；缺证阻断该组合结论，不能从required局部降低root full。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [professional_acceptance: dh-local-index](distfs-production-hardening.prd.md#dh-local-index) | [des-dh-index](#des-dh-index) | path拒绝/CAS先于atomic index/local FileStore功能 | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_distfs/src/lib_tests.rs`；existing file_store_rejects_invalid_paths、file_store_write_read_roundtrip、file_store_overwrite_updates_hash_and_metadata；index-write故障完整oracle planned。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-local-index](distfs-production-hardening.prd.md#dh-local-index) | [des-dh-precondition](#des-dh-precondition) | expected hash在同根目录引用事务中匹配后操作，None兼容、冲突无分布式lock | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_distfs/src/lib_tests.rs`；existing file_store_write_if_match_enforces_hash_precondition、file_store_delete_if_match_enforces_hash_precondition、file_store_if_match_rejects_invalid_expected_hash；只同进程。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-local-index](distfs-production-hardening.prd.md#dh-local-index) | [des-dh-audit-gc](#des-dh-audit-gc) | audit missing/dangling/orphan；delete只unreferenced且unpinned回收 | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_distfs/src/lib_tests.rs`；existing file_index_audit_reports_missing_dangling_and_orphan_blobs、file_store_delete_preserves_blob_referenced_by_another_file；prune_orphan_blobs_removes_only_unreferenced_data断言live/pinned保留、orphan删除；shared-ref测试断言最后引用删除才回收。非世界pruning证明。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-manifest](distfs-production-hardening.prd.md#dh-manifest) | [des-dh-manifest](#des-dh-manifest) | sorted canonicalCBOR/CAS export，完整validate后atomic import | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_cas/src/manifest.rs`；existing file_index_manifest_export_import_restores_index、file_index_manifest_import_rejects_missing_blob；roundtrip断言路径a/b排序、恢复2项及原字节；missing-blob测试仅断言错误，完整failed-import nonmutation仍planned；仅local mapping。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-local-probe](distfs-production-hardening.prd.md#dh-local-probe) | [des-dh-probe](#des-dh-probe) | local challenge integrity/cursor rotation，不是remote attestation | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_distfs/src/challenge_scheduler.rs`；existing probe_with_cursor_records_hash_mismatch_failure、probe_with_cursor_advances_and_accumulates_state；localCAS。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-local-probe](distfs-production-hardening.prd.md#dh-local-probe) | [des-dh-probe-state](#des-dh-probe-state) | atomic scheduler persist；missing默认，坏state warning/default不阻main settlement/tick | [testing-manual.md](../../../testing-manual.md)；S4 required planned chain-runtime load/persist：missing/unreadable/malformed/writefailure与worker进展；distfs_probe_runtime.rs仅implementation locator，尚未定位完整测试。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-backoff-report](distfs-production-hardening.prd.md#dh-backoff-report) | [des-dh-backoff](#des-dh-backoff) | budget/backoff/reason multiplier/old defaults兼容、不改算法/公式 | [testing-manual S4/S9A](../../../testing-manual.md)；exact local test source `crates/oasis7_distfs/src/challenge_scheduler.rs`；existing probe_with_policy_limits_checks_per_round、probe_with_policy_applies_backoff_after_failure_round、probe_cursor_state_deserializes_from_legacy_snapshot、probe_with_policy_rejects_zero_reason_multiplier；local。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-backoff-report](distfs-production-hardening.prd.md#dh-backoff-report) | [des-dh-report](#des-dh-report) | chain-runtime CLI严格范围；epoch aggregate-only、详细state local | [testing-manual.md](../../../testing-manual.md)；S4 required planned CLI拒非法值、legacy state/readback与epoch fields oracle；CLI/worker/report完整测试未定位，本地lib suite不足。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: dh-decompression-bound](distfs-production-hardening.prd.md#dh-decompression-bound) | [des-dh-decompression](#des-dh-decompression) | 缓存解压实际输出预算与完整性后发布 | [testing-manual S4](../../../testing-manual.md)；§13 的有界 reader/sink 计数、损坏输入、缓存不变和大文件兼容场景；planned local fixture，入口为现有 DistFS lib suite | source、固定容量配置、输出计数、缓存前后结果与命令退出码 | 最终报错不足；未运行，不证明 decoder workspace、RSS、CPU 或网络 DoS 上界 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[AC-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) | [材料校验子范围](#des-dh-manifest) | manifest及其引用blob经get_verified、size/version/path完整校验后替换本地索引；只承接hash-bound材料校验子范围 | [testing-manual S9A](../../../testing-manual.md)；planned同candidate P2P/runtime消费负例：错误hash/缺提交证明/非权威服务输入拒绝，观察无canonical进展；本页局部fixture只验证材料机制 | source/config/world/profile与真实拒绝结果、canonical前后状态；QA独立判定 | 未运行，local CAS、manifest、probe或DHT成功均不足以证明完整AC |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[AC-DCS-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-001) | [des-dh-local-retention-coordination](#des-dh-local-retention-coordination) | 同根保留事务与成功 pin 后不丢材料 | [testing-manual 本地 CAS retention 与 GC 协调](../../../testing-manual.md#本地-cas-retention-与-gc-协调)；`crates/oasis7_cas/src/retention_tests.rs` 跨进程锁等待、pin 后原字节保留、杀进程释放原锁及回收先行的 missing；完整同步自有耐久发布器、Node 修复及 Runtime/attempt 消费者回归 | 当次 PR 的 source、命令、实际退出码与独立评审；原始 RED/GREEN 对照 | 合作进程与支持 OS 锁的本地文件系统；不证明完整历史 GC、远端域或掉电 |

## 12. 决策、长期风险与未决问题

选择保留现有专业合同并精确分配，拒绝以导航/历史标签免除真实设计，也拒绝将local证明升级为full。manifest import可覆盖映射；probe坏state默认丢连续性但非权威恢复；shared refs/pins和failed-import nonmutation需独立验证。无remote challenge/PoRep/PoSt/ACL/lease/encryption/global恢复承诺。

runtime/P2P负责缺失技术/实现；ops负责环境事实；QA负责计划覆盖和verdict；producer审读产品含义。新算法/schema、权限/默认值变化、额外消费者或claim扩展触发重新评审；未解除项保留在GitHub正式evidence及剩余required set，不能靠未来Issue占位关闭。下列原章节的里程碑/审计轮次是保留provenance，不维护可变进度。



- 对应需求文档：`doc/p2p/distfs/distfs-production-hardening.prd.md`
- 对应项目记录：GitHub Issue / GitHub Project

## 设计定位

本设计将 MIG-067..075 的九个完成阶段收敛为一个历史可追溯、以当前代码为行为真值的稳定入口。它覆盖本地文件索引保护、local-CAS challenge 自探测、reward-runtime 配置/状态接线和 adaptive backoff；不扩张为分布式一致性、远程证明或生产恢复设计。

## 结构与边界

- 文件索引层：规范化相对路径映射 `FileMetadata`；写入先落 CAS，再原子更新本地 `files_index.json`。CAS precondition、审计、孤儿判定和 manifest 导入导出保护本地索引；删除会回收未被索引引用且未 pin 的 blob。并发冲突由调用者处理，不能由接口名推导跨进程线性化。
- 本地 probe 层：cursor 和 policy 从本地 CAS 选择 blob，生成本地自检报告；它不验证远程 provider 也不构成多节点 attestation。
- reward-runtime 层：`oasis7_chain_runtime` 解析 `--reward-distfs-*` 参数，加载/原子写入 probe state，并把错误降为 warning + default cursor，保持主链路可用。
- adaptive 层：每轮预算、原因分类 multiplier 和最大 backoff 控制局部 I/O 压力；状态字段默认化确保旧 probe state 可读。
- reporting 层：当前对外 epoch report 只承载 aggregate checks/failures/ratio。详细 cursor/config/backoff 是本地状态而非外部 metrics contract。

## 运行面约束

- probe-state 缺失可默认初始化；不可读或 malformed 状态会警告后默认化。这是 best-effort scheduler continuity，不是数据、checkpoint 或 state-sync 的恢复路径。
- 不以重启掩盖 state、配置或 blob 失败。运维应保存状态文件、日志和稳定错误，再依现行 runbook 处理环境根因。
- distributed provider、replica maintenance、拓扑和 NodeRuntime 最佳努力轮询以 `distfs-distributed-resilience` 为权威；其失败语义不得被本地自探测文档覆盖。
- 当前代码/测试是行为锚点；Phase 5 的详细 epoch-report 描述已被 aggregate-report 现状替代，Phase 8 的 CLI 归属为 chain runtime，Phase 9 的 backoff 不是外部 telemetry。

## 验证入口

- `crates/oasis7_cas/src/{lib.rs,store.rs,manifest.rs,challenge_scheduler.rs}`。
- `crates/oasis7_chain_runtime/src/host/{cli.rs,distfs_probe_runtime.rs,reward_runtime_worker.rs}`。
- 本地回归：`env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib`。
- 涉及节点、真实数据可用性或恢复声明时，使用 `testing-manual.md` S9A 的 state-sync/blob closure、triad、real-env 和 release 分层验证；本设计不把本地测试提升为这些证据。

<a id="des-dh-decompression"></a>
## 13. 有界解压与缓存提交

产品适用性：N/A；reason=本地缓存输出预算不独立定义玩家状态或服务等级；scope=缓存准入后的解压分配与失败提交，hash-bound材料/权限仍由本页材料边界承接。

本节消费[缓存解压输出上界](distfs-production-hardening.prd.md#dh-decompression-bound)。它是待实现/验证的局部设计，不更改 blob 格式、缓存容量、合法大文件策略或现有完整性算法。

### 13.0 当前实现与目标缺口

冻结源码 `compressed_range_cache.rs` 仍在 `Vec::with_capacity(raw_len)` 后执行 `decoder.read_to_end(&mut bytes)`，最后才核对实际长度。`lib.rs::get_range` 的 32 MiB entry 准入只看头部 raw_len；它不能约束实际解压分配。本节预算是既有待实现目标，不能以 `local_blob_codec.rs` 另一条有budget路径代签。

当前实现解码前已插入空 entry，满额时先 FIFO eviction；错误时不写 `Some(bytes)`，但不保证整个 cache/FIFO 不变。下述“无错误缓存项、无关缓存保持原合同”是目标验收，必须分别检查已解码数据、空占位及淘汰影响。现有合法压缩范围读取/并发/轮转测试不含小声明大输出计数oracle，不证明此缺口已关闭。

### 13.1 校验和分配顺序

头部读取与格式/数值范围检查 -> 决定是否进入现有缓存准入路径 -> 建立有界解压输出 -> 核对完整解码、精确长度及原完整性要求 -> 发布缓存项。声明长度 `L`、转换为地址空间长度以及加一哨兵运算必须检查溢出；不得先以未验证长度 reserve/分配。

对缓存准入路径，令 `C` 为该路径实际缓存容量，`B = min(L, C)`。reader/sink 在每次交付解压字节时限制剩余预算；允许最多 `B + 1` 个输出字节用于发现超长，然后立即停止并返回原错误体系中可诊断的失败。不能先无界解码到 Vec，再包一层长度检查；也不能在外层已经完整分配后才套 bounded reader。

正常完成要求实际长度恰为 `L`，不是“读满 B 就当成功”；长度短于声明、压缩流截断、解码错误或完整性失败全部拒绝。错误不发布部分缓存，不改 blob/index，不触发另一条无界解码重试；无关的已有缓存项保持原合同。

### 13.2 容量范围与兼容

`C` 来自已有缓存配置/常量，不增加另一个漂移的全局限制。以 32 MiB 容量作为该问题的回归配置时，分别覆盖小声明、大实际输出和恰好达到容量的边界。正常超过缓存准入范围的合法 blob 仍使用原有非缓存路径；本节只防止已选缓存路径用不可信头部绕过自己的输出预算，不宣称所有读取都只有 32 MiB 内存。

输出计数限制不等于完整 decoder workspace、压缩输入驻留、线程并发总量或 CPU 限制。实现审读需确认具体库没有在预算之外先物化完整输出；有额外内部窗口/分配上界问题时独立说明，不能用这个局部补丁宣称全进程资源防护已完成。没有现场证据时不声称发生了远程 OOM、可利用攻击或数据损失。

### 13.3 可观察验收

| 输入 | 必须观察的结果 |
| --- | --- |
| 合法声明及合法完整压缩流；包含恰好 B 字节和原格式允许的空 blob | 精确读取、原完整性通过后才缓存；重复读取符合原缓存语义；不因零长度本身新增拒绝 |
| 小 `raw_len`，实际解压内容超过声明或 32 MiB 回归容量 | reader/sink 的实际输出计数最多 B+1，立即失败；未物化完整大输出，无新缓存项 |
| 零长度声明但实际非空、长度/加一溢出、截断和坏压缩内容 | 按相应边界失败，不通过分配、短读成功或回退掩盖错误 |
| 合法的大 blob 非缓存路径 | 原先支持的读取仍可使用；不因缓存修复变成全局尺寸拒绝 |

使用计数 reader/sink 或等价仪表验证实际输出上界，并比较失败前后的缓存状态；只断言 error 或依赖一次 RSS 采样都不足。正反例进入现有 DistFS lib 回归，文档和测试定义不代表实际执行通过。
