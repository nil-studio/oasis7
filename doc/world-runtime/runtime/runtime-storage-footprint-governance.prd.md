# oasis7 Runtime：执行桥接与运行态存储体积治理

- 对应设计文档: `doc/world-runtime/runtime/runtime-storage-footprint-governance.design.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history；本文与 design 保留 replay、checkpoint、GC、恢复和验证合同。

审计轮次: 4

- 详细技术设计文档: `doc/world-runtime/runtime/runtime-storage-footprint-governance.design.md`

<a id="storage-profile-history-boundary"></a>
## 适用边界：容量策略与正式世界历史

`dev_local`、`release_default`、`soak_forensics` 仅选择容量、热窗口和检查点密度策略，不是 authority profile；它们不决定正式世界的提交权、独立持久条件或历史保留范围，也不改变原 `world_id`、`chain_id`、创世身份与合法资产连续性。

本文既有 legacy 本地执行桥接、`llm_bootstrap` 样本和 `2500` heights 预算保留原验收范围；latest 加载成功或窗口内 replay 通过不能作为正式世界完整历史证据。正文 SC-6、AC-5、NFR-5 及 Replay contract 的 `policy-retained H` 用于正式持久世界时，必须覆盖从合法初始锚到当前已确认边界的全部合法高度，不能由上述三个 storage profiles 缩窄。

正式世界的全部历史事实、原证据字节及必要执行/恢复依赖持续保留；“完整历史”不等于“每个高度各存一份展开全量 snapshot”。只有等价内容已持久保留、可精确取回并通过完整依赖验证时，才能减少重复表示或恢复加速材料；索引、哈希或固定保留数量本身不是替代证明。具体产品结果由[完整世界历史与无损存储](../../product/world-infrastructure/lossless-world-history-storage.prd.md#lws-requirements)承接，专业目标由[正式世界存储效率设计](formal-world-storage-efficiency.design.md#des-lws-001)规定；本次边界澄清不增加已实现或已验证声明。

完整历史会随真实新增信息增长。切换 profile、重算 pin 或触及预算均不得删除唯一历史内容；正式范围的依赖、持久条件或容量无法满足时，保留材料、报告原因并停止不安全维护或新写入准入。本文本地诊断降级不授予正式 serving 资格，重新服务仍须核验完整历史闭包、proof 与现行恢复闸门。

## 1. Executive Summary
- Problem Statement: 当前 `oasis7_viewer_live` / `oasis7_chain_runtime` 的默认运行态持久化在短时本地闭环中也会产生明显的磁盘膨胀：一次约 `2102` 高度的运行样本中，`output/chain-runtime/viewer-live-node/store` 约 `1.18 GiB`、`reward-runtime-execution-world/.distfs-state/blobs` 约 `635 MiB`，而当前最新执行世界实际只引用约 `1.55 MiB` 的 sidecar blob。执行桥接按高度保留全量 `snapshot_ref`、执行世界 sidecar 缺少引用回收、`snapshot.json` 中 `tick_consensus_records` 持续增长，导致本地调试、重复启动与长跑验证的磁盘成本过高。
- Proposed Solution: 为 runtime 引入“canonical replay log + checkpoint + 分层保留 + 引用治理 + 指标可观测”方案：把运行态数据拆分为当前可恢复 head、热历史窗口、稀疏检查点、权威变更日志、冷元数据与可回收孤儿 blob 六类数据，分别实施 retention / compaction / GC；同时对 `tick_consensus_records` 做热冷分层，明确“日志为真、快照为缓存”的回放契约，在控制默认磁盘占用的同时保证可追溯与可回放。
- Proposed Solution 补充（2026-05-15）: 在不改变 `content_hash` / `snapshot_ref` / `journal_ref` 合同的前提下，`LocalCasStore` 允许对大体积 blob 采用“磁盘透明压缩、读出仍返回原始 bytes”的落盘策略，优先压缩 execution bridge snapshots/journals 与 sidecar segments 的重复结构数据。
- Success Criteria:
  - SC-1: 默认 `dev_local` / launcher profile 在 `llm_bootstrap` 场景连续运行到 `2500` committed heights 后，`output/chain-runtime/<node_id>/store` 稳态占用 `<= 256 MiB`。
  - SC-2: `reward-runtime-execution-world/.distfs-state/blobs` 在提交已确认且清理成功后仅保留被当前有效 manifest / journal segments / 受保护 generation 与外部 pin 引用的 blob，孤儿 blob 数量为 `0`；`2500` heights 样本下清理收敛后的总占用 `<= 16 MiB`。提交与清理分别判定；准备新代或维护降级期间允许保留额外受保护数据，不能为满足稳态预算破坏恢复材料。正式世界的保护根还必须包含全部历史及其依赖，不能把“当前有效”限于 latest manifest。
  - SC-3: 默认 profile 下 `reward-runtime-execution-world/snapshot.json` 在 `2500` heights 样本下 `<= 512 KiB`，且 `tick_consensus_records` 热窗口外的数据仍可经归档索引审计读取。
  - SC-4: 在 retention / GC 执行后，latest-state 重启恢复成功率 `100%`，恢复后的 `execution_state_root` 与 GC 前一致。
  - SC-5: status/metrics 输出必须包含当前 storage profile、各目录字节数、pin 集大小、最近一次 GC 结果与失败原因，供 launcher / soak / release gate 直接消费。
  - SC-6: 对 retention policy 明确保留的任意目标高度 `H`，系统必须能从最近 checkpoint + canonical replay log 重建出与原记录一致的 `execution_state_root`；正式持久世界的 `H` 包含全部合法高度，局部 retained-height 验收不能代替完整历史与续跑验收。
  - SC-7: `release_default` 必须把 `execution_hot_head_heights` 收敛到不高于 `execution_checkpoint_interval`；在 exact-height restore 仍依赖热窗口快照的前提下，默认发布档位不得再为同一 replay 区间重复保留额外一倍的 execution snapshots。密度目标不授权删除唯一恢复材料；完整等价恢复尚未证明的范围继续保留并报告未收敛。

## 2. User Experience & Functionality
- User Personas:
  - Runtime 维护者：希望默认运行态可长期调试，不因历史快照无限增长而频繁手工清目录。
  - QA / 长跑维护者：希望根据 `dev_local` / `release_default` / `soak_forensics` 选择不同保留策略，同时不破坏恢复和审计。
  - 发布工程师：希望启动器与链运行时有可预测的磁盘预算，并能在 status/报表中看到增长来源。
  - 安全/审计评审者：希望 GC 只删除未引用历史，不删除当前 head 或仍被审计链路依赖的数据。
- User Scenarios & Frequency:
  - 本地启动器闭环：开发者每天多次执行，要求默认 profile 不出现无界磁盘增长。
  - CI / required-tier 回归：每次 runtime 存储链路改动都要验证 footprint、重启恢复和引用完整性。
  - Soak / forensic 长跑：按周或发布前运行，允许更高占用，但必须显式切换 retention profile。
  - 事故复盘：低频但关键，需要在 head window 或归档 checkpoint 范围内快速定位指定高度。
- User Stories:
  - PRD-WORLD_RUNTIME-013: As a Runtime 维护者, I want execution-bridge snapshots and sidecar blobs to follow bounded retention with explicit pin sets, so that default runs do not grow disk usage without limit.
  - PRD-WORLD_RUNTIME-014: As a QA/审计维护者, I want latest-state recovery and checkpointed replay evidence to survive GC, so that volume reduction does not break restart, diagnosis, or auditability.
  - PRD-WORLD_RUNTIME-015: As a 发布工程师, I want profile-based storage policies and metrics surfaced through runtime status, so that dev/release/soak environments can enforce different disk budgets predictably.
- Critical User Flows:
  1. Flow-RSF-001（默认开发 profile 持久化）:
     `committed height 推进 -> execution_bridge 写 latest snapshot/journal refs -> retention manager 核对完整保护根并更新 pin set -> 清理已证明无依赖或有等价持久表示的冗余 CAS 材料 -> status 输出最新 storage metrics`。正式保护根不随热窗口缩窄。
  2. Flow-RSF-002（执行世界 sidecar 保存）:
     `RuntimeWorld::save_to_dir -> 准备并验证新 generation 的完整引用闭包 -> 确认 index 持久提交 -> 调用方安装对应 World -> 清理不再受保护的数据 -> 成功清理后收敛到 latest + rollback-safe 及外部 pin`。提交未知时暂停写入并核对原候选；提交后的维护降级不撤销提交。
  3. Flow-RSF-003（重启恢复）:
     `进程重启 -> 读取已提交 index -> 完整装配并校验 latest 候选，候选完整性失败时尝试 indexed rollback-safe -> 恢复同代 World 与元数据 -> 核对 replay/root 与当前服务闸门`。健康旧候选只证明恢复材料可读，不自行恢复当前写权。
  4. Flow-RSF-004（soak / forensic 模式）:
     `运维选择 soak_forensics profile -> 放宽 hot window / checkpoint 保留 -> 长跑完成后导出 metrics/report -> 按 profile 关闭或回到 dev_local`。
  5. Flow-RSF-005（快照内链路压缩）:
     `tick_consensus_records 超过热窗口 -> 旧 records 写入 archive segment / index -> snapshot.json 仅保留热窗口 + archive anchor -> 审计查询按需读取归档段`。
  6. Flow-RSF-006（指定高度回放）:
     `选择目标高度 H -> 定位最近 checkpoint C -> 加载 checkpoint state -> 回放 canonical replay log (C+1..H) -> 对比 execution_state_root -> 输出 replay 成功/失败结论`。
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| Storage profile 选择 | `storage_profile`, `execution_hot_head_heights`, `execution_checkpoint_interval`, `sidecar_generations_keep`, `tick_consensus_hot_limit` | runtime / launcher / script 启动时加载统一 profile 枚举，可由 CLI/config 显式覆盖，默认 `dev_local` | `configured -> active -> degraded/failed` | 显式参数覆盖 profile 默认值；非法组合直接拒绝启动；`release_default` 默认要求 `execution_hot_head_heights <= execution_checkpoint_interval`，避免在 checkpoint 已覆盖的区间内重复保留整份 execution snapshot；删除前仍须证明等价恢复 | 仅启动参数/配置维护者可调整容量策略；不取得 authority 或缩短正式历史的权限 |
| Execution bridge retention | `snapshot_ref`, `journal_ref`, `height`, `retention_class=head/checkpoint/archive` | 每个高度写 record 后核对完整保护根并重算 pin set，仅对已证明可回收的表示执行 sweep | `written -> pinned -> pruned/archived` | latest 高度永远保留；checkpoint 按高度升序选取；正式世界全部历史与依赖纳入保护，离开热窗口不等于失去引用 | 自动执行，无用户绕过入口 |
| CAS blob 落盘压缩 | `content_hash`, `blob_encoding`, `raw_size_bytes`, `stored_size_bytes` | 大体积 blob 写入 CAS 时可透明压缩；读路径始终返回原始 bytes，hash 校验仍基于原始 payload | `raw -> compressed-on-disk` 或 `raw -> raw-on-disk` | 仅当压缩后连同 header 都小于原始 bytes 时才启用；`content_hash` 始终保持原始 payload hash，不因磁盘编码变化 | 自动执行；调用方无感知 |
| Sidecar manifest GC | `generation_id`, `manifest_hash`, `journal_segment_hashes`, `pinned_blob_hashes` | 确认 generation 持久提交后执行维护；确认未提交才保留旧 World，未知时隔离写入并核对 | 保存结果为 `NotCommitted/Committed/StatusUnknown`；`Committed` 后独立记录清理成功或维护降级 | 替代 index 提交前保护当前 index 的全部引用闭包；提交后只清理新 index 与外部 pin 均不再引用的数据 | 自动执行；仅 runtime 自身可写 |
| Tick consensus compaction | `tick_consensus_hot_limit`, `archive_segment_size`, `archive_index` | 超出热窗口时把旧记录转入 archive 段并更新索引 | `hot -> archived` | 热窗口按最新 tick 保留；archive segment 按 tick 连续范围分段；archive seek 必须能按 `from_tick..to_tick` 读回 | 自动执行；审计只读 |
| Replay contract | `commit_ref`, `checkpoint_anchor`, `retained_heights`, `replay_profile` | 指定高度回放时从最近 checkpoint + canonical log 重建目标状态 | `requested -> replaying -> matched/mismatched` | 本地 legacy/bootstrap 验收按其保留范围；正式世界范围为全部合法高度，root 校验之外仍须满足[完整历史边界](#storage-profile-history-boundary) | 自动执行；测试/审计只读 |
| Metrics / status 输出 | `bytes_by_dir`, `blob_counts`, `ref_count`, `pin_count`, `orphan_blob_count`, `last_gc_at_ms`, `last_gc_result`, `last_gc_error`, `degraded_reason`, `storage_profile`, `effective_budget`, `replay_summary` | status API / state file 输出指标；异常时给出原因，launcher / 脚本无需扫描内部目录即可判断预算与回放能力 | `collected -> published` | 各目录保留递归字节数，total_bytes 对真实目录并集去重；GC 结果保留失败、未运行、不可用及未知的不同语义；`replay_summary.mode` 仅允许 `latest_only` / `full_log_only` / `checkpoint_plus_log` | status 只读；配置修改受启动权限控制 |
| Replication hot/cold retention 对齐 | `max_hot_commit_messages`, `cold_index`, `archive_bytes` | 超出热窗口后迁移到冷索引/对象存储，并更新 metrics | `hot -> cold-indexed` | 热窗口按最新高度回推的连续高度范围定义，不再按“最近 N 个现存文件”解释；冷索引统一走 `<namespace>.cold-index/index.json` 元数据协议，并以 `from_key/to_key + first/last_content_hash + entry_count` 表示 cold range anchor；cold-index scan 与按高度 seek 必须共享同一边界语义；rollout 期间若 canonical/legacy 任一缺失，读路径会回填另一侧别名 | 自动执行；profile 仅决定冷热策略，正式历史保留与独立持久条件不变 |
- Acceptance Criteria:
  - AC-1 (PRD-WORLD_RUNTIME-013): `oasis7_viewer_live` / `oasis7_chain_runtime` 默认开发 profile 在 `2500` heights 样本下，`output/chain-runtime/<node_id>/store <= 256 MiB`，且 CAS sweep 后不存在“record 仍引用、blob 已删除”的 dangling refs。
  - AC-2 (PRD-WORLD_RUNTIME-013): `reward-runtime-execution-world/.distfs-state/blobs` 在提交已确认且清理成功后收敛到 latest + rollback-safe generation 及外部 pin 的引用集合，孤儿 blob 计数为 `0`；准备新代、提交未知或维护失败时保留额外数据不改变已确认提交结果。
  - AC-3 (PRD-WORLD_RUNTIME-014): 在 execution-bridge retention 与 sidecar GC 生效后，latest-state restart 恢复成功，恢复后 `execution_state_root`、`journal_len`、`module_registry` 与 GC 前一致。
  - AC-4 (PRD-WORLD_RUNTIME-014): 任意 GC 中断、部分写入或 pin set 构建失败都不得破坏当前已提交 index 引用的 latest、rollback-safe 与外部 pin 完整闭包；系统必须进入 `degraded` 并保留恢复所需数据。`StatusUnknown` 还须保护候选闭包并停止旧 World 发布，不得把降级等同未提交。
  - AC-5 (PRD-WORLD_RUNTIME-014): 对 retention policy 明确保留的任意目标高度 `H`，系统必须能从最近 checkpoint + canonical replay log 重建出与 `execution_records/H` 一致的 `execution_state_root`。正式世界的目标集合为全部合法高度；本地窗口样本通过不证明该完整集合通过。
  - AC-6 (PRD-WORLD_RUNTIME-015): status / metrics 输出必须暴露 `storage_profile`、`effective_budget`、`bytes_by_dir`、`retained_heights`、`checkpoint_count`、`replay_summary`、`last_gc_result`、`last_gc_error`、`degraded_reason`，并可被 launcher 或脚本直接采样，无需额外扫描内部目录。
  - AC-6.1 (PRD-WORLD_RUNTIME-015): `oasis7_chain_runtime`、`oasis7_game_launcher`、`oasis7_web_launcher` 与 launcher UI 必须暴露同名 profile 枚举输入，并沿进程边界无损透传到链运行时。bundle 入口 `run-game.sh`、`run-web-launcher.sh`、`run-chain-runtime.sh` 必须共享同一 `OASIS7_CHAIN_STORAGE_PROFILE` 覆盖通道，且未设置时继承各自二进制默认值，不在 shell 中重复硬编码默认 profile。
  - AC-7 (PRD-WORLD_RUNTIME-015): `snapshot.json` 在 `2500` heights 样本下 `<= 512 KiB`；旧 `tick_consensus_records` 通过 archive index 可按区间读取并通过链路校验。
- AC-8 (PRD-WORLD_RUNTIME-013/014/015): required/full 测试矩阵中必须包含 footprint 上限、restart recovery、GC fail-safe、profile 切换、archive 读取与 retained-height replay 回归，并分别判定下文 G1、G2、G3 及其交叉边界，不能用一个汇总恢复成功结果替代。

### Execution bridge record compatibility
- 当前 `ExecutionBridgeRecord` 为 schema v3：除 `world_id`、height、node/execution hash、state root、journal length、latest/snapshot/journal/commit/checkpoint/external-effect refs 外，还记录 `prev_node_block_hash`、`proposer_id`、`action_root` 与 world-head-proof ref/hash。
- V1/V2 记录仍可兼容读取；V2 不是当前最终 schema。bridge 仅证明本地 committed context、CAS refs、checkpoint manifest 与执行记录的一致性，不是 light-client、state/receipt proof、DA sampling、public-testnet 或 mainnet finality 证明。
- 恢复必须保存并校验 `last_applied_committed_height`：stale 或 non-contiguous committed height 失败关闭，checkpoint install/restart 后仍以 canonical log 与保留 checkpoint 重建并校验 state root。
- Non-Goals:
  - 不修改共识协议、区块语义或 DistFS challenge 计费语义。
  - 不对 `target/`、打包产物或 third-party 工具链缓存做统一清理策略。
  - 不在本阶段引入远端对象存储、外部数据库或跨节点历史归档服务。
  - 不要求默认开发 profile 永久逐高度保留展开的完整 `snapshot_ref`；本文 legacy/bootstrap 路径的回放验收仍限于其原保留范围。正式世界不以 `soak_forensics` 为深历史保留的前提，全部合法历史及依赖持续保留；减少展开快照须先具备已验证的等价重建路径，具体由[完整历史目标](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003)承接。

### Generation 保存与恢复的独立合同（目标）

以下条款细化 PRD-WORLD_RUNTIME-013/014/015 的本地持久化与恢复要求，承接[根 runtime generation 合同](../prd.md#runtime-generation-persistence-contract)。它们不授予正式链提交权，也不替代[合法 authority profile 的证明与独立故障域持久条件](../prd.md#runtime-authority-profile-commit)。`runtime_engineer` 负责保存/恢复语义，`qa_engineer` 分别判定各组验收；具体故障矩阵见[配对设计](runtime-storage-footprint-governance.design.md#storage-generation-validation-matrix)。合同定义、实现与实际验证结果分别成立。

<a id="storage-generation-g1"></a>
#### G1：普通保存结果、内存安装与重试保持一致

普通保存必须绑定稳定操作身份、原 base generation 与本次精确候选，涵盖 snapshot、journal、module store 及适用的 sidecar、archive、recovery metadata 和幂等记录承诺。不得只用 state root、逻辑高度或可被多次普通保存继承的 recovery metadata hash 判同一次提交。提交判定须建立在已验证的单写者和基线前提上；只看到替换后的 index 而尚未确认所承诺的目录同步等持久条件，不能判为 `Committed`；缺少提交确认也不等于已证明 `NotCommitted`。

| 本地持久结果 | 可观察含义 | 调用方义务 |
| --- | --- | --- |
| `NotCommitted` | 本次精确候选已确认未跨过持久提交点 | 保留旧 World；按原身份及当前有效前置条件重审或重试，不发布候选业务效果 |
| `Committed` | 本次精确候选及其持久条件已确认 | 安装或恢复对应 World 后才接受后续写入，返回原操作结果；镜像、GC、GC 结果 index 重写或 metrics 维护失败只报告维护降级 |
| `StatusUnknown` | 无法证明本次候选已提交或未提交 | 保留原操作/候选身份及旧、新恢复材料，禁止旧 World 再发布；先核对或完成原提交，不能当确定失败、换新身份重做或追加矛盾终态 |

解除写入隔离必须基于原候选的确定结果与对应 World 安装，而不是一次 I/O 重试成功。调用方和状态投影必须能核对保存结果、实际 generation、写入隔离及维护原因；现有状态投影可承载这些语义，不新增业务 receipt 或独立状态台账。已有已提交结果时，同身份同内容重试返回该结果；确认 `NotCommitted` 的基础设施失败可按原身份及当前有效前提重试，不永久缓存为业务终态。冲突内容拒绝，不能重复分配 lease、扣款或生成业务效果。

**独立验收 G1：** 分别覆盖 index 提交前失败、提交后错误、GC 结果重写与兼容镜像失败、持久结果无法确认；对账返回结果、活 World、重启 World 和原 lease/receipt。已提交结果不能被旧内存的后续无关操作覆盖，同身份重试无第二效果；未知期间后续写入被阻断，核对并安装后才恢复。对应[设计 G1](runtime-storage-footprint-governance.design.md#storage-generation-g1-design)。

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) 经 [root generation 合同](../prd.md#runtime-generation-persistence-contract) 与 [root generation receiver](../design.md#runtime-generation-persistence-design)，仅承接持久结果与普通调用方一致、已提交或未知时不伪造无效果终态。本地 generation durable commit 不是网络正式 authority 提交。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-generation-g2"></a>
#### G2：替代 index 提交前保留当前恢复集合

当前持久 index 指向 latest B、rollback-safe A 时，准备 C 不得提前删除 A/B 的任何必要材料。保护集合包含 generation-local manifest、payload、recovery metadata、snapshot/journal/module/archive 的 CAS 依赖和外部 pin，不只保护 blob 名称。C 的替代 index 确认持久提交后，才能按新 index 的 C/B 与外部 pin 回收其余材料；提交未知时保护旧、新集合。`keep=2` 是成功清理后的本地 generation 稳态，不禁止 A/B/C 的暂态共存；清理失败允许多占空间并后续重试。它不是正式世界历史保留数量，正式历史及其依赖始终纳入外部保护根，不能因退出 A/B/C 窗口而删除唯一内容。

**独立验收 G2：** 在已有 A/B 的第三代保存中，使替代 index 的写入或持久确认失败，旧 index 未替换时其字节与 A/B 完整闭包保持不变；B 原已损坏时，A 的可恢复性不能因尝试 C 而丢失。C 已提交后保留 C/B，只有确认不再引用的 A 才可回收；回收失败不改判 C 未提交。对应[设计 G2](runtime-storage-footprint-governance.design.md#storage-generation-g2-design)。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root generation 合同](../prd.md#runtime-generation-persistence-contract) 与 [root generation receiver](../design.md#runtime-generation-persistence-design)，仅承接替代 index 确认前保护旧恢复材料闭包及未知期间保护旧、新材料。不证明真实独立故障域持久性或重新 serving。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-generation-g3"></a>
#### G3：候选选择覆盖完整同代恢复

普通与 authoritative loader 都必须把候选记录验证、必要 CAS 实际读取与校验、解码、snapshot/journal/module/archive 装配、World 构造及适用恢复元数据核对放在同一个候选边界。latest 优先；候选自身的完整性故障可以拒绝该候选并尝试 index 中的 rollback-safe。index 本身无法可信解释、世界/权威身份冲突或所有允许候选都不可恢复时，整体失败关闭，不能把所有错误无条件当作可回退故障。

候选选择与验证期间不得附加可写持久化配置、写入新 generation、启动调度、调用 provider 或补发业务效果。只有完整候选选定后才可安装并进入既有恢复流程；World、journal、receipt、nonce 和 authoritative recovery metadata 必须属于同一代，禁止拼代或从未索引的可变兼容 JSON 补齐。结果须可辨认实际 generation、是否回退及原因。

**独立验收 G3：** 普通与 authoritative 两入口分别覆盖 latest 独占的 snapshot/journal CAS 对象缺失或损坏、后续装配/World 构造失败；健康 latest 优先，健康 rollback-safe 能被完整选中且 metadata 同代，候选探测无写入或业务副作用。共享必要对象损坏、两代皆坏及不可解释 index/identity 冲突仍失败关闭；保留既有 archive 与兼容读取负例。对应[设计 G3](runtime-storage-footprint-governance.design.md#storage-generation-g3-design)。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root generation 合同](../prd.md#runtime-generation-persistence-contract) 与 [root generation receiver](../design.md#runtime-generation-persistence-design)，仅承接完整同代候选的只读材料校验与恢复连续性。材料可读不等于恢复到当前 head、非空事件后缀续跑或重新 serving。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-generation-cross-boundary"></a>
#### 提交判定与恢复材料选择不可互相代替

G1 回答“本次精确候选是否提交”，G3 回答“哪一代材料现在完整可读”。B 已有提交证据而只有 A 可读时，选择 A 不证明 B 未提交，也不能抹去 B 的已确认 receipt、余额、nonce 或历史。健康旧候选只允许作为恢复材料；当前写权还须通过[根恢复闸门](../prd.md#runtime-recovery-acceptance)核对同世界、当前 authority/head、版本和单调连续性。

**交叉验收 G1×G3：** 构造新代已提交但只能回退旧代的样例，保存判定不得因回退结果变为 `NotCommitted`，调用方不得从旧业务状态继续写；恢复材料可读、只读恢复与可服务分别呈现，未通过既有闸门时不产生新权威效果。对应[交叉设计](runtime-storage-footprint-governance.design.md#storage-generation-cross-boundary-design)。G1/G2/G3 的确定性故障与交叉断言进入 `test_tier_required`，`test_tier_full` 保留进程重启/中断与实际存储环境的组合验证；文档检查不代签这些执行结果。

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)、[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root generation 合同](../prd.md#runtime-generation-persistence-contract) 与 [root generation receiver](../design.md#runtime-generation-persistence-design)，仅承接提交判定与恢复材料选择独立、已知提交/未知状态不被健康旧候选替代。本地保存与只读候选不代签正式提交证明或同世界服务闸门。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本专题不引入新的 AI 推理链路）。
- Evaluation Strategy: 以磁盘占用、恢复一致性、GC 引用完整性、状态接口可观测性为主，避免使用主观评价指标。

## 4. Technical Specifications
- Architecture Overview:
  - 当前运行态实际形成三套相互关联的数据：
    1. `execution_bridge` 的高度级 CAS 历史（`output/chain-runtime/<node_id>/store`）。
    2. `execution_world` 当前 head + `.distfs-state` sidecar（`reward-runtime-execution-world`）。
    3. `node-distfs` 的复制热数据与冷索引（`output/node-distfs/<node_id>`）。
  - 2026-03-08 的本地样本显示：约 `2102` heights 时，`store` 为 `1.18 GiB`、`reward-runtime-execution-world/.distfs-state/blobs` 为 `635 MiB`、`snapshot.json` 为 `2.2 MiB`，其中 `tick_consensus_records` 单项约 `1.67 MiB`；而当前 sidecar 最新 manifest 实际只引用约 `1.55 MiB` 的 blob。这说明问题主要来自“历史版本无限保留”与“当前 sidecar 缺少 sweep”。
  - 目标架构明确采用“canonical replay log 为真、checkpoint 为跳点、latest snapshot 为缓存”的持久化层次，把数据划分为六类：
    - `Recoverable Head`: latest state 恢复必需数据，永远 pin。
    - `Hot Window`: 最近连续若干高度的完整历史，供调试和短回放。
    - `Sparse Checkpoints`: 低密度保留的历史快照，作为回放跳点。
    - `Canonical Replay Log`: 可重建 retained heights 的权威输入日志，GC 不得破坏；正式范围为全部合法高度，并须保留完整原执行与恢复依赖。
    - `Cold Metadata`: 小体积索引/记录，允许长期保留；正式归档还须保存其实际引用内容，不能仅保留元数据或 hash。
    - `Orphan / Unpinned Data`: 已证明不再被任一有效 generation、完整历史、在途恢复或合法读取依赖的数据，允许 GC；热窗口或固定数量不是该证明。
- Integration Points:
  - `crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs`
  - `crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/mod.rs`
  - `crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime.rs`
  - `crates/oasis7_runtime/src/world/persistence.rs`
  - `crates/oasis7_runtime/src/snapshot.rs`
  - `crates/oasis7_node/src/replication.rs`
  - `crates/oasis7_cas/src/store.rs`
  - `doc/world-runtime/runtime/runtime-integration.md`
  - `doc/world-runtime/prd.md`
- Edge Cases & Error Handling:
  - 保存新 generation 时磁盘写满：确认未提交才中止切换并保留旧 World；已提交则安装/恢复对应 World 并独立报告维护失败，无法确认则按 [G1](#storage-generation-g1)隔离写入并核对。`last_gc_result` 只描述 GC，不能代替保存结果或把已提交降格为 `save_failed`。
  - GC 扫描过程中发现 latest ref 缺失：立即停止 sweep，状态置为 `degraded`，禁止继续删除。
  - execution record、checkpoint 或 replay log 引用已被外部手工删除：重启或指定高度回放时必须给出结构化错误；仅供本地诊断的 legacy/bootstrap 路径可显式显示“仅 latest state 恢复 / retained-height replay 不可用”，不得静默成功。正式世界不因选择 `dev_local` 获得该服务豁免，完整闭包和 proof 未通过时禁止重新正式 serving。
  - profile 从 `soak_forensics` 切回 `dev_local`：必须完整重算 pin set，并保留正式世界全部历史与依赖根；只有替代持久表示已验证可取回时才能释放相应本地重复表示。新预算无法满足时报告受限并停止不安全维护或新写入准入，不缩窄正式历史。
  - `tick_consensus_records` 归档索引损坏：只有该索引不属于所选本地候选必要闭包且候选通过 [G3](#storage-generation-g3) 时，才可继续 latest state 的本地只读诊断加载；审计查询返回结构化错误并要求修复。正式世界完整历史闭包、proof 及现行服务闸门未验证前，不得据此重新正式 serving。
  - `replication_commit_messages` 热数据降冷时失败：保留热文件，不得先删后写。
  - 多进程误指向同一 `output/chain-runtime/<node_id>`：保留现有单 writer 假设，本阶段仅要求检测并阻止第二写者，不负责多写者并发协调。
- Non-Functional Requirements:
  - NFR-1: `dev_local` / launcher 默认 profile 在 `2500` heights 样本下，`output/chain-runtime/<node_id>` 总占用 `<= 384 MiB`，其中 `store <= 256 MiB`。
  - NFR-2: sidecar manifest-aware GC 的单次 sweep `p95 <= 500 ms`（`2500` heights 样本、本地 SSD 环境）。
  - NFR-3: latest-state restart 在默认 profile 下 `p95 <= 5 s`。
  - NFR-4: footprint 优化不得降低回放/恢复确定性；相关一致性回归必须保持 `100%` 通过。
  - NFR-5: retention policy 保留范围内的目标高度回放成功率必须为 `100%`，且重建结果 `execution_state_root` 与原记录一致；正式持久世界的范围为全部合法高度，本地窗口验收不得外推为该完整范围的证据。
  - NFR-6: storage metrics 采集与发布频率不高于每 `1 s` 一次，额外 CPU 开销 `<= 5%`（本地单节点样本）。
  - NFR-7: `soak_forensics` profile 明确允许更高磁盘占用，但必须通过 metrics 暴露实际增长并可配置上限。正式世界触及任一 profile 上限时应保留材料并限制新写入，不得通过删除唯一历史满足预算。
- Security & Privacy:
  - GC 只能在 runtime 自己的 `output/chain-runtime/<node_id>`、`output/node-distfs/<node_id>` 根下运行，不得支持任意路径删除。
  - 删除决策必须基于显式 pin 集和 manifest/record 引用计算，不允许“按文件时间猜测删除”。
  - 当前已提交 index 的 latest、rollback-safe、checkpoint 与外部 pin 完整闭包必须在替代 index 确认持久提交前保持可读；提交未知时还保留候选闭包，避免中间态造成不可恢复损坏。
  - metrics/status 仅暴露体积、引用、结果与错误，不新增敏感 payload 导出。

<a id="storage-observation-contract"></a>
### 存储指标与独立 Storage Gate 合同

本节承接 PRD-WORLD_RUNTIME-015 的可观测输出，设计见
[指标归约与 Gate 终态](runtime-storage-footprint-governance.design.md#storage-observation-design)。
它不改变 GC、提交、checkpoint 或恢复机制，不新增发布前置。

- STORAGE-AC-1：bytes_by_dir 保留各目录递归值；total_bytes 按实际目录集合的并集去重，默认嵌套、外置 override 和同目录多字段均只计一次。缺少路径关系的旧字段必须给不可确定/不完整诊断，不能硬编码忽略子目录或把未知当 0。
- STORAGE-AC-2：只有明确 `last_gc_result=failed` 产生 storage_gc_failed/critical。success、not_run、not_available、unknown 和缺字段分别保留含义；非失败不伪造失败，也不自动证明健康。degraded_reason/orphan 的独立异常不得丢失。
- STORAGE-AC-3：worker、status 与持久 JSON 对 effective interval 使用同一实际配置；必要时区分 requested/default/effective，不能把声明值当实际调度。首次采集、门槛和慢采集不重叠分别验证，degraded 通知独立更新。
- STORAGE-AC-4：独立 `oasis7-runtime-storage-gate.sh` 普通评估先保存完整 JSON/Markdown，再令 PASS=0、FAIL 非零；终端展示状态和报告路径。无效输入/读取/写报告失败仍非零，不能由最后一次 echo 掩盖，也不能吞异常以制造报告。
- STORAGE-AC-5：dry-run 只预览配置，不读 status 源、不发请求、不运行评估或 GC。省略 expected-profile 及三个合法 profile 均能输出类型正确的 JSON 与完整 Markdown；预览成功不表示存储通过。

验收覆盖嵌套并集、GC 六类状态、30000/15000/5000ms 实际配置、profile/checkpoint/orphan/degraded/GC
各负例与健康正例；普通 Gate 和 dry-run 分别记录进程退出码、JSON、Markdown 与 stderr。
观测报告、独立 CLI gate 与真实 release gate 接线不是同一个验收，不能用其中一项通过代替另一项。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03): 引入 storage profile、execution-world sidecar manifest-aware GC、基础 metrics 输出，先解决 latest head 与孤儿 blob 无界增长。
  - v1.1: 实现 execution bridge head window + sparse checkpoint retention，减少 `store` 历史 snapshot 累积。
  - v2.0: 完成 `tick_consensus_records` 热冷分层与 archive index，建立 launcher/soak 统一 footprint gate。
- Technical Risks:
  - 风险-1: 过于激进的 retention 导致历史 record 仍存在但其 `snapshot_ref` 已不可读。
    - 缓解：先定义显式 retention class，再用 pin-set sweep；dangling-ref 测试列为 required。
  - 风险-2: sidecar GC 与 save_to_dir 原子性不足，可能在异常退出后留下不可恢复状态。
    - 缓解：采用 generation pin + rollback-safe 的两阶段切换；替代 index 提交前保留旧闭包，提交未知时保护旧、新集合，`keep=2` 仅约束成功清理后的稳态。
  - 风险-3: `tick_consensus_records` 压缩破坏链路校验与审计查询。
    - 缓解：保留 archive anchor/hash chain，验证 archive read + verify 路径。
  - 风险-4: 默认 profile 与 soak profile 口径分裂，导致脚本/launcher/运行时行为不一致。
    - 缓解：用统一 `storage_profile` 枚举和 status 输出；脚本/launcher 只透传，不自定义语义。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-WORLD_RUNTIME-013 | TASK-WORLD_RUNTIME-030/031/032 | `test_tier_required` | 构造 `>= 2500` heights 的 execution-bridge 样本，验证清理收敛后的 `store`/sidecar 体积上限、pin set 完整性、无 dangling refs；独立判定 [G2](#storage-generation-g2) 的提交前闭包保护 | execution bridge、world persistence、CAS blob retention |
| PRD-WORLD_RUNTIME-014 | TASK-WORLD_RUNTIME-030/031/032/033 | `test_tier_required` + `test_tier_full` | latest-state restart、checkpoint replay、GC fail-safe 注入、保留高度回放校验、soak profile 恢复验证；分别判定 [G1](#storage-generation-g1)、[G3](#storage-generation-g3) 与[交叉边界](#storage-generation-cross-boundary) | 重启恢复、审计与长跑取证 |
| PRD-WORLD_RUNTIME-015 | TASK-WORLD_RUNTIME-031/032/033 | `test_tier_required` | status/metrics 输出字段测试、profile 参数透传测试、snapshot size regression、archive 读取验证；核对 [G1](#storage-generation-g1) 保存结果与维护结果独立、实际 generation 与写入隔离可读 | 可观测性、launcher/脚本配置与治理口径 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-RSF-001 | 默认 profile 仅保留 latest head + 热窗口 + 稀疏 checkpoint | 每个 committed height 永久保留完整 snapshot/journal refs | 当前本地样本证明全量历史保留会在短时运行中产生 GiB 级膨胀，不适合作为默认链路。 |
| DEC-RSF-002 | sidecar 采用 manifest-aware pin/sweep | 每次 save 前清空 `.distfs-state` 目录重写 | 直接清空在异常退出时有更高恢复风险，也无法保证 rollback-safe 恢复。 |
| DEC-RSF-003 | `tick_consensus_records` 热冷分层并保留 archive anchor | 直接禁止记录 tick consensus 历史 | 会损失审计与链路验证能力，不符合 runtime 可追溯目标。 |
| DEC-RSF-004 | 采用 profile-based storage policy（`dev_local` / `release_default` / `soak_forensics`） | 单一全局 retention 常量 | 开发、启动器、长跑取证三类环境的预算差异显著，需要明确可观测的模式治理。 |
| DEC-RSF-005 | canonical replay source 采用变更日志（commit log）+ checkpoint，snapshot 仅作为加速缓存 | 继续把每高度完整 snapshot 作为默认真相源 | 现有样本证明“每高度全量快照”体积不可控；日志为真更符合可回放要求。 |

现行解释（2026-10-17）：以上历史 diagnosis 与 DEC 原文保留。DEC-RSF-001 的“全量历史”及旧样本中的“历史版本无限保留”不构成删除正式逻辑历史的授权；需区分保留完整原历史与逐高度重复保存展开快照。后者可按已验证的等价持久表示收敛，前者仍按[正式历史边界](#storage-profile-history-boundary)完整保留，不能由预算或 profile 重定义。

## 当前合同、历史样本与上游接受
2102 heights 的 2026-03-08 占用和 2026-05-15 压缩补充保留为历史 diagnosis/decision，不能当本轮当前测量；SC/AC/NFR 的 2500 heights、budget、p95/CPU/频率仍是独立有条件目标，§6 旧 tasks/decisions 是 provenance，不刷新为新执行证据。
本合同接受 [DCS-002 同世界恢复](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 的本地 material/replay 子义务、[DWE-004 历史版本](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) 的 retained manifest/artifact 支持，以及产品根 SC-2/3/6/7 的 pin/reconstruction 子义务。保留高度 H 的 canonical log、checkpoint、原版本/hash/module binding 必须唯一解析，依赖未验证不能 aggressive sweep。准确设计见 [storage history](runtime-storage-footprint-governance.design.md#storage-history-dependency)。
schema v3+V1/V2 compatibility、last_applied 连续性/current local binding 与 R0 latest/R1 retained 保证分开；这些本地能力不证明正式全部历史，root equality/启动也不证明 finalized checkpoint、redundant archive、light proof/DA 或可投票/写入。重新服务另走 [root gate](../design.md#runtime-recovery-design)，ops/P2P 提供同窗口 proof/role/archive evidence；本地存储 profile 不改变产品 world identity 或迁移权限。正式保留范围与优化后的完整证据按[新产品要求](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001)及[专业设计](formal-world-storage-efficiency.design.md#des-lws-001)承接。各 SC/AC/NFR 的独立验证及未运行范围见配对 design 的新增 §11.1，旧 §16/历史 trace 保留。

R1 的 committed-input log 重执行与 `WorldJournal` 事件后缀归约分别取证；完整 snapshot 往返也不能代替后者。
恢复结果拟用于继续执行时，另须满足 [事件后缀与续跑合同](../prd.md#runtime-journal-suffix-recovery) 对适用路径的控制状态完备性和正向对照；AC-3 的 root/journal_len/registry 与 AC-5 的 retained root 检查仍保留，不能单独证明 allocator/schedule 已正确续接。

## 存储分项接受定位
下列定位分别保留旧SC/AC/NFR/规则条件；预算均为目标，未由本轮实际测量证明；原DEC-RSF和迁移历史不变。

<a id="storage-accept-ac1"></a>
#### AC-1：体积预算与引用完整性
- AC-1/SC-1：2500 committed heights llm_bootstrap dev/launcher store≤256 MiB，record引用无dangling。该独立条件由配对设计 storage-case-ac1 与其验证场景承接。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design)，仅承接record 引用与 retained material 不 dangling 的材料完整性子义务。2500 heights 与256 MiB是本专题独立工程预算，不是产品恢复验收通过的替代。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-ac2"></a>
#### AC-2：清理收敛与暂态材料保护
- AC-2/SC-2：提交确认且清理成功后收敛到latest+rollback-safe及外部pin引用集合，sidecar≤16 MiB，orphan=0；暂态/降级保护额外数据。该独立条件由配对设计 storage-case-ac2 与其验证场景承接，保存结果与预算收敛分别判定。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design)，仅承接确认提交后 retained/pin 材料保护及暂态/未知不提前删除的子义务。16 MiB/orphan稳态预算与清理收敛仍独立判定，不代签全部恢复链。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-ac3"></a>
#### AC-3：GC 后 latest restart 对账
- AC-3/SC-4：GC后latest restart，execution root/journal_len/module_registry相同。该独立条件由配对设计 storage-case-ac3 与其验证场景承接。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design)，仅承接GC 后 latest restart 的 execution root/journal_len/module_registry 对账子义务。不代签非空事件后缀控制状态续跑或重新 serving。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-ac4"></a>
#### AC-4：故障与未知状态保护
- AC-4：GC中断/partialwrite/pin失败保当前index的latest+rollback-safe完整闭包，degraded保恢复；提交未知还保护候选并隔离旧World写入。该独立条件由配对设计 storage-case-ac4 与其验证场景承接。

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)、[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root generation 合同](../prd.md#runtime-generation-persistence-contract) 与 [root generation receiver](../design.md#runtime-generation-persistence-design)，仅承接G1未知保护与调用方fence、当前index及候选材料保护的子义务。满盘/中断/pin失败等独立故障仍分别验收，不以单一恢复成功代签。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-ac5"></a>
#### AC-5：保留高度重建
- AC-5/SC-6：每 policy-retained H 由 nearest C<=H + canonical log 重建原 root；正式世界的 H 为全部合法高度，本地窗口样本仍只证明原局部范围。该独立条件由配对设计 storage-case-ac5 与其验证场景承接。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design)，仅承接policy-retained H 的 checkpoint 与 canonical committed-input log 重建子义务。committed-input 重执行不是非空 WorldJournal 事件后缀正向续跑。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-ac6"></a>
#### AC-6：存储观测字段
- AC-6/SC-5：profile/effective_budget/bytes/retained/checkpoint/replay/lastgc/error/degraded可读，无额外扫描。该独立条件由配对设计 storage-case-ac6 与其验证场景承接。

<a id="storage-accept-ac61"></a>
#### AC-6.1：profile 入口与 wrapper 透传
- AC-6.1：四入口同枚举无损透传，三wrapper OASIS7_CHAIN_STORAGE_PROFILE非空注入/默认继承/bundle runtime。该独立条件由配对设计 storage-case-ac61 与其验证场景承接。

<a id="storage-accept-ac7"></a>
#### AC-7：快照预算与归档校验
- AC-7/SC-3：2500 heights snapshot≤512KiB，archive区间读取+链路校验。该独立条件由配对设计 storage-case-ac7 与其验证场景承接。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design)，仅承接热窗口外历史材料经archive读取并校验链路的子义务。2500 heights与512 KiB快照预算仍为独立工程条件，不扩大为完整恢复。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-ac8"></a>
#### AC-8：独立验证矩阵
- AC-8：required/full分别含footprint/restart/failsafe/profile/archive/retained replay矩阵，并独立判定G1/G2/G3及交叉边界。该独立条件由配对设计 storage-case-ac8 与其验证场景承接。

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)、[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root generation 合同](../prd.md#runtime-generation-persistence-contract) 与 [root generation receiver](../design.md#runtime-generation-persistence-design)，仅承接本行独立G1/G2/G3及交叉矩阵所对应的持久结果/调用方一致性与同代材料恢复子义务。footprint/profile/archive及required/full其余矩阵仍独立；引用不是整组产品验收或回归通过。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-sc7"></a>
#### SC-7：hot/checkpoint 保留密度
- SC-7：release_default hot_head_heights≤checkpoint_interval，不重复保留额外同区间 snapshot；只有完整等价恢复已验证后才减少原表示，密度目标不授权删唯一历史。该独立条件由配对设计 storage-case-sc7 与其验证场景承接。

<a id="storage-accept-nfr1"></a>
#### NFR-1：稳态空间预算
- NFR-1：2500 heights node root≤384MiB且store≤256MiB。该独立条件由配对设计 storage-case-nfr1 与其验证场景承接。

<a id="storage-accept-nfr2"></a>
#### NFR-2：sweep 性能预算
- NFR-2：2500 heights localSSD sweep p95≤500ms。该独立条件由配对设计 storage-case-nfr2 与其验证场景承接。

<a id="storage-accept-nfr3"></a>
#### NFR-3：latest restart 性能预算
- NFR-3：default latest restart p95≤5s。该独立条件由配对设计 storage-case-nfr3 与其验证场景承接。

<a id="storage-accept-nfr4"></a>
#### NFR-4：优化不降低恢复确定性
- NFR-4：优化不能降低replay/recovery确定性，相关回归100%。该独立条件由配对设计 storage-case-nfr4 与其验证场景承接。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design)，仅承接体积优化不降低已有replay/recovery确定性的子义务。相关回归100%仍需同候选实际执行，不由静态关系证明。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-nfr5"></a>
#### NFR-5：保留高度回放完整率
- NFR-5：所有 policy-retained heights replay 成功 100% 且原 root 相等；正式世界包括全部合法高度，不能由 storage profile 缩窄。该独立条件由配对设计 storage-case-nfr5 与其验证场景承接。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design)，仅承接 policy-retained heights 重建原 root 的子义务。100% 与每个保留目标 H 仍独立验证；legacy/bootstrap 局部证据不外推为深历史或 serving，正式完整历史承诺仍按[适用边界](#storage-profile-history-boundary)成立。原独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-nfr6"></a>
#### NFR-6：采样频率与 CPU 预算
- NFR-6：metrics采集发布频率不高于每1s一次，额外CPU≤5% local单节点。该独立条件由配对设计 storage-case-nfr6 与其验证场景承接。

<a id="storage-accept-nfr7"></a>
#### NFR-7：soak 资源上限
- NFR-7：soak显式更高占用、metrics增长/可配上限。该独立条件由配对设计 storage-case-nfr7 与其验证场景承接。

<a id="storage-accept-compression"></a>
#### 原始内容与透明压缩
- 原payload content_hash不变，compressed header后更小才压缩，读回rawbytes。该独立条件由配对设计 storage-case-compression 与其验证场景承接。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design)，仅承接压缩前后原始payload内容hash不变且读回原bytes的hash-bound材料完整性子义务。是否采用压缩及header后更小的编码策略仍为工程条件，不代签恢复正确性。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-cold-range"></a>
#### 冷热范围与归档索引兼容
- continuous latest hot range/shared coldanchor scan seek同边界，pack len/offset/hash与canonical/legacy别名回填。该独立条件由配对设计 storage-case-cold-range 与其验证场景承接。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design)，仅承接retained历史材料的连续范围、anchor及pack读取校验子义务。scan/seek同边界和canonical/legacy别名回填各自独立，目录或索引一致不证明完整恢复。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-legacy-height"></a>
#### record 兼容与高度连续性
- v3新写/V1V2兼容，stale/noncontiguous heightfailclosed；legacy仅latest-safe无aggressiveGC。该独立条件由配对设计 storage-case-legacy-height 与其验证场景承接。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design)，仅承接本地retained record兼容读取与stale/noncontiguous高度失败关闭的连续性子义务。v3/V1V2兼容与legacy仅latest-safe不获得完整authority/readiness。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-save-failure"></a>
#### 保存故障与提交三态
- save满盘/新generation中断按G1区分未提交、已提交和未知；保护恢复材料不恢复旧写权，维护失败不撤销提交；pin/latestmissing停止，replay mismatch阻aggressiveGC。该独立条件由配对设计 storage-case-save-failure 与其验证场景承接。

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)、[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root generation 合同](../prd.md#runtime-generation-persistence-contract) 与 [root generation receiver](../design.md#runtime-generation-persistence-design)，仅承接G1保存三态/调用方fence与恢复材料保护的子义务。保存、维护、恢复结果分别判定；不抹去满盘、未知、pin缺失或replay mismatch的独立失败条件。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-single-writer"></a>
#### 写者、目录与敏感数据边界
- 仅runtime指定根删除，显式pin/无mtime猜测，第二writer检测阻止、metrics不新增payload。该独立条件由配对设计 storage-case-single-writer 与其验证场景承接。

<a id="storage-accept-version-dependency"></a>
#### 历史版本依赖保护
- retained原manifest/artifact/moduleanchor唯一解析，missing/hashconflict停止unsafe sweep/不称matched。该独立条件由配对设计 storage-case-version-dependency 与其验证场景承接。

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) 经 [root version receiver](../design.md#runtime-version-design)，仅承接retained原manifest/artifact/module binding唯一解析及缺失/冲突时停止unsafe sweep的子义务。完整activation ledger/proof、跨版本replacement及版本窗口组合仍未证明。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。

<a id="storage-accept-service-boundary"></a>
#### 本地恢复与服务权限边界
- localroot/restart仅历史条件，append/finality/version/head与redundantarchive仍外部。该独立条件由配对设计 storage-case-service-boundary 与其验证场景承接。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root recovery receiver](../design.md#runtime-recovery-design)，仅承接本地root/restart仅是恢复材料条件以及外部root gate的证据边界。同世界服务、追加/最终性/version/head和redundant archive仍由既有外部receiver与ops/runtime/QA按原test_tier_full同窗口判定。原条文、独立验收和未证明范围不变；本引用不改变产品状态，不新增实现或执行证据。
