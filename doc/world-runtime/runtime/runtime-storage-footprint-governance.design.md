# oasis7 Runtime：执行桥接与运行态存储体积治理（详细技术设计）

审计轮次: 4

- 上游 PRD：[运行态存储治理及 profile/历史边界](runtime-storage-footprint-governance.prd.md#storage-profile-history-boundary)。
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。
- 适用范围: `oasis7_viewer_live` / `oasis7_chain_runtime` 默认运行链路下的 execution bridge、execution world sidecar、node-distfs 复制热数据与 footprint metrics。
- 正式世界边界：[完整历史要求](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001)及[无损存储专业设计](formal-world-storage-efficiency.design.md#des-lws-001)约束本专题的正式持久路径。以下 legacy/local bootstrap 样本、字段和局部验收保留原范围；本次边界澄清不表示现有实现已具备完整正式历史回收或恢复能力。

## 1. 设计目标
- **Replayability-first**：体积治理不能破坏“可追溯、可回放”；必须明确 canonical replay source、checkpoint 语义与 GC 边界。正式持久世界保留全部已承诺历史高度及其完整依赖，storage profile 的热窗口不能缩小该范围。
- **Latest-state recoverability**：任意时刻成功提交后的 latest head 必须可恢复，且恢复后 `execution_state_root` 与 GC 前一致。
- **Bounded default footprint**：默认 `dev_local` / launcher 的热数据、重复表示与临时材料按适用预算治理；完整历史的真实新增信息仍会增长，不承诺正式世界总存储恒定，也不以达到预算为由删历史。
- **Profile-governed retention**：`dev_local`、`release_default`、`soak_forensics` 通过同一套 storage profile 控制热工作集、恢复加速密度与诊断用量；它们不是 authority profile，不决定世界身份、提交权或历史保留承诺。
- **Derived data is disposable**：只有已经证明存在完整等价恢复路径、且不再被历史、未决操作、读者或回滚依赖的物理表示，才可作为派生材料回收；snapshot、journal 或 sidecar 的类型名本身不能证明可删除。

## 2. 非目标
- 不改变共识协议、区块签名语义或 DistFS challenge 业务规则。
- 本专题的原本地改造不直接引入远端对象存储、外部数据库或跨节点中心化归档服务；正式无损分层及独立故障域合同由[配套设计](formal-world-storage-efficiency.design.md#des-lws-005)拥有。
- 不要求每个历史高度都有一份本地展开全量 `snapshot_ref`。legacy/local bootstrap 的已有验收只证明其实际保留范围；正式世界无论选何 storage profile，仍须保留全部承诺历史及原依赖内容，不能用该局部证据降低承诺。
- 不尝试在本轮清理 `target/`、bundle、浏览器缓存等非运行态产物。

## 3. 现状诊断

下列体积样本和初始写法是本方案形成时的历史诊断，不代表当前测量；当前 generation 故障边界
按 §§8、13、16 的固定基线与目标合同分别判断，不能把已有 index/sweep 当作完整原子性证明。
这些样本及下文 `2500` heights、store/sidecar/node-root 预算只约束原负载、目录和配置，不是正式端点、独立副本与冷归档的全盘容量上限，也不证明当前正式提交仍沿相同写法。

### 3.1 当前三套主要数据
- `output/chain-runtime/<node_id>/store`
  - execution bridge 的本地 CAS 仓库。
  - 目前每个 committed height 都写入一份 `snapshot_ref` 和一份 `journal_ref`。
- `output/chain-runtime/<node_id>/reward-runtime-execution-world`
  - current head 的 `snapshot.json` / `journal.json`。
  - 另含 `.distfs-state/blobs`，按 256 KiB 默认 chunk 大小写入 sidecar blob。
- `output/node-distfs/<node_id>`
  - 复制热消息、commit 镜像、`files_index.json` 和本地 blob store。

### 3.2 已观测问题
- `store` 中约 `2102` 个唯一 `snapshot_ref` 累积约 `1175.58 MiB`，而 `journal_ref` 仅约 `7.70 MiB`。
- `reward-runtime-execution-world/.distfs-state/blobs` 累积约 `635.28 MiB`，但 latest manifest + journal segments 实际只引用约 `1.55 MiB`。
- `snapshot.json` 达到 `2.2 MiB`，其中 `tick_consensus_records` 单项约 `1.67 MiB`。
- `replication_commit_messages` 目前仍是热窗口文件；只有超过默认 `4096` 条后才下沉到 cold index。

### 3.3 根因
- execution bridge 把“每高度完整快照”当作默认持久化语义，而不是“checkpoint + replay log”。
- 初始 sidecar 缺少 generation-aware sweep；现有 index/sweep 仍须满足本设计的提交结果、旧引用保护与完整候选读取合同。
- snapshot 把 `tick_consensus_records` 全量驻留在热状态，而不是热冷分层。
- 复制层的 hot/cold 保留策略与 execution bridge / sidecar 不统一，状态接口也没有直观暴露预算与实际增长。

## 4. 设计原则与回放契约
### 4.1 Canonical replay source
本方案按原权威和可验证的替代关系区分保留内容与派生表示；下面列举原本地路径，不是正式世界完整依赖清单：

- **权威输入日志（必须保留）**
  - `consensus/commits/<height>.json` 对应的 commit 内容与顺序。
  - 影响确定性执行的外部输入必须在 commit 前物化为 committed payload，或以“commit-height -> external-effect receipt”形式被权威引用。
  - 模块工件原字节及 hash / manifest / active version 等执行依赖；只有 hash 或版本名不等于保存了内容。
- **可证明等价后才允许回收的表示**
  - 每高度 `snapshot_ref` / `journal_ref`。
  - `reward-runtime-execution-world/.distfs-state/blobs` 的旧 generation chunk。
  - 热提交消息镜像、快照内热窗口外的 `tick_consensus_records`。

正式提交中的 snapshot/journal、原结果、历史 native/WASM 工件、控制链、证明、未决输入和外部效果账本，可能是原历史或续跑的唯一材料；其闭包按[正式保留根](formal-world-storage-efficiency.design.md#des-lws-001)和[安全回收](formal-world-storage-efficiency.design.md#des-lws-004)逐项判断，不能仅因它们可由执行过程产生就归为可删缓存。

### 4.2 Replay contract
- **R0 Latest Recovery Contract**
  - latest successful committed height 必须总是可以直接恢复。
  - latest head 相关 manifest / snapshot / journal / module refs 永远 pin。
- **R1 Retained Replay Contract**
  - legacy/local bootstrap 的原验收仍按其明确保留的目标高度 `H` 核对。“retained”在正式持久世界覆盖所有已承诺历史高度，不随 storage profile、热窗口、checkpoint 数量或本机角色缩小。
  - 每个适用 `H` 必须存在完整原 snapshot 或经过验证的 `C <= H` checkpoint 加 `(C+1..H)` 原始内容与依赖路径；原根匹配、控制状态、原字节/证明和新工作续跑分别按适用恢复合同核对。不能把普通 Journal 或 committed-input 日志直接当作已证明完整的状态 delta。
- **R2 Audit Contract**
  - 即使不保留每高度展开全量 snapshot，也保留定位与核对所需 metadata：`height`、`execution_state_root`、`execution_block_hash`、`journal_len`、`checkpoint_anchor`、`commit_ref`。这些索引必须能取回原 payload 及其完整依赖；metadata、hash 或首尾高度不足以证明历史内容仍完整，不能替代原文。
- **R3 Fail-safe Contract**
  - 任意 GC、compaction、generation 切换失败都不得破坏 R0–R2 的适用保留与恢复条件；正式世界完整历史不得因维护失败被删减。闭包、原内容或证明不可验证时停止相应回收，保留已知/未知材料并显式 `degraded`；正式 serving/new confirmation 仍受原恢复闸门约束，不能以 latest 可读带过缺证。

### 4.3 Profile matrix
下列均为 storage profile，与权威、共识和独立故障域 profile 分开。三档只改变物理放置、热密度和读取成本；正式世界仍保留全部承诺历史，原局部 fixture 不证明这一全局能力已实现。

| Profile | 目标 | 本地展开快照 | Checkpoint 加速密度 | 日志与原依赖保留 | 适用场景 |
| --- | --- | --- | --- | --- | --- |
| `dev_local` | 控制热/临时用量 | latest head + 短 hot window | 稀疏 checkpoint，删除需满足 §9.3 | 全部适用 R1 原内容及依赖 | 本地开发、启动器；名称不授予删正式历史权限 |
| `release_default` | 控制恢复成本 | latest head + 较长 hot window | 规则化 checkpoint，删除需满足 §9.3 | 全部适用 R1 原内容及依赖 | 单节点/发行的存储配置；不证明正式持久资格 |
| `soak_forensics` | 增加取证材料 | latest head + 长 hot window | 高密度或全量 checkpoint | 全部适用 R1 原内容及依赖 + 更多审计输出 | 长跑、事故复盘；额外副本可增多，原历史不缩短 |

## 5. 数据分类与目录方案

### 5.1 目录角色
建议保持现有根路径不变，在其下新增清晰子角色：

```text
output/
  chain-runtime/<node_id>/
    reward-runtime-execution-world/        # latest head + sidecar
    reward-runtime-execution-records/      # 每高度 metadata index
    execution-checkpoints/                 # 稀疏 checkpoint manifest/index
    execution-archives/                    # tick consensus archive/index
    store/                                 # execution bridge CAS (仅保留 pinned blobs)
    reward-runtime-state.json
    reward-runtime-execution-bridge-state.json
    reward-runtime-storage-metrics.json
  node-distfs/<node_id>/
    replication_commit_messages/           # hot commit mirror
    replication_commit_messages_cold_index.json
    replication_commit_messages.cold-index/
      index.json
      segments/                           # fixed height-range packed cold commits
    store/                                 # distfs blobs + legacy cold commit compatibility blobs
```

### 5.2 数据分层
- `reward-runtime-execution-world/`
  - 只表示 **latest recoverable head**。
  - `snapshot.json` / `journal.json` 继续保留，供快速恢复和人工检查。
  - `.distfs-state` 在提交确认且清理成功后收敛到 latest、rollback-safe generation 及外部 pin；准备新代、提交未知或清理失败期间保留额外受保护材料。
- `execution-checkpoints/`
  - 新增 checkpoint manifest/index，承载可回放的稀疏快照。
  - checkpoint 使用 execution store CAS，和 latest head 的 sidecar 分离，避免 latest directory 无限保留历史 generation。
- `reward-runtime-execution-records/`
  - 保留每高度 metadata，但不要求每条都持有唯一 snapshot/journal blob。
  - 对已有完整替代恢复路径的非 checkpoint 高度，本地 record 可仅持 `checkpoint_anchor`、`commit_ref`、`execution_state_root` 等索引；被引用的原内容仍须可获取，不能留下 hash-only 历史。
- `store/`
  - 保存完整保留根和所有 pins 可达的 blob。latest/hot/checkpoint/archive/module 是本地基础集合；正式路径还必须纳入 §9.2 的完整历史、未决与读者等闭包。

## 6. 核心数据结构
### 6.1 StorageProfileConfig
```rust
struct StorageProfileConfig {
    profile: StorageProfile,
    execution_hot_head_heights: u64,
    execution_checkpoint_interval: u64,
    execution_checkpoint_keep: u64,
    execution_sidecar_generations_keep: u32,
    tick_consensus_hot_limit: usize,
    tick_consensus_archive_segment_len: usize,
    replication_max_hot_commit_messages: usize,
    metrics_emit_interval_ms: u64,
}
```

### 6.2 ExecutionBridgeRecord schema v3 and compatible readers
```rust
struct ExecutionBridgeRecordV3 {
    schema_version: u32,
    world_id: String,
    height: u64,
    node_block_hash: Option<String>,
    prev_node_block_hash: Option<String>,
    proposer_id: Option<String>,
    action_root: Option<String>,
    execution_block_hash: String,
    execution_state_root: String,
    journal_len: usize,
    latest_state_ref: Option<String>,
    snapshot_ref: Option<String>,
    journal_ref: Option<String>,
    commit_log_ref: Option<String>,
    checkpoint_ref: Option<String>,
    external_effect_ref: Option<String>,
    world_head_proof_ref: Option<String>,
    world_head_proof_hash: Option<String>,
    simulator_mirror: Option<ExecutionSimulatorMirrorRecord>,
    timestamp_ms: i64,
}
```

#### 设计说明
- `commit_log_ref` 定位 canonical replay log 中该高度的实际 commit 内容；hash / path 只负责定位与校验，内容及依赖必须保存。snapshot/journal refs 的释放条件以完整保留根及 pin 为准。
- V1/V2 记录可读，缺失字段按 legacy compatibility 处理；新写入为 v3，V2 不再被表述为最终当前 schema。
- `last_applied_committed_height` 是持久 bridge state：恢复后只接受连续的下一高度，stale/non-contiguous 输入失败关闭。checkpoint install/restart 必须重新验证 record refs、canonical log 与 state root。
- world-head proof/ref 只绑定本地 committed context、CAS refs 与 checkpoint manifest；不是 light-client、receipt/state proof、DA sampling、public-testnet 或 mainnet finality 证明。

### 6.3 ExecutionCheckpointManifest
```rust
struct ExecutionCheckpointManifest {
    checkpoint_id: String,
    world_id: String,
    height: u64,
    snapshot_ref: String,
    journal_ref: String,
    execution_state_root: String,
    execution_block_hash: String,
    module_anchor: ModuleAnchor,
    created_at_ms: i64,
}
```

### 6.4 SidecarGenerationIndex
```rust
struct SidecarGenerationIndex {
    latest_generation: String,
    rollback_safe_generation: Option<String>,
    generations: BTreeMap<String, SidecarGenerationRecord>,
    last_gc_result: SidecarGcResult,
}

struct SidecarGenerationRecord {
    manifest_hash: String,
    journal_segment_hashes: Vec<String>,
    pinned_blob_hashes: Vec<String>,
    created_at_ms: i64,
}
```

### 6.5 TickConsensusArchiveIndex
```rust
struct TickConsensusArchiveIndex {
    hot_from_tick: u64,
    hot_to_tick: u64,
    archived_segments: Vec<TickConsensusArchiveSegment>,
}

struct TickConsensusArchiveSegment {
    from_tick: u64,
    to_tick: u64,
    content_hash: String,
    record_count: usize,
    hash_chain_anchor: String,
}
```

### 6.6 SharedColdIndexManifest
```rust
struct StorageColdIndexManifest {
    schema_version: u32,
    namespace: String,
    key_kind: String,
    value_kind: String,
    hot_range: Option<StorageColdIndexRange>,
    cold_range_anchor: Option<StorageColdIndexRangeAnchor>,
}

struct StorageColdIndexRange {
    from_key: u64,
    to_key: u64,
}

struct StorageColdIndexRangeAnchor {
    from_key: u64,
    to_key: u64,
    first_content_hash: String,
    last_content_hash: String,
    entry_count: usize,
}

struct CommitMessagePackRef {
    segment_id: String,
    offset: u64,
    len: u64,
    content_hash: String,
}
```

#### 设计说明
- 共享 cold index 目录采用 `<namespace>.cold-index/index.json`；有分段数据时统一使用同级 `segments/` 目录承载 payload。
- `hot_range` 表示当前仍保留在热路径中的连续键范围；对 replication 即 latest-based height window，对 tick/archive 则可映射为 tick range。
- `replication_commit_messages` 的冷归档默认采用固定高度跨度的 segmented pack 布局：同一高度段内的 commit message 以 `len + payload` 追加进共享 `.pack` 文件，cold index 只保留 `segment_id + offset + len + content_hash`，从而避免“小 commit 一文件”带来的 inode 与块边界放大。
- 旧 `height -> content_hash` 冷索引在第一次读回/维护时会自动迁移到 `CommitMessagePackRef`，并在 pack 索引落盘成功后删除不再被 file index / pin 引用的 legacy CAS blob。
- `cold_range_anchor` 只承载冷区边界锚点而不是全量 payload，最低要求为 `from_key` / `to_key` / `first_content_hash` / `last_content_hash` / `entry_count`，供审计、metrics 与 seek 策略共用；cold-index scan 与按 key seek 的边界必须从同一 anchor 派生。
- rollout 期间 canonical 与 legacy alias 采用双写 + 读时回填策略：若 `<namespace>.cold-index/index.json` 或旧别名缺失，读取路径会补回另一侧，避免已有样本和脚本立即失效。

### 6.7 StorageMetricsSnapshot
```rust
struct StorageMetricsSnapshot {
    storage_profile: String,
    effective_budget: StorageProfileConfig,
    bytes_by_dir: BTreeMap<String, u64>,
    blob_counts: BTreeMap<String, u64>,
    ref_count: u64,
    pin_count: u64,
    retained_heights: Vec<u64>,
    checkpoint_count: usize,
    replay_summary: StorageReplaySummary,
    orphan_blob_count: u64,
    last_gc_at_ms: Option<i64>,
    last_gc_result: String,
    last_gc_error: Option<String>,
    degraded_reason: Option<String>,
}

struct StorageReplaySummary {
    retained_height_count: usize,
    earliest_retained_height: Option<u64>,
    latest_retained_height: Option<u64>,
    earliest_checkpoint_height: Option<u64>,
    latest_checkpoint_height: Option<u64>,
    mode: String,
}
```

- `effective_budget` 直接回显当前 profile 对应的预算口径，避免 launcher / 脚本重复推导。
- `replay_summary.mode` 只允许 `latest_only` / `full_log_only` / `checkpoint_plus_log` 三档，供外部快速判断回放保证等级。

### 6.8 Bundle Wrapper Profile Contract
- `run-game.sh`、`run-web-launcher.sh`、`run-chain-runtime.sh` 只在 `OASIS7_CHAIN_STORAGE_PROFILE` 非空时注入对应 profile 参数。
- 未设置 `OASIS7_CHAIN_STORAGE_PROFILE` 时，wrapper 必须继承底层二进制默认值，避免在 shell 中复制 `dev_local` 等默认常量。
- `run-game.sh` / `run-web-launcher.sh` 必须显式指向 bundle 内 `oasis7_chain_runtime` 二进制，保证 profile 覆盖不会落到外部 PATH 上的其他 runtime。

## 7. 写路径设计
### 7.1 Commit 落账顺序
目标：保证 canonical log 先成立，再生成派生执行数据。本节是原本地 execution bridge 改造次序；正式协议仍遵循其原候选、决策与持久回执，不能据此把当前 formal 全量材料宣称为已经改成稀疏写入。

1. `NodeRuntime` 提交共识高度 `H`。
2. canonical commit log 持久化成功，得到 `commit_ref(H)`。
3. execution bridge 执行 commit，得到 latest `snapshot_value` / `journal_value` / `execution_state_root`。
4. 判断 `H` 是否属于：
   - latest head
   - hot window
   - sparse checkpoint
5. 写入 schema-v3 `ExecutionBridgeRecord`：
   - 所有高度都记录 `commit_ref`、`execution_state_root`、`execution_block_hash`。
   - 只有 hot/checkpoint 高度才写 `hot_snapshot_ref` / `hot_journal_ref`。
6. 更新 latest execution world 目录。
7. 若 `H` 触发 checkpoint，则写 `execution-checkpoints/<checkpoint_id>.json` 并 pin checkpoint refs。
8. 运行 retention manager 重算完整保留根及 pin set；通过 §9 和相应闭包验证后，才对 execution store 做 sweep。
9. 发布 metrics/state。

### 7.2 为什么不能“每高度先写完整快照”
- 历史本地样本显示每高度 snapshot 会增加重复持久用量；不能直接外推当前正式路径的节省量。
- 对 replay 来说，真正必须的是：
  - 最近可恢复 head
  - 足以跳点的 checkpoint
  - 不可丢的 canonical replay log、原结果、版本及完整控制状态依赖
- 减少展开快照的前提是每个依赖它的历史高度仍有已验证完整路径。正式路径先采用[原字节共享](formal-world-storage-efficiency.design.md#des-lws-002)，未来稀疏快照加完整增量须通过[非空后缀与续跑 gate](formal-world-storage-efficiency.design.md#des-lws-007)；当前日志不完整时保留原快照。

## 8. Latest head 与 sidecar generation 设计
### 8.1 Latest head 目录角色
`reward-runtime-execution-world/` 只服务以下目标：
- latest-state 快速重启
- 人工查看当前 `snapshot.json` / `journal.json`
- latest generation sidecar 恢复

<a id="storage-generation-g2-design"></a>
### 8.2 Generation 两阶段切换

承接 [G2 旧引用保护](runtime-storage-footprint-governance.prd.md#storage-generation-g2)。
保留范围由当前**已持久提交的 index**决定，不能提前采用内存中的候选 index：

1. 在既有 single-writer 约束下捕获旧 index 与本次候选身份；保护旧 index 引用的完整闭包，包括 generation manifest、generation-local snapshot/journal payload、可选 recovery metadata、CAS blobs、archive 及必要 module 依赖。
2. 生成新的 manifest / journal segments / pinned blob set，并在 staging 中写入候选及元数据；此时只增加材料，不删除旧 index 仍引用的任何对象。
3. 按 [完整候选读取](#storage-generation-g3-design) 验证新代可独立组装，完成候选文件及引用依赖要求的同步；再次核对旧 head 未漂移。
4. 同步临时 index，原子替换 `latest_generation` 指针/index，并完成其父目录同步，才确认本地 durable commit；新 index 把旧 latest 标为 `rollback_safe_generation`。rename 可见而目录同步失败，按 [G1](#storage-generation-g1-design) 保持未知并保护旧、新两组材料，不运行 sweep。
5. 确认新 index 已提交后，以新 retained set、完整历史根与外部 pin 的并集为依据，回收不再引用的 generation-local 文件、manifest 与 CAS blobs。清理失败只保留多余数据，记录可重试维护结果；未索引的遗留 payload 也须证明不属于历史、未决提交或其他保护闭包才能回收。

A/B 已提交而 C 尚未确认提交时，必须保留旧 index 的 A/B 全部闭包；B 自身损坏不解除 A 的保护。
C 正常提交后，只有 A 不再被完整历史或任何外部 pin 引用，才可按 C/B 的本地 retained set 回收 A。清理与重新计算 pin 失败都不能撤销 C 的提交；保留两代本地加速材料不等于正式世界只保留两代历史。

### 8.3 Sweep 规则
- 允许删除：已确认提交的新 index、完整历史根与外部 pin 均不再引用，且不属于未决提交保护集合、已有验证充分的替代表示或恢复路径的 sidecar blobs 和 generation-local 工件。
- 禁止删除：
  - latest generation 任何 chunk
  - rollback-safe generation 任何 chunk
  - 被任一历史、checkpoint、未决候选、reader、rollback 或迁移根复用的 chunk

<a id="storage-generation-g1-design"></a>
### 8.4 普通保存的提交判定与内存安装

承接 [G1 普通事务结果](runtime-storage-footprint-governance.prd.md#storage-generation-g1)。
owner 为 runtime_engineer；本节及 §§8.2、8.5、8.6 的审读基线为
`eng-cc/oasis7@27db6f894bd106e9a4fee9ee3062cd8fd86e1056`，来源为
[#4452](https://github.com/eng-cc/oasis7/issues/4452) 的独立 G1/G2/G3。以下为目标合同，
现有 ordinary save 与 authoritative recovery 的局部代码/测试不构成完整通过证据。

普通事务在调用持久化前保留待安装 World、原 lease/receipt 返回值与精确候选身份。
候选身份绑定存储/world scope、旧 generation/head、本次 generation id、精确 snapshot manifest
及有序 journal 内容摘要、必要 pin/archive/sidecar 闭包，以及适用 metadata 的摘要或明确缺省。
复用已有序列化、hash、record 和业务 operation key；仅比较 metadata hash、state root、时间或高度
不能代替完整候选核对，generation id 也不替代业务幂等 key。本条不新增磁盘格式或业务 ledger。

持久化层向普通事务调用方提供以下可区分结果；名称表达内部语义，不冻结新的外部 wire enum：

| 提交结果 | 必要事实 | 内存、返回值与后续写入 |
| --- | --- | --- |
| `NotCommitted` | 确认本次未替换持久 index，旧已提交 index/head 仍成立，且没有未决替换 | 丢弃候选，保留旧 World；返回明确未提交的错误，既有幂等 key 可按原规则重试。 |
| `Committed` | 本次精确候选的 index 发布及所需文件/父目录同步已经确认 | 不可失败地安装同一候选，返回原 lease/receipt；同 key 重试返回原结果，后续无关账户保存也必须以该已提交 World 为基准。 |
| `StatusUnknown` | 替换/同步/回读失败，现有证据不能确定本次结果 | 保留原 key、候选及必要材料；在普通持久化发布边界 fence 旧 World，不允许后续 mutator/save 或依赖该未决结果的外部派发越过。核对出确定结果并对齐内存后才解除。 |

提交核对器必须直接核对 index 中本次候选的完整身份及持久确认，不调用可回退 loader 来判断未提交。
index.tmp open/write/sync 等已知发生在替换前的失败可确认 `NotCommitted`；rename 后父目录同步
失败，即使读取到了新 index，也不能立即确认 `Committed`。仍持有同一 writer 权限时，可核对
精确候选并补齐所需同步；同步成功才确认，否则保持 `StatusUnknown`。读到了旧候选也不能单独
证明本次未提交。失去 writer ownership 或 head 身份冲突时停止发布，不通过重写旧 index 消除歧义。

确认提交后的 GC、`last_gc_result` index 重写、顶层 `snapshot.manifest.json` /
`journal.segments.json` 兼容镜像更新属于维护结果；失败不能改写业务提交分类，也不能迫使调用方
丢弃已提交候选。维护结果可重试、可标记 degraded；既有 status/诊断应区分提交结果、所选 generation、
fence 状态与维护原因，避免把已提交但维护失败报告为无效果。读状态沿用最后已验证状态并标明限制，
不因状态查询新建业务效果。此合同须由所有普通 generation-backed mutation 共用，不能只修 reserve。

<a id="storage-generation-g3-design"></a>
### 8.5 完整同代候选读取

承接 [G3 候选恢复](runtime-storage-footprint-governance.prd.md#storage-generation-g3)。
普通与 authoritative loader 复用一个只读候选读取边界：捕获 indexed record → 校验 manifest、
pin 集合与必要 archive/metadata → 读取并校验 snapshot chunks 与 journal segments 的 CAS 内容 →
解码、组装并以 `World::from_snapshot` 等既有规则构造/校验 detached World → 一次返回同代
World、snapshot/journal、可选 metadata 及身份。候选期间不附加可写 persistence，不写磁盘
audit/cache/GC/index，不调用 provider/dispatcher，也不修改活 World；成功后由调用入口决定安装。
“record 校验通过”不是候选完整通过，组装及 World 构造失败也必须返回到这一候选边界。
候选循环不得调用会执行 `recover_loaded_runtime_world`、恢复副作用或保存新代的完整加载入口；
这些操作只能在候选选定后按对应恢复合同执行，不能让试读一个坏候选先修改存储。

先尝试 latest，再尝试 index 明确列出的 rollback-safe；错误按 typed 原因分类，不按 helper 位置
或错误字符串决定是否跳代：

| 错误分类 | 处置 |
| --- | --- |
| 候选自身的完整性错误 | 该候选引用的文件/blob 缺失、摘要不符或内容损坏导致的解码/组装失败，记录 candidate id、阶段与原因后尝试下一 indexed 候选。 |
| 来源或消费合同错误 | index 本身不可解释、world/authority/version 不允许消费，或不能归为候选完整性问题的 I/O、权限、取消、资源预算错误，直接向上传播，不靠回退绕过。 |

所有原 hash/schema/语义及入口资源限制保留；两代都无效或共享必要 blob 损坏仍失败。
authoritative 返回值的 snapshot、journal、metadata 必须同代，禁止拼接两代或退到未索引可变 JSON。
健康 latest 始终优先；选择 rollback-safe 时返回实际身份及降级原因。无 index 的既有 legacy 入口
保持原兼容规则，不因本条扩大为 indexed 恢复的后备来源。

<a id="storage-generation-cross-boundary-design"></a>
### 8.6 提交核对、恢复选择与服务资格

承接 [交叉边界](runtime-storage-footprint-governance.prd.md#storage-generation-cross-boundary)。
提交核对回答“本次候选是否已提交”，候选读取回答“当前哪代完整可读”，两者不得互相替代。
B 已提交而后损坏时，G3 返回健康 A 不证明 B 为 `NotCommitted`；已确认的提交事实不能降为未提交。
无法确认原决定时保持 `StatusUnknown`；已有提交证据但材料不足以安装/恢复时保留该事实并进入
恢复受阻状态，不能发布旧 A 或重新产生 B 的业务效果。

选出的旧 generation 仅是经过验证的恢复材料；恢复至要求的 committed head、核对 receipt/lease
及完成 [同世界服务闸门](../design.md#runtime-recovery-design) 之前，保持只读/隔离和写入 fence。
只读读取成功不授予正式 authority、voting 或 serving；本地 generation 提交不代替网络正式提交证明。
G1 的未知保护优先于 G2 的回收目标，G3 的回退选择不得解除 G1 的 fence。

## 9. Execution bridge retention 设计
### 9.1 Retention classes
- `LatestHead`
- `HotWindow`
- `Checkpoint`
- `ArchiveOnly`

### 9.2 Pin set 计算
每个 sweep 周期在受控边界重算 execution store 保留根及 pin set；原本地基础集合包括：
- latest head record 的 `snapshot_ref` / `journal_ref`
- hot window 中所有 record 的 `hot_snapshot_ref` / `hot_journal_ref`
- checkpoint manifest 引用的 `snapshot_ref` / `journal_ref`
- replay archive index / tick archive segment blobs
- 恢复所需 module artifact 原内容及 refs

正式路径还必须按[完整历史闭包](formal-world-storage-efficiency.design.md#des-lws-001)纳入全部历史 qualified/控制链、原 snapshot/input/result/proof/版本依赖，未决 Frozen/Prepared/Decided 等材料，以及 reader/transfer、rollback、attempt、迁移源/目标和物理 recipe/dictionary 的保护根。引用边须按实际对象类型遍历，不能只扫 latest 或直接 artifact.references。

只有完整 typed mark 成功、期间新增根和 pins 已重核、替代内容在必要独立故障域验证可读，才按[安全回收事务](formal-world-storage-efficiency.design.md#des-lws-004)释放不再需要的表示。未被不完整 pin 扫描发现，不等于可回收；未知类型、缺内容、预算不足或读者状态不明均保留并停止相应 sweep。TTL、mtime、最近数量或磁盘压力不能替代证明。

### 9.3 Checkpoint 策略
- 默认每 `N` 个 committed heights 生成一个 checkpoint。
- checkpoint 间隔由 profile 决定：
  - `dev_local` 较稀疏
  - `release_default` 中等
  - `soak_forensics` 更密集
- 最近 `K` 个 checkpoint 是本地恢复加速的密度目标，不能单独构成删除其他 checkpoint 的条件。
- 删除某旧 checkpoint 前，枚举所有曾依赖它、且仍在适用历史承诺中的高度 `H`；正式世界包括全部承诺高度。对每个 `H` 验证仍有完整原 snapshot，或另一个 `C' <= H` 加完整原日志/增量、版本及控制依赖能恢复原内容与语义。更晚的 checkpoint `C' > H` 不能恢复更早的 `H`，只证明存在更新 checkpoint 不足以删除。
- 上述覆盖、原引用字节/证明、必要故障域与 reader/rollback/未决 pins 全部成立后，才可释放旧展开表示；任何区间不具备完整 delta 或原工件时继续保留所需 checkpoint。checkpoint 密度调整不改变世界历史范围。

## 10. Replay 设计
### 10.1 指定高度重建
输入：目标高度 `H`；正式世界适用范围为全部承诺历史，原 local bootstrap 验收仍只说明实际覆盖高度。

1. 在 checkpoint index 中找到最近的 `C <= H`。
2. 加载 checkpoint snapshot/journal/module anchor。
3. 从 canonical replay log 读取 `(C+1..H)` 对应 commits。
4. 使用确定性执行器顺序执行。
5. 将重建得到的 `execution_state_root` 与 `execution_records/H` 中记录值对比。
6. 匹配则返回 success；否则进入 mismatch fault。

这里的 success 仅表示本条 committed-input 重执行路径的 root 匹配；它不代签 `WorldJournal` 事件后缀恢复或重新开放服务。
完整 snapshot 往返、事件后缀归约与 committed-input 重执行的输入/执行边界按 [runtime 恢复设计](../design.md#runtime-journal-suffix-recovery-design) 分别标识。
拟继续执行的候选还必须具备该路径完整的 action cursor/era 与 tick schedule，并按 [续跑验证](../design.md#runtime-journal-suffix-validation) 提供正向对照；不能只凭本步骤的 root success 丢弃仍是唯一控制状态来源的 checkpoint/历史材料。

### 10.2 无 checkpoint 情况
- 若 `H` 在 hot window 内，可使用经验证的 `C <= H` hot snapshot 及完整后缀作为起点；不能从较晚快照倒推更早历史。
- 若某适用 `H` 无完整原 snapshot，也没有可验证的 `C <= H` 恢复路径，就违反 replay contract，GC 不允许造成该状态；正式路径须保留材料并按原服务闸门限制能力。

### 10.3 外部非确定性约束
若某些效果当前未完全体现在 committed payload 中，则必须在实现前明确补齐。否则 replay 只能得到“结构上可重放”而不能保证 bit-identical state。必须逐项核对：
- LLM 决策是否已经在 commit 前固定成 action payload
- 模块版本/工件 hash 是否在 replay 时可唯一解析
- 随机种子/时钟依赖是否已包含在 block / commit 上下文

正式路径同时适用[原候选、冻结输入与外部回执未知的恢复边界](formal-world-storage-efficiency.design.md#des-lws-007)：重用原已持久结果，按原幂等身份核对效果，不能为补齐历史重新调用模型、派发已确认效果或收费。完整 delta 尚未证明时不回收唯一 snapshot/控制状态材料。

## 11. Tick consensus 热冷分层
### 11.1 目标
控制 `snapshot.json` 中 `tick_consensus_records` 的体积，同时保留链路验证与审计能力。

### 11.2 方案
- `snapshot.json` 仅保留最近 `tick_consensus_hot_limit` 条 record。
- 热窗口外 records 被编码为 archive segment，写入 execution store 或 archive 目录。
- `TickConsensusArchiveIndex` 记录 tick 范围、content hash 与 hash chain anchor。
- 审计读取按区间查询 archive segment，再验证 segment hash 与 anchor。

### 11.3 结果
- latest snapshot 只保留热记录，显著缩小 `snapshot.json`。
- 历史链路验证不依赖热内存常驻集合。

## 12. Metrics / 状态输出设计
### 12.1 输出位置
- `reward-runtime-storage-metrics.json`
- chain status API 新增 storage section
- launcher / soak 脚本直接读取上述字段

### 12.2 最低字段
- `storage_profile`
- `bytes_by_dir`
- `blob_counts`
- `ref_count`
- `pin_count`
- `retained_heights`
- `checkpoint_count`
- `effective_budget`
- `replay_summary`
- `orphan_blob_count`
- `last_gc_at_ms`
- `last_gc_result`
- `last_gc_error`
- `degraded_reason`

### 12.3 使用方式
- 开发者快速判断为什么涨体积。
- soak profile 可以把“保留策略导致的正常增长”和“孤儿泄漏”区分开。
- required/full 测试直接断言预算与错误状态。

<a id="storage-observation-design"></a>
### 12.4 指标归约与独立 Gate 终态

产品层 N/A；reason: collector/adaptor字段归约、实际采样间隔、独立CLI退出与dry-run配置预览是operator工具的工程正确性验收；scope: 仅STORAGE-AC-1–5五项内部观测及进程边界；不判定真实release接线，不排除存储恢复、服务或发布外部义务。仅免新增产品叶分配，不免本条专业验收。

承接 [STORAGE-AC-1–5](runtime-storage-footprint-governance.prd.md#storage-observation-contract)。
collector 提供实际目录关系及可用性，adapter 统一字段，summary 保留解释；不更改 GC 的删除决策。

| 责任 | 设计与反例 |
| --- | --- |
| 容量并集 | 使用可验证的规范化目录身份与包含关系计算并集，不重扫或公开敏感原始路径；同目录别名去重。root=40B 内含 child=20B、grandchild=16B，另有外置目录=16B 时 total=56B 而非逐字段相加的 92B。外置 override 不能因字段名像 child 被忽略；关系不足时 total 不可用且解释缺失项 |
| GC availability | 显式分类 success/failed/not_run/not_available/unknown/缺字段；仅 failed 派生失败 GC 告警/优化项。其他类别按可用性给诊断，独立 degraded/orphan 告警照常保留；未知不是 success |
| 有效采集间隔 | worker 接受的 effective interval 是 status/JSON 回显来源；requested/default 仅作来源解释。用可控时钟覆盖三档配置、首次采集、门槛及一次慢采集未完成时不重入，独立 degraded 通知不等待周期 |
| 普通 Gate | 解析/读取→评估→保存两种报告→输出状态/路径→返回判定。Bash 显式保留 Python/评估退出结果，报告成功和最终 echo 不将 FAIL 改为 0；输入或报告写入失败保持错误，无成功声明 |
| dry-run | 参数先经 argv/env 传给 serializer，输出真实 string/null/number/bool；Markdown 不使用会展开反引号命令替换的 heredoc。不访问传入的 status 路径，不执行评估；标注这是配置预览而非 PASS |

验证分别覆盖完整输入链与进程边界：容量经 runtime collection→adapter→summary 的生产形 fixture；GC 六类状态
与独立异常组合；间隔以可控时钟检查实际调用次数；Gate 用 orphan 超限、checkpoint 不足、profile 不匹配、
启用 require-no-degraded 的 degraded 及启用 require-gc-clean 的 failed 逐项断言非零且报告为 FAIL。
健康 fixture 必须为 0/PASS，无效输入和不可读源保持错误；dry-run 用不存在的源路径证明不读取，覆盖省略 profile
及 dev_local/release_default/soak_forensics，stderr 不出现 command not found。

此处定义待实现/待验证的合同；普通报告、CLI exit、配置预览和 release 接线分别判定，不推断当前发布门禁已接入，
不运行真实节点、GC、恢复或性能压力测试来代替这些有限回归。

## 13. 失败语义与恢复策略
### 13.1 GC 失败
- 任何 pin set 构建失败、历史闭包/引用校验失败、必要内容/证明或 latest ref 缺失：
  - 停止 sweep
  - 写 `last_gc_result=failed`
  - 写 `degraded_reason`
  - 保留现有 pinned + unknown 数据，不冒进删除
  - 正式恢复或 serving 所需闭包/证明未满足时，按[原服务闸门](../design.md#runtime-recovery-design)限制受影响能力；latest 可读或本地 degraded 输出不能代签正式服务资格

### 13.2 Save 中断
- latest generation 切换必须两阶段提交。
- 若确认在替换 index 前退出，旧 index 与其完整引用闭包保持不变；仅在核实不属于未决提交保护集合后清理 staging。
- rename 后同步未确认、回读不明按 [G1](#storage-generation-g1-design) fence 并保留旧/新材料；已确认提交后的维护失败保留新业务结果，不能统称为回滚旧 World。

### 13.3 Replay mismatch
- 若 checkpoint + canonical log 重建出的 `execution_state_root` 与记录不一致：
  - status 标记 `replay_contract_broken`
  - 阻断进一步 aggressive GC
  - 在 `test_tier_full` 与 release gate 中作为硬阻断
  - 正式世界保留全部相关材料，恢复到所需合法边界并补齐闭包/证明前，不由旧快照匹配结果重新开放写服务

## 14. 迁移方案
### 14.1 向后兼容
- 旧 `ExecutionBridgeRecord` 仍可读；迁移时若缺 `commit_ref` / `retention_class`，按 legacy 模式处理。
- legacy 模式下禁止 aggressive GC；原 latest-safe sweep 只表示本地受保护范围，不授权删除正式历史。无法证明完整旧引用闭包时保留 unknown 材料并阻止相应回收。

### 14.2 渐进迁移步骤
1. 先落 storage profile / metrics，不改变存储语义。
2. 再引入 sidecar generation pin/sweep。
3. 再把 execution bridge 改为“checkpoint + hot window”。
4. 最后引入 `tick_consensus_records` archive index。

### 14.3 回退策略
- 任一步失败都可回退到“保留更多数据、不删历史”的安全模式。
- 回退不允许改变 canonical log 结构。
- 正式新物理格式的混合读取、切换及旧 binary 能力检查由[正式迁移合同](formal-world-storage-efficiency.design.md#des-lws-006)拥有；回退软件不能抹掉新写历史，已有 legacy 恢复测试不证明新格式迁移已完成。

## 15. 代码落点建议
- `crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/mod.rs`
- schema-v3 `ExecutionBridgeRecord` 与 V1/V2 compatible reader
  - checkpoint 写入逻辑
  - pin set 计算与 retention manager
- `crates/oasis7_runtime/src/world/persistence.rs`
  - sidecar generation index
  - manifest-aware sweep
- `crates/oasis7_runtime/src/snapshot.rs`
  - `tick_consensus_records` 热冷分层索引字段
- `crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime.rs`
  - storage profile 配置与 metrics 发布
- `crates/oasis7_node/src/replication.rs`
  - hot/cold commit mirror 与 storage profile 对齐
- `testing-manual.md`
  - footprint / replay contract / GC fail-safe 验收矩阵

## 16. 测试设计
本节既有 `2500` heights、llm_bootstrap、local SSD 和单目录预算仍按原 fixture/配置独立判断，不上调其历史证明范围。正式完整历史、P/R 独立故障域、原字节迁移、长期规模及全生命周期实际容量另按[正式验证设计](formal-world-storage-efficiency.design.md#des-lws-009)提供证据；局部 required 通过不能替代该组合。

### 16.1 `test_tier_required`
- `2500` heights 样本下默认 profile 体积预算验证。
- latest-state restart 后 `execution_state_root` 一致。
- 提交已确认且 sidecar generation sweep 成功收敛后 orphan blob 数量为 `0`；暂态/未知/维护降级另验恢复保护，不以稳态预算促成提前删除。
- retained target height 可由 checkpoint + canonical log 重建。
- `tick_consensus_records` archive read + verify 成功。
- replication cold-index scan / seek 与 tick archive range seek 在边界口径上保持一致。

### 16.2 `test_tier_full`
- GC 中断/部分写入注入，验证 fail-safe。
- profile 切换：`dev_local -> soak_forensics -> dev_local`。
- replay mismatch 注入，验证系统进入 `degraded` 并阻断 aggressive GC。
- 长跑脚本对 metrics / budget gate 的联合验证。

<a id="storage-generation-validation-matrix"></a>
### 16.3 Generation 故障边界矩阵

以下 GEN 场景是待实现/执行的验证计划。本次文档修改未运行行为测试；现有局部基线来自
[persistence 测试](../../../crates/oasis7_runtime/src/tests/persistence.rs)、
[cognition economy 测试](../../../crates/oasis7_runtime/src/tests/cognition_economy.rs) 与
[authoritative recovery 局部测试](../../../crates/oasis7_runtime/src/world/authoritative_recovery_generation.rs)。
后者已有 `recovery_generation_post_commit_failure_recovers_new_metadata`、
`authoritative_recovery_skips_invalid_latest_generation_for_rollback_safe`，不能代签普通事务或 snapshot/journal 组装矩阵。
实施时将可确定性注入的 GEN 场景接入受影响模块的 required 回归；实际文件系统持久性按适用平台另验，
不以本地回读或测试 failpoint 代替真实掉电证据。执行按 [现有测试手册](../../../testing-manual.md) 记录命令、候选、环境和结果。
GEN-G2 中允许回收 A 的本地 fixture 须明确 A 没有额外历史或外部 pin；该结论不放宽正式完整历史根。新增正式 GC 场景还应包含被较早 H 唯一依赖的 A，验证它继续受到保护。

| 目标场景 | 同一单 writer 临时目录中的故障刺激 | 必须对账的结果 |
| --- | --- | --- |
| GEN-G1 | index 前；index 后 failpoint；GC 结果 index 重写；两个兼容镜像分别失败；rename 后目录 sync 失败及回读失败 | 返回分类、活 World、重启 World 的 lease/余额/receipt 一致；同 key 不重复，无关账户后续保存不抹掉已提交租约；同步不明时保持 fence，正常及原 precommit 回归保留。 |
| GEN-G2 | 先存 A/B，再让 C 的 index.tmp open/write/sync 分别失败；另设 B 局部损坏；C 提交后清理失败与重试 | C 未提交时旧 index 字节与 A/B 全闭包不变，健康 A 前后可用；C 提交后保留 C/B，允许回收 A；清理失败只多保留材料。 |
| GEN-G3 | 普通/authoritative 两入口 × latest 独占 snapshot chunk/journal segment × 缺失/损坏；另注入后续组装及 detached World 构造失败 | 八格及构造完整性故障选择健康 rollback-safe，metadata 同代；探测前后 index/文件/活 World 不变、provider/dispatcher 调用为零；健康 latest 优先；两代均坏、共享必要 blob 坏、来源/合同错误仍失败；保留 archive/index 负例。 |
| GEN-G1-G3 | B 已有提交证据但后损坏、只能读到 A；以及 B 发布后目录 sync 未确认但新 index 可见 | 不因返回 A 判 B 未提交，不因可见判持久；保留已知提交事实或未知状态，旧 World 不可写；只有核对并恢复到所需 head 后解除 fence。 |

## 17. 开放问题

### 未决风险与决策依赖
- canonical commit log 当前是否已完整承载所有外部非确定性结果；若没有，需要先补“external effect ledger”再做 aggressive retention。
- checkpoint 是只保 latest `snapshot_ref + journal_ref` 还是同时保 module anchor 的完整 manifest；当前设计建议必须带 module anchor。
- `tick_consensus_records` archive segment 是放 execution store 还是 node-distfs store；当前建议优先放 execution store，避免跨子系统依赖扩大。

## 18. 结论
- 本地目标是减少热副本、重复展开状态和临时材料。canonical 原内容及依赖保持，checkpoint 提供经过验证的恢复起点，latest head 提供快速恢复路径，GC 只释放不再被完整保留集合依赖的表示。
- 正式世界先按无损物理合同共享/压缩；只有完整原字节、历史闭包与 J1/J2 等续跑证据成立后，才评估稀疏 checkpoint 加完整增量。该目标不把旧本地样本或当前局部实现升级为正式全历史能力。

## 存储适用设计承接视图
owner runtime_engineer；原视图审读基线为 eng-cc/oasis7@f9d5a552d9af04c1b1398262808198a58e560230，本次 generation 增补及相关验收修订采用[G1](#storage-generation-g1-design)所列基线。旧 §§1–18 章节保留；尤其旧 §11.1 目标仍是 tick compaction anchor，不改名。新增“11.1 验证映射表”是 trace view，语义/heading 不覆盖旧 anchor。历史体积样本不是当前测量，定义结构/测试不是 proven capability。

<a id="storage-history-dependency"></a>
### 历史版本、引用与权威材料
Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670、O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c；分别承接[完整历史](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001)、[冷热不缩历史](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003)及[安全回收](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004)在本专题的删除与保留边界。

正式 retained H 为全部承诺历史高度；每个 H 具有原完整 snapshot 或 C<=H 加完整原内容及依赖的可验证恢复路径。原 governing manifest、native/WASM 工件原字节/hash/schema/module binding 唯一解析；当前版本不能替代旧 receipt/资源/责任解释。R0 latest、R1 全部适用历史、R2 原审计内容及索引、R3 fail-safe 分别验证；metadata 不替代 payload，较晚 checkpoint 不代签较早 H。

pin 包括本地 latest/rollback-safe、hot/checkpoint/archive，及[正式保留根](formal-world-storage-efficiency.design.md#des-lws-001)的全部历史、控制链、证明、未决/reader/rollback/attempt/迁移依赖。按[正式 GC 事务](formal-world-storage-efficiency.design.md#des-lws-004)先完整闭合再回收；缺 anchor/artifact/proof、未知引用或 hash mismatch 不称 matched replay，停止不安全 sweep，并遵守原正式服务闸门。具体 ABI/schema/migration 由 [WASM interface](../wasm/wasm-interface.md)，finalized checkpoint/active registry/redundant archive 由 P2P/ops。旧 ModuleAnchor 概念与 open question 保留，不声称实际完整 schema 已实现。
current ExecutionBridgeRecord 在 crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/mod.rs 为 v3，new_v3/V1/V2 reader；旧 schematic hot_snapshot_ref/hot_journal_ref 描述 retention 角色，不是新增 DTO 字段。实际 snapshot_ref/journal_ref/ref reader 以 source 为准，不能因 schematic prose 改 current schema。record/checkpoint/root/CAS 只证明本地 committed binding，不提供 BFT/light proof。

<a id="storage-state-transaction-persistence"></a>
### 状态、事务与持久化
canonical commit log 先持久，再执行 bridge/records/latest/checkpoint，最后 retention/metrics；记录 last_applied 防 stale/noncontiguous，restore/checkpoint 重新核 refs/log/root。world generation 先 staging/校验/sync，再原子 latest pointer/index 及必要目录同步；确认新 index 提交前保护旧 index 的完整引用闭包，未知时保留旧/新材料并 fence，确认后以新 retained set 回收。GC/镜像维护在业务 commit 外且只在授权 runtime 根目录；维护失败不撤销已提交业务结果。pin 构建失败/latest missing/未知引用保留数据、degraded、停止 sweep；只有确认 NotCommitted 才维持旧 World 的业务状态，不能由任意 save Err 推导回滚。普通保存与完整候选读取分别遵循 [G1](#storage-generation-g1-design)、[G2](#storage-generation-g2-design)、[G3](#storage-generation-g3-design) 和 [交叉边界](#storage-generation-cross-boundary-design)。replay mismatch 标记 replay_contract_broken 并阻断 aggressive GC/full release verdict；留更多数据 rollback 不改 canonical log。
R0 latest-only restart 与 R1 exact H 的证据分开：legacy/local bootstrap 只报告实际覆盖范围，不能外推正式能力；正式世界不因超出 storage profile 热窗口失去完整历史保证。legacy 无 ref 时的 latest-safe sweep 也不得删除尚未证明可替代的正式依赖。原始 payload hash 不受磁盘压缩编码影响，只有 header 后确实更小才压缩；正式 codec/共享/迁移的目标另按[原字节合同](formal-world-storage-efficiency.design.md#des-lws-002)，不把通用 CAS 基础表述为 formal 已实现。continuous latest-based hot range 与 shared cold anchor/segmented pack/双写别名回填保持旧合同，seek/scan 边界同源。多进程第二 writer 必须检测阻止，不新增多writer协调。旧 budget/p95/CPU/metrics 采样仍限其 PRD NFR 样本；正式总盘按[实际物理容量合同](formal-world-storage-efficiency.design.md#des-lws-009)分域度量，soak_growth 不作为 leak 或删历史豁免。

<a id="storage-service-limit"></a>
### 只读恢复、服务限制与演进
restore root 相等仅本地历史材料条件，开放 voting/serving/new intent 还需 [root recovery gate](../design.md#runtime-recovery-design) 的同world/finality/append/version/head；state-sync/ops 同窗口材料不可由 latest-only green 代签。CLI/launcher/bundle 同名 profile 和 OASIS7_CHAIN_STORAGE_PROFILE 非空注入、默认继承及 bundle runtime 路径保持旧 AC-6.1/§6.8。status 仅 budget/bytes/pins/replay/GC/原因，不导出 payload；不按 file mtime 猜删除。migration 按 §14 渐进保留 legacy refs，external-effect committed payload materialization 未完整时保留 §17 风险、禁止假定 bit-identical replay；输入/profile/retention/version/消费者变更触发 runtime/P2P/WASM/launcher/QA 复核。
归档或索引缺证时，G3 允许的本地只读诊断候选仍须满足其原错误分类和完整组装要求；该候选不产生正式 serving 权。正式完整闭包/证明未核验、独立持久条件未成立或新格式读者不兼容时，保留材料并限制能力，不能以调整 storage profile 或加载较旧状态绕过。

<a id="storage-case-ac1"></a>
#### AC-1/SC-1：2500 committed heights llm_bootstrap dev/launcher store≤256 MiB，record引用无dangling
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-ac2"></a>
#### AC-2/SC-2：提交确认且清理成功后收敛到latest+rollback-safe及外部pin，sidecar≤16 MiB，orphan=0；暂态/降级保留保护数据
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-ac3"></a>
#### AC-3/SC-4：GC后latest restart，execution root/journal_len/module_registry相同
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。
该比较保留为存储恢复断言；恢复后继续执行的证明另按 [J1/J2 续跑](../design.md#runtime-journal-suffix-validation) 记录适用路径，空 suffix 结果不关闭非空 suffix 义务。

<a id="storage-case-ac4"></a>
#### AC-4：GC中断/partialwrite/pin失败保当前index完整闭包；未知另保候选并隔离旧World写入
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-ac5"></a>
#### AC-5/SC-6：每policy-retained H由nearestC<=H+canonical log重建原root
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。
明确证据是 committed-input 重执行；若使用 WorldJournal 事件后缀，则独立满足 [控制记录兼容](../design.md#runtime-journal-suffix-compatibility) 和 [续跑矩阵](../design.md#runtime-journal-suffix-validation)，不相互替代。

<a id="storage-case-ac6"></a>
#### AC-6/SC-5：profile/effective_budget/bytes/retained/checkpoint/replay/lastgc/error/degraded可读，无额外扫描

产品层 N/A；reason: 本条规定既有status/metrics接口字段及避免额外扫描的内部观测实现合同；scope: 仅profile/effective_budget/bytes/retained/checkpoint/replay/lastgc/error/degraded可读性与采集成本；字段可读不替代恢复正确性、服务资格或release判定。仅免新增产品叶分配，不免本条专业验收。
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-ac61"></a>
#### AC-6.1：四入口同枚举无损透传，三wrapper OASIS7_CHAIN_STORAGE_PROFILE非空注入/默认继承/bundle runtime

产品层 N/A；reason: 本条规定launcher与wrapper之间的同枚举配置接线和无损透传，不产生新的产品能力或默认部署承诺；scope: 仅四入口同枚举、三wrapper非空注入/默认继承/bundle runtime；保留全部原条件，不排除恢复、服务、经济及发布外部义务。仅免新增产品叶分配，不免本条专业验收。
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-ac7"></a>
#### AC-7/SC-3：2500 heights snapshot≤512KiB，archive区间读取+链路校验
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-ac8"></a>
#### AC-8：required/full分别含footprint/restart/failsafe/profile/archive/retained replay及独立G1/G2/G3/交叉矩阵
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-sc7"></a>
#### SC-7：release_default hot_head_heights≤checkpoint_interval，不重复保留额外同区间snapshot

产品层 N/A；reason: hot/checkpoint密度及避免额外重复snapshot是同一存储profile内的缓存保留策略验收，不单独定义产品恢复目标；scope: 仅release_default原hot_head_heights≤checkpoint_interval与同区间不重复保留额外snapshot；R0/R1恢复正确性、exact-height restore和服务资格仍分别按既有合同证明。仅免新增产品叶分配，不免本条专业验收。
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-nfr1"></a>
#### NFR-1：2500 heights node root≤384MiB且store≤256MiB

产品层 N/A；reason: 本条是在固定样本与profile下的工程磁盘预算，不新增面向玩家或网络的容量承诺；scope: 仅2500 heights下node root≤384MiB且store≤256MiB原条件；不排除材料完整性、恢复/服务和发布外部义务。仅免新增产品叶分配，不免本条专业验收。
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-nfr2"></a>
#### NFR-2：2500 heights localSSD sweep p95≤500ms

产品层 N/A；reason: 本条是固定本地SSD与样本条件下的sweep性能预算，不另设产品服务时限；scope: 仅2500 heights/localSSD/sweep p95≤500ms原条件；GC引用完整性、恢复/服务与发布仍独立验证。仅免新增产品叶分配，不免本条专业验收。
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-nfr3"></a>
#### NFR-3：default latest restart p95≤5s

产品层 N/A；reason: 本条是默认profile latest restart的工程性能预算，不替代同世界恢复结果或产品服务许可；scope: 仅default latest restart p95≤5s原条件；恢复正确性、非空事件后缀续跑、重新serving及发布保持外部义务。仅免新增产品叶分配，不免本条专业验收。
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-nfr4"></a>
#### NFR-4：优化不能降低replay/recovery确定性，相关回归100%
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-nfr5"></a>
#### NFR-5：所有policy retained heights replay成功100%且原root相等
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-nfr6"></a>
#### NFR-6：metrics采集发布频率不高于每1s一次，额外CPU≤5% local单节点

产品层 N/A；reason: 本条限定内部metrics采集发布频率与本地单节点额外CPU成本，不定义产品实时性或服务SLA；scope: 仅每1s最多一次与额外CPU≤5%原条件；状态可信性、恢复/服务及发布仍独立成立。仅免新增产品叶分配，不免本条专业验收。
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-nfr7"></a>
#### NFR-7：soak显式更高占用、metrics增长/可配上限

产品层 N/A；reason: 本条规定显式soak工程profile的资源可观测性与可配上限，不改变默认策略或新增无限容量保证；scope: 仅soak显式更高占用、metrics增长与可配上限原条件；不是泄漏豁免，不排除恢复、服务和发布外部义务。仅免新增产品叶分配，不免本条专业验收。
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-compression"></a>
#### 原payload content_hash不变，compressed header后更小才压缩，读回rawbytes
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-cold-range"></a>
#### continuous latest hot range/shared coldanchor scan seek同边界，pack len/offset/hash与canonical/legacy别名回填
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-legacy-height"></a>
#### v3新写/V1V2兼容，stale/noncontiguous heightfailclosed；legacy仅latest-safe无aggressiveGC
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-save-failure"></a>
#### save满盘/新generation中断按G1区分三态，保护材料不恢复旧写权，维护失败不撤销提交；pin/latestmissing停止，replay mismatch阻aggressiveGC
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-single-writer"></a>
#### 仅runtime指定根删除，显式pin/无mtime猜测，第二writer检测阻止、metrics不新增payload

产品层 N/A；reason: 本条规定runtime本地存储与metrics的工程安全机制，不独立定义网络提交权、产品数据访问权或新服务权限；scope: 仅授权runtime根、显式pin而非mtime猜测、第二writer拒绝与无敏感payload导出；原root DWE/DCS、G1–G3、恢复/服务闸门及全部删除/保护条件仍适用。仅免独立产品叶分配，不免本条专业验收。
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-version-dependency"></a>
#### retained原manifest/artifact/moduleanchor唯一解析，missing/hashconflict停止unsafe sweep/不称matched
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

<a id="storage-case-service-boundary"></a>
#### localroot/restart仅历史条件，append/finality/version/head与redundantarchive仍外部
按 [历史依赖](#storage-history-dependency)、[状态/事务/持久化](#storage-state-transaction-persistence) 及 [服务限制](#storage-service-limit) 执行；具体旧字段/目录/枚举/预算/失败优先级原样适用。本条件保留独立判断，局部测试或document检查不能替代实际阈值/同窗口材料。

### 2.1 需求承接与分配表

限定产品来源沿配对 PRD“当前合同、历史样本与上游接受”、[profile/历史边界](runtime-storage-footprint-governance.prd.md#storage-profile-history-boundary)及既有根 receiver 承接；以下局部义务与新增正式边界不替换原独立专业接受关系，也不扩大旧测试的证明范围：

- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 经 [root generation 合同](../prd.md#runtime-generation-persistence-contract) 与 [root recovery receiver](../design.md#runtime-recovery-design)，定位 G2/G3/交叉边界及 [历史材料与 replay](#storage-history-dependency) 的本地子义务。完整非空事件后缀正向续跑、独立故障域、合法 authority 与重新 serving 仍未由本专题证明，原 `test_tier_full` 和 ops/runtime/QA 同窗口组合不变。
- Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[product_requirement: REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) 经 [root generation 合同](../prd.md#runtime-generation-persistence-contract) 与 [root generation receiver](../design.md#runtime-generation-persistence-design)，仅定位 [G1](#storage-generation-g1-design) 的持久结果、调用方一致性及未知保护；本地 durable commit 不等于正式 authority 提交或全部验证者组合。
- Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[product_requirement: REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) 经 [root version receiver](../design.md#runtime-version-design)，仅定位 [storage-accept-version-dependency](runtime-storage-footprint-governance.prd.md#storage-accept-version-dependency) → [storage-case-version-dependency](#storage-case-version-dependency) 的 retained 原 manifest/artifact 保护。完整 activation ledger/proof、跨版本 replacement 与版本窗口组合仍未证明。

产品原部分覆盖/部分实现/未核实、各 AC 与 full QA 义务保持；本次仅静态关系核对，没有新实现或运行证据。其他独立行没有准确产品关系时继续由既有专业 owner 核对，不推定覆盖。

原承接视图输入身份为 eng-cc/oasis7@f9d5a552d9af04c1b1398262808198a58e560230；本次 G1/G2/G3 及交叉边界增补按[G1](#storage-generation-g1-design)所列基线。local anchors 是本文技术接受关系，非机器 schema。每行范围独立，外部未决保留。

| 上游 requirement / acceptance | 具体 obligation 与条件 | 本设计条款 | 外部 owner / dependency | 排除或未覆盖 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1；[REQ-LWS-001](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001) | 正式全部承诺 H 及原依赖完整；索引/hash 不能替代原文 | [历史边界](runtime-storage-footprint-governance.design.md#storage-history-dependency) | Runtime/formal/WASM；[正式闭包权威](formal-world-storage-efficiency.design.md#des-lws-001) | 原 local bootstrap、latest/root 与 2500 样本不证明完整正式历史 |
| Trace refs: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670；[REQ-LWS-003](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003) | storage profile 只调工作集/恢复密度；删除 checkpoint 覆盖所有依赖 H | [历史边界](runtime-storage-footprint-governance.design.md#storage-history-dependency) | Runtime/ops；[正式冷热合同](formal-world-storage-efficiency.design.md#des-lws-003) | 旧 retained-window 场景不授权缩短正式历史或改变 authority |
| Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c；[REQ-LWS-004](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004) | 先闭合完整根与未决/reader/rollback pins，后释放可替代表示 | [历史边界](runtime-storage-footprint-governance.design.md#storage-history-dependency) | Runtime/formal/ops；[正式 GC](formal-world-storage-efficiency.design.md#des-lws-004) | 两代保留、未发现引用、TTL 或本地 sweep 成功不证明正式可删 |
| [professional_acceptance: storage-observation-contract](runtime-storage-footprint-governance.prd.md#storage-observation-contract) | STORAGE-AC-1–5：容量并集、GC availability、实际间隔、普通 Gate 与 dry-run 分别验收 | [观测设计](#storage-observation-design) | 对应工具/运行时维护者及 QA | 文档修订不声明脚本或现场验收通过，不新增发布门禁 |
| [professional_acceptance: storage-generation-g1](runtime-storage-footprint-governance.prd.md#storage-generation-g1) | 普通保存按精确候选与 durable 证据区分三态，安装/原返回值/幂等/fence 一致，维护失败独立 | [G1 设计](#storage-generation-g1-design) | runtime persistence 与普通 mutator；现有 cognition key；QA | 目标；不新增业务 ledger、外部 wire enum 或网络正式提交证明 |
| [professional_acceptance: storage-generation-g2](runtime-storage-footprint-governance.prd.md#storage-generation-g2) | 替代 index 确认前保旧闭包，确认后回收，未知保护旧/新材料 | [G2 设计](#storage-generation-g2-design) | runtime storage 单 writer；DistFS 引用与 sync；QA | 不增加多 writer 协调或远端归档；未证明真实掉电 |
| [professional_acceptance: storage-generation-g3](runtime-storage-footprint-governance.prd.md#storage-generation-g3) | 只读完整同代候选含 CAS 组装，typed 错误决定回退，健康 latest 优先 | [G3 设计](#storage-generation-g3-design) | 普通/authoritative loader；DistFS 解码校验；QA | 不吞全局错误、不改 legacy/index/schema 校验，不授予 serving |
| [professional_acceptance: storage-generation-cross-boundary](runtime-storage-footprint-governance.prd.md#storage-generation-cross-boundary) | commit resolver 与 fallback loader 分离；可读旧代或可见 index 均不替代持久/服务证明 | [交叉设计](#storage-generation-cross-boundary-design) | runtime writer fence；既有恢复服务闸门；QA | 不实现正式 authority 或整个同世界恢复链 |
| [professional_acceptance: storage-accept-ac1](runtime-storage-footprint-governance.prd.md#storage-accept-ac1) | AC-1/SC-1：2500 committed heights llm_bootstrap dev/launcher store≤256 MiB，record引用无dangling | [设计](#storage-case-ac1) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac2](runtime-storage-footprint-governance.prd.md#storage-accept-ac2) | AC-2/SC-2：提交确认且清理成功后收敛到latest+rollback-safe及外部pin，sidecar≤16 MiB，orphan=0；暂态/降级保留保护数据 | [设计](#storage-case-ac2) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac3](runtime-storage-footprint-governance.prd.md#storage-accept-ac3) | AC-3/SC-4：GC后latest restart，execution root/journal_len/module_registry相同 | [设计](#storage-case-ac3) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac4](runtime-storage-footprint-governance.prd.md#storage-accept-ac4) | AC-4：GC中断/partialwrite/pin失败保当前index完整闭包；未知另保候选并隔离旧World写入 | [设计](#storage-case-ac4) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac5](runtime-storage-footprint-governance.prd.md#storage-accept-ac5) | AC-5/SC-6：每policy-retained H由nearestC<=H+canonical log重建原root | [设计](#storage-case-ac5) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac6](runtime-storage-footprint-governance.prd.md#storage-accept-ac6) | AC-6/SC-5：profile/effective_budget/bytes/retained/checkpoint/replay/lastgc/error/degraded可读，无额外扫描 | [设计](#storage-case-ac6) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac61](runtime-storage-footprint-governance.prd.md#storage-accept-ac61) | AC-6.1：四入口同枚举无损透传，三wrapper OASIS7_CHAIN_STORAGE_PROFILE非空注入/默认继承/bundle runtime | [设计](#storage-case-ac61) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac7](runtime-storage-footprint-governance.prd.md#storage-accept-ac7) | AC-7/SC-3：2500 heights snapshot≤512KiB，archive区间读取+链路校验 | [设计](#storage-case-ac7) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac8](runtime-storage-footprint-governance.prd.md#storage-accept-ac8) | AC-8：required/full分别含footprint/restart/failsafe/profile/archive/retained replay及独立G1/G2/G3/交叉矩阵 | [设计](#storage-case-ac8) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-sc7](runtime-storage-footprint-governance.prd.md#storage-accept-sc7) | SC-7：release_default hot_head_heights≤checkpoint_interval，不重复保留额外同区间snapshot | [设计](#storage-case-sc7) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr1](runtime-storage-footprint-governance.prd.md#storage-accept-nfr1) | NFR-1：2500 heights node root≤384MiB且store≤256MiB | [设计](#storage-case-nfr1) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr2](runtime-storage-footprint-governance.prd.md#storage-accept-nfr2) | NFR-2：2500 heights localSSD sweep p95≤500ms | [设计](#storage-case-nfr2) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr3](runtime-storage-footprint-governance.prd.md#storage-accept-nfr3) | NFR-3：default latest restart p95≤5s | [设计](#storage-case-nfr3) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr4](runtime-storage-footprint-governance.prd.md#storage-accept-nfr4) | NFR-4：优化不能降低replay/recovery确定性，相关回归100% | [设计](#storage-case-nfr4) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr5](runtime-storage-footprint-governance.prd.md#storage-accept-nfr5) | NFR-5：所有policy retained heights replay成功100%且原root相等 | [设计](#storage-case-nfr5) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr6](runtime-storage-footprint-governance.prd.md#storage-accept-nfr6) | NFR-6：metrics采集发布频率不高于每1s一次，额外CPU≤5% local单节点 | [设计](#storage-case-nfr6) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr7](runtime-storage-footprint-governance.prd.md#storage-accept-nfr7) | NFR-7：soak显式更高占用、metrics增长/可配上限 | [设计](#storage-case-nfr7) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-compression](runtime-storage-footprint-governance.prd.md#storage-accept-compression) | 原payload content_hash不变，compressed header后更小才压缩，读回rawbytes | [设计](#storage-case-compression) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-cold-range](runtime-storage-footprint-governance.prd.md#storage-accept-cold-range) | continuous latest hot range/shared coldanchor scan seek同边界，pack len/offset/hash与canonical/legacy别名回填 | [设计](#storage-case-cold-range) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-legacy-height](runtime-storage-footprint-governance.prd.md#storage-accept-legacy-height) | v3新写/V1V2兼容，stale/noncontiguous heightfailclosed；legacy仅latest-safe无aggressiveGC | [设计](#storage-case-legacy-height) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-save-failure](runtime-storage-footprint-governance.prd.md#storage-accept-save-failure) | save满盘/新generation中断按G1区分三态，保护材料不恢复旧写权，维护失败不撤销提交；pin/latestmissing停止，replay mismatch阻aggressiveGC | [设计](#storage-case-save-failure) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-single-writer](runtime-storage-footprint-governance.prd.md#storage-accept-single-writer) | 仅runtime指定根删除，显式pin/无mtime猜测，第二writer检测阻止、metrics不新增payload | [设计](#storage-case-single-writer) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-version-dependency](runtime-storage-footprint-governance.prd.md#storage-accept-version-dependency) | retained原manifest/artifact/moduleanchor唯一解析，missing/hashconflict停止unsafe sweep/不称matched | [设计](#storage-case-version-dependency) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-service-boundary](runtime-storage-footprint-governance.prd.md#storage-accept-service-boundary) | localroot/restart仅历史条件，append/finality/version/head与redundantarchive仍外部 | [设计](#storage-case-service-boundary) | runtime storage；WASM历史artifact；launcher；P2P/ops proof；QA | 定义/局部material不证明量化阈值/BFT/服务 |

### 11.1 验证映射表

SC-7 原 [driver storage profile 入口](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/driver_storage_profile.rs) 只提供 hardened policy、dev supply、初始化/部分缓存文件与正常 commit/CAS record 的集成相关定义，没有 hot_head/checkpoint_interval 比较断言；下表改用真实 profile 配置断言。两者均未运行，配置 equality 不代签实际 retention、exact-height restore 或 footprint。

所有下列行为验证在本次文档编辑中未运行。定义/计划与当前实现、实际执行、发布分别成立；执行须另固定 source/integration/tested tree、config/world/entry/environment/window、exit/result/artifacts，并回 PR 与实际验证记录。target场景尚无完整runner时明确保持待证明，现有test/manual只是有界接收入口，不能伪称已实现或通过。
新增正式边界的验证使用配套无损手册；原各 AC/NFR 数值及案例保持原负载/路径，不是正式端点、P/R 与冷归档的整体预算或验收替代。

| 上游 requirement / acceptance | 本设计条款 | 独立 obligation / 条件 | 验证 source / ID、层级及候选环境 | 证据目标 | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1；[REQ-LWS-001](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001) | [历史边界](runtime-storage-footprint-governance.design.md#storage-history-dependency) | 所有承诺 H 的原内容、版本/控制依赖及证明完整 | [V-LWS-HISTORY](formal-world-storage-efficiency.manual.md#v-lws-history)；同候选、同完整历史逐对象检查，跨热窗/版本并移除唯一 payload 负例；formal full 范围 | 原对象/高度覆盖、缺失拒绝及实际恢复记录 | 规划未执行；旧 latest 或局部 retained replay 不证明此义务 |
| Trace refs: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670；[REQ-LWS-003](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003) | [历史边界](runtime-storage-footprint-governance.design.md#storage-history-dependency) | profile/checkpoint 变化不删除较早 H 的唯一恢复起点 | [V-LWS-HISTORY](formal-world-storage-efficiency.manual.md#v-lws-history)；构造早期 H 仅依赖旧 C、存在更晚 C'>H，检查拒绝回收；补齐等价路径后再验证释放 | 每个被影响 H 的恢复路径、原内容与回收判定 | 规划未执行；profile 配置 equality 或单个目标恢复不证明全范围 |
| Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c；[REQ-LWS-004](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004) | [历史边界](runtime-storage-footprint-governance.design.md#storage-history-dependency) | 任何必要历史/未决/pin 未闭合均不 sweep | [V-LWS-GC](formal-world-storage-efficiency.manual.md#v-lws-gc)；真实 typed 根、旧 checkpoint、unknown、并发 reader/rollback 与迁移中断；required 局部 + full 进程/故障域 | 根/pin/保留理由、原操作和恢复结果、实际释放 | 规划未执行；原两代 sidecar fixture 不证明正式全局 GC |
| [professional_acceptance: storage-observation-contract](runtime-storage-footprint-governance.prd.md#storage-observation-contract) | [观测设计](#storage-observation-design) | STORAGE-AC-1–5：容量并集、GC availability、实际间隔、普通 Gate 与 dry-run 分别验收 | [现有验证入口](../../../testing-manual.md)；在实际实施候选按观测设计中的各正负 fixture 和进程边界分别运行，入口存在不代表矩阵已覆盖 | 实施 PR 的候选、命令、逐项结果和未运行项 | 本次未运行行为、真实节点、部署或性能验收 |
| [professional_acceptance: storage-generation-g1](runtime-storage-footprint-governance.prd.md#storage-generation-g1) | [G1 设计](#storage-generation-g1-design) | GEN-G1 的提交前后、维护和 sync/回读故障分别成立 | [局部基线](../../../crates/oasis7_runtime/src/tests/cognition_economy.rs)；目标 required GEN-G1：同候选单 writer 临时目录，注入 §16.3 全部故障窗并对账返回值、内存、恢复和同 key/后续写入；现有基线不覆盖该组合 | PR 中实际故障窗、命令、exit 与 lease/余额/receipt、fence 对账 | 本次未实现/运行；平台持久性、真实掉电/provider 未证明 |
| [professional_acceptance: storage-generation-g2](runtime-storage-footprint-governance.prd.md#storage-generation-g2) | [G2 设计](#storage-generation-g2-design) | GEN-G2 的 A/B/C 保护与提交后清理独立于 G1 | [局部基线](../../../crates/oasis7_runtime/src/tests/persistence.rs)；目标 required GEN-G2：同候选单 writer 临时目录，第三代 index.tmp open/write/sync 注入、坏 B/健康 A、提交后 GC 失败与重试；比旧 index 字节与全闭包 | PR 中实际 A/B/C 身份、故障窗、工件完整性和回收结果 | 本次未实现/运行；已有 interrupted-save 测试不代签全部窗口 |
| [professional_acceptance: storage-generation-g3](runtime-storage-footprint-governance.prd.md#storage-generation-g3) | [G3 设计](#storage-generation-g3-design) | GEN-G3 八格、后续组装/World 构造及共享损坏/来源错误负例；返回同代且探测无副作用 | [局部基线](../../../crates/oasis7_runtime/src/tests/persistence.rs)；目标 required GEN-G3：同候选两入口临时目录，取 latest 独占 blobs 后分别缺失/损坏并注入后续构造失败；核对身份与同代材料、零文件/活 World 写入及零 provider/dispatcher 调用，保留 archive/index 负例 | PR 中实际独占/共享对象集合、各格错误、选择及非变更结果 | 本次未实现/运行；不证明业务恢复到最新 head 或正式 serving |
| [professional_acceptance: storage-generation-cross-boundary](runtime-storage-footprint-governance.prd.md#storage-generation-cross-boundary) | [交叉设计](#storage-generation-cross-boundary-design) | GEN-G1-G3 不把回退当未提交、不把 rename 可见当 durable | [局部基线](../../../crates/oasis7_runtime/src/tests/persistence.rs)；目标 required GEN-G1-G3：同候选单 writer 注入 B 提交后损坏及目录 sync 未确认，分别观察提交分类、候选选择、维护状态和写入 fence | PR 中实际原提交证据、回退身份、同步结果与零旧 World 发布 | 本次未实现/运行；恢复服务闸门的完整证据仍由既有 owner 提供 |
| [professional_acceptance: storage-accept-ac1](runtime-storage-footprint-governance.prd.md#storage-accept-ac1) | [设计](#storage-case-ac1) | AC-1/SC-1：2500 committed heights llm_bootstrap dev/launcher store≤256 MiB，record引用无dangling | [现有 test/manual](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/retention.rs)；required 目标场景：AC-1/SC-1：2500 committed heights llm_bootstrap dev/launcher store≤256 MiB，record引用无dangling；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac2](runtime-storage-footprint-governance.prd.md#storage-accept-ac2) | [设计](#storage-case-ac2) | AC-2/SC-2：提交确认且清理成功后收敛到latest+rollback-safe及外部pin，sidecar≤16 MiB，orphan=0；暂态/降级保留保护数据 | [现有 test/manual](../../../crates/oasis7_runtime/src/tests/persistence.rs)；required 目标场景：AC-2/SC-2：提交确认且清理成功后收敛到latest+rollback-safe及外部pin，sidecar≤16 MiB，orphan=0；暂态/降级保留保护数据；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac3](runtime-storage-footprint-governance.prd.md#storage-accept-ac3) | [设计](#storage-case-ac3) | AC-3/SC-4：GC后latest restart，execution root/journal_len/module_registry相同 | [现有 test/manual](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/driver_authoritative_recovery.rs)；required 目标场景：AC-3/SC-4：GC后latest restart，execution root/journal_len/module_registry相同；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac4](runtime-storage-footprint-governance.prd.md#storage-accept-ac4) | [设计](#storage-case-ac4) | AC-4：GC中断/partialwrite/pin失败保当前index完整闭包；未知另保候选并隔离旧World写入 | [现有 test/manual](../../../crates/oasis7_runtime/src/tests/persistence.rs)；full 目标场景：AC-4：GC中断/partialwrite/pin失败保当前index完整闭包；未知另保候选并隔离旧World写入；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac5](runtime-storage-footprint-governance.prd.md#storage-accept-ac5) | [设计](#storage-case-ac5) | AC-5/SC-6：每policy-retained H由nearestC<=H+canonical log重建原root | [现有 test/manual](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/replay.rs)；required 目标场景：AC-5/SC-6：每policy-retained H由nearestC<=H+canonical log重建原root；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac6](runtime-storage-footprint-governance.prd.md#storage-accept-ac6) | [设计](#storage-case-ac6) | AC-6/SC-5：profile/effective_budget/bytes/retained/checkpoint/replay/lastgc/error/degraded可读，无额外扫描 | [现有 test/manual](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/driver_observability.rs)；required 目标场景：AC-6/SC-5：profile/effective_budget/bytes/retained/checkpoint/replay/lastgc/error/degraded可读，无额外扫描；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac61](runtime-storage-footprint-governance.prd.md#storage-accept-ac61) | [设计](#storage-case-ac61) | AC-6.1：四入口同枚举无损透传，三wrapper OASIS7_CHAIN_STORAGE_PROFILE非空注入/默认继承/bundle runtime | [现有 test/manual](../../../testing-manual.md)；required 目标场景：AC-6.1：四入口同枚举无损透传，三wrapper OASIS7_CHAIN_STORAGE_PROFILE非空注入/默认继承/bundle runtime；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac7](runtime-storage-footprint-governance.prd.md#storage-accept-ac7) | [设计](#storage-case-ac7) | AC-7/SC-3：2500 heights snapshot≤512KiB，archive区间读取+链路校验 | [现有 test/manual](../../../crates/oasis7_runtime/src/tests/persistence.rs)；required 目标场景：AC-7/SC-3：2500 heights snapshot≤512KiB，archive区间读取+链路校验；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-ac8](runtime-storage-footprint-governance.prd.md#storage-accept-ac8) | [设计](#storage-case-ac8) | AC-8：required/full分别含footprint/restart/failsafe/profile/archive/retained replay及独立G1/G2/G3/交叉矩阵 | [现有 test/manual](../../../testing-manual.md)；required 目标场景：AC-8：required/full分别含footprint/restart/failsafe/profile/archive/retained replay及独立G1/G2/G3/交叉矩阵；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-sc7](runtime-storage-footprint-governance.prd.md#storage-accept-sc7) | [设计](#storage-case-sc7) | SC-7：release_default hot_head_heights≤checkpoint_interval，不重复保留额外同区间snapshot | [profile 配置局部验证方法](../../../testing-manual.md#storage-profile-retention-density-validation)；`storage_profiles_scale_retention_density` 定义断言 `release_default.execution_hot_head_heights == 64` 且等于 `execution_checkpoint_interval`；64 仅为当前 source 定义，不新增固定合同数值。未运行；不证明实际保留窗口、exact-height restore、2500 heights footprint 或未重复保留 snapshot；required 目标场景：SC-7：release_default hot_head_heights≤checkpoint_interval，不重复保留额外同区间snapshot；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr1](runtime-storage-footprint-governance.prd.md#storage-accept-nfr1) | [设计](#storage-case-nfr1) | NFR-1：2500 heights node root≤384MiB且store≤256MiB | [现有 test/manual](../../../testing-manual.md)；required 目标场景：NFR-1：2500 heights node root≤384MiB且store≤256MiB；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr2](runtime-storage-footprint-governance.prd.md#storage-accept-nfr2) | [设计](#storage-case-nfr2) | NFR-2：2500 heights localSSD sweep p95≤500ms | [现有 test/manual](../../../testing-manual.md)；required 目标场景：NFR-2：2500 heights localSSD sweep p95≤500ms；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr3](runtime-storage-footprint-governance.prd.md#storage-accept-nfr3) | [设计](#storage-case-nfr3) | NFR-3：default latest restart p95≤5s | [现有 test/manual](../../../testing-manual.md)；required 目标场景：NFR-3：default latest restart p95≤5s；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr4](runtime-storage-footprint-governance.prd.md#storage-accept-nfr4) | [设计](#storage-case-nfr4) | NFR-4：优化不能降低replay/recovery确定性，相关回归100% | [现有 test/manual](../../../testing-manual.md)；required 目标场景：NFR-4：优化不能降低replay/recovery确定性，相关回归100%；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr5](runtime-storage-footprint-governance.prd.md#storage-accept-nfr5) | [设计](#storage-case-nfr5) | NFR-5：所有policy retained heights replay成功100%且原root相等 | [现有 test/manual](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/replay.rs)；required 目标场景：NFR-5：所有policy retained heights replay成功100%且原root相等；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr6](runtime-storage-footprint-governance.prd.md#storage-accept-nfr6) | [设计](#storage-case-nfr6) | NFR-6：metrics采集发布频率不高于每1s一次，额外CPU≤5% local单节点 | [现有 test/manual](../../../testing-manual.md)；required 目标场景：NFR-6：metrics采集发布频率不高于每1s一次，额外CPU≤5% local单节点；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-nfr7](runtime-storage-footprint-governance.prd.md#storage-accept-nfr7) | [设计](#storage-case-nfr7) | NFR-7：soak显式更高占用、metrics增长/可配上限 | [现有 test/manual](../../../testing-manual.md)；required 目标场景：NFR-7：soak显式更高占用、metrics增长/可配上限；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-compression](runtime-storage-footprint-governance.prd.md#storage-accept-compression) | [设计](#storage-case-compression) | 原payload content_hash不变，compressed header后更小才压缩，读回rawbytes | [现有 test/manual](../../../testing-manual.md)；required 目标场景：原payload content_hash不变，compressed header后更小才压缩，读回rawbytes；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-cold-range](runtime-storage-footprint-governance.prd.md#storage-accept-cold-range) | [设计](#storage-case-cold-range) | continuous latest hot range/shared coldanchor scan seek同边界，pack len/offset/hash与canonical/legacy别名回填 | [现有 test/manual](../../../testing-manual.md)；required 目标场景：continuous latest hot range/shared coldanchor scan seek同边界，pack len/offset/hash与canonical/legacy别名回填；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-legacy-height](runtime-storage-footprint-governance.prd.md#storage-accept-legacy-height) | [设计](#storage-case-legacy-height) | v3新写/V1V2兼容，stale/noncontiguous heightfailclosed；legacy仅latest-safe无aggressiveGC | [现有 test/manual](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/driver_authoritative_recovery.rs)；required 目标场景：v3新写/V1V2兼容，stale/noncontiguous heightfailclosed；legacy仅latest-safe无aggressiveGC；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-save-failure](runtime-storage-footprint-governance.prd.md#storage-accept-save-failure) | [设计](#storage-case-save-failure) | save满盘/新generation中断按G1区分三态，保护材料不恢复旧写权，维护失败不撤销提交；pin/latestmissing停止，replay mismatch阻aggressiveGC | [现有 test/manual](../../../crates/oasis7_runtime/src/tests/persistence.rs)；full 目标场景：save满盘/新generation中断按G1区分三态，保护材料不恢复旧写权，维护失败不撤销提交；pin/latestmissing停止，replay mismatch阻aggressiveGC；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-single-writer](runtime-storage-footprint-governance.prd.md#storage-accept-single-writer) | [设计](#storage-case-single-writer) | 仅runtime指定根删除，显式pin/无mtime猜测，第二writer检测阻止、metrics不新增payload | [现有 test/manual](../../../testing-manual.md)；required 目标场景：仅runtime指定根删除，显式pin/无mtime猜测，第二writer检测阻止、metrics不新增payload；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-version-dependency](runtime-storage-footprint-governance.prd.md#storage-accept-version-dependency) | [设计](#storage-case-version-dependency) | retained原manifest/artifact/moduleanchor唯一解析，missing/hashconflict停止unsafe sweep/不称matched | [现有 test/manual](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/replay.rs)；required 目标场景：retained原manifest/artifact/moduleanchor唯一解析，missing/hashconflict停止unsafe sweep/不称matched；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
| [professional_acceptance: storage-accept-service-boundary](runtime-storage-footprint-governance.prd.md#storage-accept-service-boundary) | [设计](#storage-case-service-boundary) | localroot/restart仅历史条件，append/finality/version/head与redundantarchive仍外部 | [现行 manual](../../../testing-manual.md)；精确局部 source ../../testing/templates/state-sync-closure-evidence-packet-template.md（定义/入口，非执行证据）；full 目标场景：localroot/restart仅历史条件，append/finality/version/head与redundantarchive仍外部；固定2500 heights/profile/localSSD或适用fault/targetH，同candidate/config/world/window采样；source为局部定义/现行manual入口，未运行，无阈值通过声明 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 定义/局部material不证明量化阈值/BFT/服务 |
