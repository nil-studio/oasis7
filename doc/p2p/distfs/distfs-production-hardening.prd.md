# DistFS 生产化硬化

## DistFS 本地硬化专业接受与设计分配

Owner role：runtime_engineer；canonical repository：eng-cc/oasis7；审读 source baseline：f9d5a552d9af04c1b1398262808198a58e560230（2026-09-26）。本文的 current 指冻结源所记专业合同；本次未独立验证实现、部署或测试通过。target 是规范目标，historical 是 MIG/CCG/TASK 与 dated evidence provenance。GitHub Issue/Project 维护实际任务与候选证据。新增稳定条款是原义务的细化入口；原章节/常量/命令/失败边界仍有效。

<a id="dh-local-index"></a>
### Local index integrity

拒绝空/absolute/..路径；CAS→atomic files_index；delete可回收unreferenced且unpinned；if-match在同一CAS根的引用事务中核对当前hash后操作，None兼容无precondition。不是分布式锁或一般FileStore跨进程线性化承诺。audit缺blob/dangling pin/orphan集合保留。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="dh-manifest"></a>
### Local manifest

规范化排序/canonical CBOR/CAS ref；导入完整验证后atomic替换本地index；受控调用方处理覆盖风险。不是全世界snapshot/replay proof。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="dh-local-probe"></a>
### Local challenge continuity

local CAS request/receipt/validation/aggregate；cursor轮转防同轮重复；reward-runtime-distfs-probe-state.json atomic写。缺文件default，不可读/malformed warning+default；best-effort scheduler而非权威恢复，不阻settlement/consensus/replication/tick。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="dh-backoff-report"></a>
### Budget, compatibility and report

chain-runtime config/CLI严格范围；budget/base/max backoff与hash mismatch/missing sample/timeout/read I/O/signature invalid/unknown multiplier；旧state新增字段default。epoch仅aggregate checks/failures/ratio，详细cursor/config/backoff是local state。不改challenge算法/reward公式/network。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="dh-decompression-bound"></a>
### 缓存解压的实际输出上界

这是本地压缩 blob 缓存填充路径的目标安全要求，不是整个 BlobStore 的统一文件尺寸上限，也不是已实施的内存峰值保证。头部声明的 `raw_len` 是待验证输入，不能独自证明后续解压分配有界。

对已通过头部格式、范围和缓存准入检查的读取，以声明长度和该路径缓存容量两者的较小值作为输出预算。解压产生字节时就执行预算检查；至多允许额外一个字节用于识别越界，然后立即失败。不得先无界 `read_to_end`/扩容，最后才用长度不一致拒绝。

只有解压完整、实际长度与声明一致且通过原有完整性检查的数据可以进入缓存。超界、截断或解码失败都不插入部分/错误缓存项，不修改原 blob/index，也不把损坏输入自动转入无界回退读取。正常的大 blob 非缓存读取保持既有合同，不能借此全局禁用所有合法大文件。

配对设计：[有界解压与缓存提交](distfs-production-hardening.design.md#des-dh-decompression)。验收必须观察实际交付到输出缓冲的字节上界及失败时缓存不变，不能只断言最终返回错误。decoder 内部 workspace、压缩输入、并发数量和 CPU 预算另有各自边界，本条不将输出预算冒充为进程 RSS 或远程 DoS 防护的完整证明。

> 历史整合说明：本专题整合 `distfs-production-hardening-phase1` 至 `phase9`（PRD-P2P-MIG-067..075）的已完成阶段。它是这些阶段的稳定专业权威与追溯入口；历史完成不构成部署、恢复、公开网络或 release readiness 结论。

- 对应设计文档：`doc/p2p/distfs/distfs-production-hardening.design.md`
- 对应项目记录：GitHub Issue / GitHub Project
- 分布式 provider 选择、复制维护与 NodeRuntime 最佳努力轮询的现行合同：`doc/p2p/distfs/distfs-distributed-resilience.prd.md`
- 模块/真实环境 claim boundary：`testing-manual.md#s9a链上大世界状态底座自闭环`

## 目标

本专题保留 DistFS 本地文件索引完整性、challenge 自探测和 reward-runtime 接线的历史生产化合同：在保持 `BlobStore`/`FileStore` 兼容的前提下，降低索引漂移、脏写和持续自探测失败的影响，并提供可配置的本地调度与诊断信息。

## 范围

不包含 CRDT/OT 多写者合并、跨进程或跨节点线性化、跨节点 challenge 协调、远程 attestation、PoRep/PoSt、动态链上参数下发、复制协议/共识提交重构、ACL/租约锁/端到端加密，或生产运维编排。

## 接口 / 数据与当前合同

### 本地索引完整性（MIG-067）

- `FileStore` 以规范化相对路径映射 `FileMetadata { path, content_hash, size_bytes, updated_at_ms }`，拒绝空路径、绝对路径与 `..` 穿越。写入顺序是先写 CAS，再原子更新本地 `files_index.json`；路径索引是本地可变入口，CAS 内容寻址仍是底层真相。
- `write_file` / `read_file` / `stat_file` / `list_files` / `delete_file` 只提供本地最小文件抽象，不承诺 ACL、租约锁、高阶目录操作、跨进程或跨节点线性化。删除路径映射后，实现会回收已不再被文件索引引用且未 pin 的 blob；不能沿用“删除永不处理底层 blob”的旧描述。
- `LocalCasStore::write_file_if_match` / `delete_file_if_match` 是同一进程内的逻辑 compare-precondition API：给出 `expected_content_hash` 时只在当前索引 hash 匹配时写入或删除；`None` 保留无前置条件兼容路径。
- 该 API 不是跨进程、多写者或跨节点的线性化保证。调用方必须处理版本冲突、重试节流和并发所有权，不能把它作为分布式锁或复制一致性证明。
- `FileIndexAuditReport` 用于定位缺失 blob、悬挂 pin 和孤儿 blob；孤儿回收仅面向未被索引且未被 pin 的集合。
- `FileIndexManifest` / `FileIndexManifestRef` 以规范化排序、canonical CBOR 和 CAS 引用导出文件索引；导入前必须完整校验，并原子替换本地 `files_index`。误用 manifest 仍可能覆盖本地映射，必须由受控操作流程处理。

### Challenge 自探测与调度（MIG-068..070）

- challenge request、receipt、验证和聚合统计提供的是本地 CAS blob 的自探测能力；它不是远程 provider、多节点或链上 attestation。
- `StorageChallengeProbeCursorState` 维护 blob cursor、轮次和累计结果；`probe_storage_challenges_with_cursor` / policy 变体按 cursor 轮转，以避免单轮重复命中同一批本地 blob。
- reward runtime 读取并原子写回 `reward-runtime-distfs-probe-state.json`。文件缺失时使用默认状态；不可读或格式错误时 runtime 记录 warning 并使用默认状态继续。该连续性是 best-effort，不是 checkpoint、state-sync、replay 或恢复保证。
- 调度失败不得替代或阻断 reward settlement、共识、复制或节点主 tick；稳定拒绝原因与 warning 是排障输入，不应用重启掩盖数据、配置或部署问题。

### 配置、退避与可观测性（MIG-071..075）

- `DistfsProbeRuntimeConfig` 和 `--reward-distfs-probe-*` / `--reward-distfs-adaptive-multiplier-*` 参数属于 `oasis7_chain_runtime`。默认值和严格范围校验限制误配；它们不属于 `oasis7_viewer_live`，也不是集中控制面或链上动态参数。
- 自适应策略有每轮检查预算、base/max backoff 和按 hash mismatch、missing sample、timeout、read I/O error、signature invalid、unknown 分类的 multiplier。它只收敛本地调度负载，不改变 challenge 验证算法、reward 公式或网络协议。
- probe state 的新增字段通过默认值兼容旧文件，并保留连续失败、退避截止时间、跳过轮次、累计/最近退避时长、原因和 multiplier 等本地状态。
- 当前 epoch report 只输出 aggregate checks/failures/ratio。早期 Phase 5 中“epoch report 输出完整 probe config、cursor state 和 challenge report”的细化字段已被当前实现取代；backoff 字段保存在本地 probe state，不是外部 epoch/metrics telemetry。外部健康/指标证据必须以当前 status、runbook 和 `doc/testing/evidence/` 为准。

## 3. 实现与验证锚点

- 本地 store、索引审计和 manifest：`crates/oasis7_cas/src/{lib.rs,store.rs,manifest.rs}`。
- challenge cursor、policy、兼容状态和自探测：`crates/oasis7_distfs/src/challenge_scheduler.rs`。
- CLI、probe-state load/persist、reward-runtime 接线：`crates/oasis7_chain_runtime/src/host/{cli.rs,distfs_probe_runtime.rs,reward_runtime_worker.rs}`。
- 变更最小回归入口：`env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib`；涉及全链路、拓扑、恢复或对外 claim 时，按 `testing-manual.md` S9A 选择附加验证。

## 里程碑与历史阶段语义

| 阶段 | PRD-ID | 保留结果/决策 |
| --- | --- | --- |
| Phase 1 | MIG-067 | CAS 写删、索引审计、孤儿回收与 manifest 基线。 |
| Phase 2 | MIG-068 | 本地 challenge request/receipt/validation 与聚合统计；非 PoRep/PoSt。 |
| Phase 3 | MIG-069 | challenge probe 到 reward runtime 的兼容接线；非网络级协调器。 |
| Phase 4 | MIG-070 | cursor 调度和本地 probe-state 持久化；错误状态仅 best-effort 默认化。 |
| Phase 5 | MIG-071 | chain-runtime CLI/runtime 配置、模块化与 aggregate report；旧细粒度 report 字段 superseded。 |
| Phase 6 | MIG-072 | 自适应失败退避、预算上限和旧 probe-state 兼容。 |
| Phase 7 | MIG-073 | reason-aware 调度和调优所需的本地语义。 |
| Phase 8 | MIG-074 | reason-aware multiplier 的 chain-runtime CLI 接线与校验。 |
| Phase 9 | MIG-075 | 本地 backoff 决策状态字段与兼容恢复；非外部 telemetry。 |

每项历史任务完成只证明当时实现与回归收口，不能推导出当前生产 custody、真实多节点 challenge、数据可用性 SLA、自动恢复、public_testnet 或 release pass。

## 已吸收的早期基础合同

- `PRD-P2P-MIG-080` 的标准 FileStore I/O 已由“本地索引完整性”段按当前实现承接；旧文件的无 blob 回收说法已纠正。
- `PRD-P2P-MIG-059/060` 的 builtin WASM artifact hash/hydrate/loader 合同已迁入 `doc/world-runtime/wasm/wasm-deterministic-build-pipeline.{prd,design}.md`。反馈账本的作者签名、append-only、tombstone、审计与限流继续由 `distfs-feedback-ledger-and-replication` 拥有，不属于 builtin artifact 权限。
- 历史 source triplet 的任务完成态只从 Git history 与 GitHub task evidence 追溯，不是当前 readiness 证据。

## 风险、运维、证据与非 readiness

- provider inventory、DHT 严格读取、跨 provider 覆盖审计、有界 repair/rebalance 与 NodeRuntime 轮询由 `distfs-distributed-resilience` 负责；轮询状态和 `last_error` 不是持久化维护审计或恢复状态。
- topology、节点 health、blob closure、checkpoint/rollback/state-sync、observer catch-up 和真实环境证据由 S9A、正式 network runbook 及 `doc/testing/evidence/` 负责。手工复制 data/checkpoint 只可作为 break-glass 证据，不能替代 live-candidate readiness。
- 出现本地 probe state 损坏、配置误配或 blob 异常时，先保全状态、日志和拒绝原因并定位 code/config/deployment 根因；重启或默认 cursor 只能是有限调度恢复，不能宣称链或数据层恢复。
- 本专题不替代 runtime 对实现语义的权威、QA 对 release gate 的裁定，或 LiveOps 对外部状态与承诺的口径。
