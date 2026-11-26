# BFT 受保护签名器最小可信基（Minimal TCB）与职责解耦设计

- 状态：`target`（中期演进设计，尚未实现；不替代现行 `wire=3` / `#5518` 交付合同）
- 所属模块与专题：`p2p` / `consensus`
- Owner role：`runtime_engineer`（协同 `blockchain_ops_engineer`、`qa_engineer`）
- 审读基线：`27bfa563d30fea72a5bf67cbf5e736ba18a19681`
- 上游权威：[`doc/p2p/prd.md`](../prd.md#p2p-target-bft)、[`doc/p2p/design.md`](../design.md#des-p2p-bft-protected-signer-v1)、[`分布式共识与状态可用性 PRD`](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-004)

## 1. 问题、目标与非目标

### 1.1 问题与背景

作为持久工业模拟与智能体协作游戏的基础设施，Oasis7 需要高频、确定性且运维可控的权威提交底座。现行 `protected_bft_v3`（`crates/oasis7_distfs/src/protected_bft_v3/backend.rs` 中的 `BftSignerBackendV3`）在演进过程中将五类异构职责叠加进了同一个受保护原子后端：

1. 不可导出 Ed25519 私钥签名；
2. 跨进程租约抢占与 `lease_generation` fencing；
3. 最多 64 个高度的完整 `completed_journals`（单 journal 上限 16 MiB）、证书历史与成员转换归档 CAS；
4. 私有候选保留原始输入对象（`BftPrivateRetentionAdmissionInputV1`，依赖闭包上限 64 MiB）与响应流量预算账本（`reserved_response_bytes`）的原子提交；
5. 每 5–100 ms 空闲轮询的远程受保护单调时钟与独立持久时间水位（`protected_time_ms`）。

这种“超级原语”使受保护签名器的状态从行业常规的百字节级膨胀到数十 MiB 级，直接偏离了 [`des-p2p-bft-header-node-v3`](../design.md#des-p2p-bft-header-node-v3) 中“私有输入存储绝不发送给受保护签名器、远程受保护签名器仅接收公开协议事件/意图”的初衷，导致单一硬件 HSM/KMS 无法直接承载，也引入了不必要的热路径序列化与远程时钟 I/O 开销。

横向对照成熟开源实践（CometBFT `tmkms`、Strangelove `Horcrux`、Aptos `safety-rules`、Consensys `Web3Signer` 以及全链游戏引擎的世界分片架构），共识签名可信基（TCB）均严格收敛在最小防双签水位与锁状态，将区块/候选历史存储与超时活性调度留在外层节点。

### 1.2 设计目标

本设计定义中期受保护签名器瘦身目标（Minimal Signer TCB）：

- **收敛签名器 TCB 至 `< 4 KiB`**：受保护后端仅持久化固定绑定摘要、排他租约代次、Tendermint Algorithm 1 最小防双签/锁定水位、当轮幂等签名槽及外部保留状态根的 32 字节摘要锚点。
- **剥离大体积存储与流量计费账**：将 64 高度历史 journal、私有候选原始输入字节（上限 64 MiB）及响应传输配额账移回 Node 本地的 `oasis7_cas` / `oasis7_distfs` 持久归档层，签名器仅单向锚定其承诺摘要而不再存储原始大对象。
- **分离共识安全性（Safety）与超时活性（Liveness）时钟**：将常规高度/轮次超时调度迁回 Node 本地单调时钟，取消共识空闲路径对远程受保护时钟的高频持久化轮询；仅在低频离线检查点续期与证书有效期边界保留可信时间水位校验。

### 1.3 非目标

- 本设计不修改产品层最终性、恢复闸门或 `H/H+1` 权威交接承诺。
- 本设计不自动豁免、缩减或关闭现行 `bft/version=1/wire=3` 下的 `#5518` / `#4975` 验收义务；现行 `wire=3` 生产落地仍按既有 `BftSignerBackendV3` 契约执行，本设计通过独立新后端契约与显式版本命名空间分阶段演进。
- 本设计文档合入不代表新精简接口、迁移切片或运行证据已实现。

## 2. 上游约束与相关角色

`runtime_engineer` 拥有共识状态机、签名器协议与本地 CAS/归档边界；`blockchain_ops_engineer` 拥有 KMS/HSM、轻量状态后端与故障域部署；`qa_engineer` 拥有双签拒绝、崩溃恢复与跨版本兼容回归判定；`producer_system_designer` 拥有基础设施产品层 `fail-closed` 承诺。

### 2.1 需求承接与分配表

Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46, O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: req-dcs-004](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-004) | 签名前持久保存轮次/阶段/锁定水位与原字节决定，同槽幂等重发、异字节与旧租约拒签，压缩签名状态至 `< 4 KiB` 最小可信基；专业来源：[p2p-target-bft](../prd.md#p2p-target-bft) | [des-bft-minimal-safety-tcb](#des-bft-minimal-safety-tcb) | `oasis7_consensus` / `oasis7_distfs`；ops KMS/轻量状态库；QA 防双签回归 | `target` 未实现；不替代多节点网络分区与治理转换的完整组合验收 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[product_requirement: req-dcs-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001) | 私有候选原始字节与传输预算归位 Node CAS/归档层，签名器单向锚定保留头摘要以防未授权伪造；专业来源：[p2p-full-sync-capacity-checkpoint](../prd.md#p2p-full-sync-capacity-checkpoint) | [des-bft-storage-accounting-decoupling](#des-bft-storage-accounting-decoupling) | `oasis7_cas` / `oasis7_distfs` / `oasis7_node`；QA 删库重建与冷恢复负例 | `target` 未实现；不改变 512 MiB 材料上限与 4 GiB 传输预算规则本身 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: req-dcs-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) | 64 高度历史归档与签名器单向锚点在冷重开时严格对账，缺损或回滚原件保持 `fail-closed` 停签；专业来源：[p2p-full-sync-capacity-checkpoint](../prd.md#p2p-full-sync-capacity-checkpoint) | [des-bft-storage-accounting-decoupling](#des-bft-storage-accounting-decoupling) | `oasis7_cas` / `oasis7_distfs` / `oasis7_node`；QA 冷恢复对账 | `target` 未实现；不替代跨节点全量状态同步恢复演练 |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: ac-dcs-004](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-004) | 共识轮次超时迁回 Node 本地单调时钟，移除空闲路径远程持久时钟轮询，低频 checkpoint/授权边界保留可信时间校验；专业来源：[轮次与锁定](../design.md#des-p2p-bft-round-lock-v1)、[checkpoint 更新](../design.md#des-p2p-bft-checkpoint-renewal-v1) | [des-bft-clock-liveness-separation](#des-bft-clock-liveness-separation) | `oasis7_consensus` / `oasis7_node`；ops 外部 checkpoint 时间源；QA 异步时钟漂移与活性验证 | `target` 未实现；本地时钟仅驱动 round timeout 活性，绝不解锁 `lockedValue` 或延长离线根策略有效期 |

## 3. 当前状态、目标状态与差距

| 对象/能力 | 当前状态（基线 `27bfa563`） | 目标状态（Minimal TCB） | 差距与迁移前提 | 证据或 owner |
| --- | --- | --- | --- | --- |
| 受保护签名状态大小与结构 | `BftSignerStateV3` + `BftFirstReserveSnapshotV1` 含 64 高度 journal、证书历史、私有保留原始字节与流量账，峰值达数十 MiB | `BftMinimalSafetyState` 固定 `< 4 KiB`（绑定摘要、租约代次、高水位、Algorithm 1 锁与 32B 摘要锚点） | 需新增独立最小后端 trait 与版本化域，完成一致性向量测试后切换调用方 | `runtime_engineer`；当前未实现 |
| 私有候选输入与流量账归属 | `compare_and_commit_first_reserve` 要求受保护后端同时存储原始输入字节与 `reserved_response_bytes` | 原始字节与流量账留在 Node 受控归档/CAS；签名器仅单向推进 `retention_head_digest` 与序列号 | 需在 Node 提交预约前先落盘本地 CAS pin 并生成确定性摘要锚点 | `runtime_engineer`；当前未实现 |
| 共识超时与可信时间边界 | 高度计时原点与每 5–100 ms 空闲轮询均经远程 `protected_time_ms` 推进持久 floor | 轮次超时由 Node 本地单调时钟驱动；可信 UTC/floor 仅用于低频 checkpoint 续期与证书有效期校验 | 需将 reducer 超时触发器与远程 signer 解耦，保留 Algorithm 1 权重门槛启动规则 | `runtime_engineer` / `qa_engineer`；当前未实现 |

## 4. 边界与结构

解耦后的验证者节点分为三个职责正交的层次：

- **受保护最小签名可信基（Protected Minimal Signer TCB）**：唯一持有或调用不可导出验证者共识私钥的边界。只消费公开共识头部摘要、轮次锁证明摘要与单调保留锚点摘要，输出域分隔 Ed25519 签名；不读取、不存储、不传输任何私有业务动作字节或完整历史区块。
- **Node 权威存储与私有归档层（Node CAS & Private Retention Archive）**：位于 `oasis7_cas` / `oasis7_distfs`，负责完整 64 高度历史证书、已完成签名日志镜像、上限 64 MiB 的私有候选输入闭包及响应流量预算账本（`reserved_response_bytes`）的本地原子落盘与跨副本同步。
- **Node 共识调度与执行驱动层（Consensus Reducer & Execution Driver）**：位于 `oasis7_consensus` / `oasis7_node` / `oasis7_chain_runtime`，负责父状态重执行、达到 `q` 投票权重后的本地单调超时计时、候选选择以及向最小签名器发起幂等签名请求。

<a id="des-bft-minimal-safety-tcb"></a>
### 4.1 `des-bft-minimal-safety-tcb`：`< 4 KiB` 最小防双签与锁定状态

最小签名器的持久快照严格限制在固定尺寸上限（canonical CBOR 编码 `<= 4096` 字节），仅包含以下不可裁剪的安全字段：

1. **不可变身份绑定**：`schema_version`、`binding_digest`（承诺 `world_id`、`chain_id`、`activation_digest`、协议版本与验证者公钥）及不可擦除的 `private_store_instance_digest`。
2. **排他实例 Fencing**：单调递增的 `lease_generation: u64` 与当前活跃 `lease_token_digest`。常规 `PrepareAcquire -> Acquire` 不得抢占仍活跃的未释放租约（返回冲突）；仅在干净释放或显式受控会话/运维 fencing 后递增代次，新代次生效后旧代次的任何签名或状态推进请求立即拒绝。
3. **Tendermint Algorithm 1 安全水位、排程与锁**：
   - 当前 `authority_epoch`、`active_validator_set_hash`、`schedule_digest`（承诺 `starting_priority` 及 `h -> h+2` 跨 `h+1` 传递的 `pending_transition`，确保外置历史 QC 后仍严格执行确定性 proposer 校验与 `h+2` 集合转换）、本机 `membership_install_watermark`、`height`、`round`；
   - 当前 `(height, round)` 已占用的阶段槽位（`propose` / `prevote` / `precommit`，含 `nil` 票决定、有界待签/已签原始头 preimage 及 64 字节原签名缓存，支持网络丢包后的同槽原字节幂等重发）；
   - `locked_round` / `locked_value_digest` 与 `valid_round` / `valid_value_digest`（严格遵循 [`des-p2p-bft-round-lock-v1`](../design.md#des-p2p-bft-round-lock-v1)）：
     - **`propose` 校验**：若 `valid_value_digest.is_some()` 或提案头携带 `valid_round`，须核对提案值与对应高度 `valid_round` 的法定权重（`floor(2W/3)+1`）`prevote` 证明一致；
     - **跨轮 `prevote` 解锁条件**：当存在旧锁且拟投异值时，调用方必须附上 `locked_round <= valid_round < round` 的法定权重 `prevote` 签名证明；通过后仅允许签发 **`prevote(round, Block(v))`**（不直接改锁或签发 `precommit`）；
     - **每次非 `nil` `precommit` 的当轮 Polka 与无条件锁推进**：**每一次** `precommit(round, Block(v))`（包含首次上锁 `locked_value_digest == None` 以及在更高 `round` 对同一已锁定值 `v` 重新 `precommit`）均必须核验**当前轮 `round` 本身**针对 `(height, round, Block(v))` 的法定权重 `prevote` 证明，并在同一事务中无条件更新 `locked_round = Some(round), locked_value_digest = Some(v)` 与 `valid_round = Some(round), valid_value_digest = Some(v)`。
4. **单向外部保留与时间/续期防回滚水位**：
   - 保留锚点：`committed_header_id`、`retention_sequence: u64`、`retention_head_digest` 与 `successor_minimum_sequence: u64`（沿合法前驱单向递增，杜绝本地删库重建后复用旧签名器身份）；
   - 续期与撤销水位（承接 [`des-p2p-bft-checkpoint-renewal-v1`](../design.md#des-p2p-bft-checkpoint-renewal-v1)，总计 `< 128` 字节）：`utc_lower_floor_unix_ms`、`clock_epoch` / `time_generation`、`trust_update_sequence` 及发行方撤销状态最低水位，每次签发提案或投票前在受保护签名器内重验有效窗口与撤销门槛，防止本地盘回滚重放过期策略或已撤销授权。

上述全部固定尺寸字段合计远小于 `4 KiB`，生产环境可直接采用以下任一轻量高可靠形态落地，无需特种大存储硬件：
- **轻量多副本共识签名簇（Horcrux 模式）**：3 或 5 个独立微实例通过 Raft 同步 `< 4 KiB` 水位并配合门限签名或 KMS；
- **KMS/HSM + 单行强一致键值/关系表（Web3Signer / Aptos 模式）**：不可导出私钥留在 `LunaBftSigningKeyV3` / Cloud KMS / Vault Transit，`< 4 KiB` 状态作为单行记录存于独立托管 PostgreSQL `SERIALIZABLE` 表或 etcd 键值项中。

<a id="des-bft-storage-accounting-decoupling"></a>
### 4.2 `des-bft-storage-accounting-decoupling`：私有保留大对象与流量账回归存储层

为恢复签名器零私有载荷边界并消除大体积 CAS 瓶颈：

1. **先本地耐久 Pin，后锚定摘要签名**：Node 在请求提案签名或切换保留窗口前，先在本地受控私有归档目录与专用 CAS 中完成原始动作、依赖闭包（上限 64 MiB）及保留映射表的 `fsync` 落盘，计算包含前驱哈希链的 `retention_head_digest` 与单调 `retention_sequence`，仅将该 32 字节摘要随签名意图提交给受保护签名器。
2. **防删库重建不变量保留**：签名器在 `BftMinimalSafetyState` 中持久记录 `private_store_instance_digest` 与最新 `retention_head_digest`。若验证者本地私有归档盘丢失、回滚或被清空重建，Node 重启后本地归档头将落后于签名器记录的 `retention_head_digest`（或实例身份不匹配）；启动对账必须在开放投票或服务前触发 `fail-closed` 停签，直至从合法授权备份完整恢复精确原件并核验全部 BLAKE3 摘要通过。
3. **响应流量预算独立持久化**：候选与归档读取的累计响应消耗（`reserved_response_bytes`，4 GiB 上限及不退款预留规则）属于数据面防滥用与容量控制，移回 Node 归档读取网关的持久配额账本维护。配额耗尽阻塞新读取与新提案准入，但不再耦合进共识私钥的每一次投票 CAS 事务。
4. **历史签名日志外置与排程锚点延续**：已完成高度的 `completed_journals` 与 64 高度认证头前缀保存在 Node 本地 WAL 镜像与归档库供审计和对外证明服务使用；签名器在对照当前 `schedule_digest` 验证本高度合法 QC 后，确定性导出下一高度 `schedule_digest`（保留 `h -> h+2` 待激活集合转换并核验 `h+2` 安装回执水位）并推进 `height` 与 `committed_header_id`，不再在受保护状态内保留过去 64 个高度的全量日志副本。

<a id="des-bft-clock-liveness-separation"></a>
### 4.3 `des-bft-clock-liveness-separation`：共识超时活性与可信时间水位分离

依据 Tendermint Algorithm 1，共识安全性（不在同一高度产生两个有效提交证书）完全由法定权重交集、轮次单调水位与锁定规则保证，不依赖节点间或硬件时钟同步；时钟仅决定何时触发超时进入下一 `round`（活性）：

1. **移除空闲路径远程时钟轮询**：`propose`、`prevote`、`precommit` 超时计时器改由 Node 进程本地单调时钟（OS monotonic `Instant`）按既有权重门槛规则（观察到同阶段任意值 `>= q` 权重后启动，时长随 `round` 退避）驱动，彻底移除每 5–100 ms 调用远程 `protected_time_ms` 更新持久时间 floor 的热路径 I/O。
2. **本地时钟异常不影响安全性**：即便验证者本机时钟快进或停滞，最坏情况仅导致该节点提前投出合法 `nil` 票或超时慢于对等节点，签名器的单调 `(height, round, phase)` 槽与当轮/跨轮 Polka 锁定规则仍严格阻止任何冲突双签或非法解锁。
3. **低频治理与续期边界严格保留策略锚定的可信 UTC**：离线检查点续期（[`des-p2p-bft-checkpoint-renewal-v1`](../design.md#des-p2p-bft-checkpoint-renewal-v1)）、治理根策略有效期与撤销状态校验继续严格消费策略固定公钥签名的 `BftCheckpointTimeObservationV1`（`bounded-utc-observation-v1` 与 `protected-elapsed-upper-bound-v1`）及签名器内持久化的 `utc_lower_floor_unix_ms` / 撤销水位；严禁将本机未认证 wall-clock、普通 NTP 回复或未签名数据库服务端单调时钟强转充当 UTC 授权依据。

## 5. 关键运行流程

1. **冷启动与租约对账**：Node 读取本地私有归档与 CAS，先校验世界身份、H/前驱链与材料完整性；随后连接最小受保护签名器执行 `PrepareAcquire -> Acquire`（仅在无活跃租约或已完成受控 fencing 后递增 `lease_generation`）。Node 比对本地 WAL/归档水位与签名器返回的 `(height, round, schedule_digest, retention_sequence, retention_head_digest)`：若签名器水位更高且存在未广播原签名则补发续办，若本地缺失签名器已锚定的保留原件或提交历史则立即 `fail-closed` 阻断启动。
2. **提案/投票签名热路径**：Node 本地完成父状态重执行与私有候选持久化 Pin → 组装固定尺寸请求（含目标 `(height, round, phase)`、有界待签头 preimage、必要的 `valid_round` 或当轮 `round` 法定权重 `prevote` 证明、单调 `retention_head_digest`）→ 签名器在 `< 4 KiB` 原子事务中核对 `lease_generation`、`utc_lower_floor_unix_ms`/撤销门禁、`schedule_digest`、Algorithm 1 锁转移与保留锚点单调性 → 调用不可导出密钥对完整域分隔 preimage 签名并写入当轮幂等缓存 → 返回 64 字节签名。
3. **崩溃与丢包重试**：若签名已提交而网络响应丢失，Node 重连并取得新租约后重提同一 `(height, round, phase)` 及相同原始 preimage 请求，签名器命中当轮幂等缓存直接返回原签名；若尝试提交异字节候选则被明确拒签。

## 6. 接口与数据合同

| 接口/条款 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| 最小安全后端契约（拟名 `BftMinimalSignerBackendV1`） | `RemoteBftSignerService` → KMS/轻量事务存储 | 独立命名空间 `oasis7.bft.protected-signer.minimal.v1`，canonical CBOR `<= 4 KiB` | `lease_generation` 严格单调且不抢占活跃租约；`(height, round, phase)` 同 preimage 幂等返回原签名，异字节拒绝 | 事务提交并精确读回确认后才释放签名；存储、可信 UTC 水位或密钥不可证返回明确错误并停签 | 与 `v3` 命名空间完全隔离，禁止原地覆盖或自动降级 |
| 保留摘要锚定接口 | `Node` 归档层 → 最小签名服务 | 绑定 `private_store_instance_digest` 与单调 `retention_sequence` | `retention_sequence` 只增不减；同序列须同 `retention_head_digest` | 锚点倒退或分叉立即拒签并标记实例需人工核查 | 外层候选包 wire 与归档哈希域保持不变 |

## 7. 状态、事务与持久化

- **单一 `< 4 KiB` 原子提交点**：`lease_generation`、`schedule_digest`、`(height, round, phase)` 槽、Algorithm 1 锁更新、当轮原签名缓存、`retention_head_digest` 及 `utc_lower_floor_unix_ms`/撤销水位必须在同一次后端 CAS 中原子更新。
- **两步操作顺序**：当不可导出密钥托管面（对完整域分隔字节执行 PureEdDSA `sign_preimage(&[u8])`）与 `< 4 KiB` 状态存储面分离时，第一步 CAS 必须先在 `< 4 KiB` 记录内原子提交更新后的 Algorithm 1 锁、`retention_sequence`/`retention_head_digest` 以及有界（`< 512 B`）待签原始 preimage 与 `preimage_digest`（`ReservedIntent`），第二步再调用 KMS/HSM 对该精确 preimage 签名并写回缓存；中断重启后若发现未完成 `ReservedIntent`，只允许对该已持久化的精确 preimage 重新获取确定性 Ed25519 签名完成闭环，绝不允许取消、退锁或改签其他值。

## 8. 部署、安全与运行约束

- 验证者节点进程仍不得持有共识裸私钥文件，生产 CLI 不提供内存或验证者同盘普通文件作为签名状态后端的回退开关。
- 最小状态存储仍须位于与验证者本地业务盘独立的故障域（如独立托管事务库或 3 节点轻量 Raft cosigner 组），以防验证者整机虚拟机快照回滚导致双签或回退 `utc_lower_floor_unix_ms` / 撤销水位。

## 9. 质量与容量

- **状态体积上限**：受保护状态单条记录严格 `<= 4 KiB`，消除 16–64 MiB 大对象在签名器 RPC 与持久事务中的复制峰值。
- **空闲轮询开销**：稳态无新区块事件或等待投票期间，签名器后端磁盘/网络轮询频率降为 `0`（仅维持轻量连接保活，不触发持久 floor 写事务）。

## 10. 兼容、迁移与回滚

演进遵循仓库 `[Slice: Dark] -> [Slice: Adapter] -> [Slice: Cutover]` 规范分批实施，不在现行 `wire=3` 活跃路径原地大改：

1. **`[Slice: Dark]`**：在 `oasis7_consensus` / `oasis7_distfs` 新增独立的 `< 4 KiB` 最小安全状态结构、当轮/跨轮轻量 Polka 证明校验、`schedule_digest` 跨高度推进与一致性契约测试套件，零现网入口改动。
2. **`[Slice: Adapter]`**：实现基于“不可导出 KMS/HSM 句柄 + `< 4 KiB` 强一致存储（或 Raft cosigner）”的适配器，以及 Node 本地私有归档先 Pin 后传摘要的桥接层，支持与旧 `v3` 配置并存验证。
3. **`[Slice: Cutover]`**：在显式新配置版本下将验证者启动入口切至最小签名器路径；已启用新命名空间的验证者禁止回退至旧 `v3` 命名空间，代码回滚若遇更高受保护水位须按 `fail-closed` 保持停签。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: req-dcs-004](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-004) | [des-bft-minimal-safety-tcb](#des-bft-minimal-safety-tcb) | `< 4 KiB` 状态完整覆盖同槽幂等重发、异字节双签拒绝、`valid_round < round` 仅放行 `prevote`、每次非 `nil` `precommit` 强制核验当轮 Polka 并推进 `locked_round`、`schedule_digest` 跨 `h -> h+2` 集合转换锚定及活跃租约保护（承接 [p2p-target-bft](../prd.md#p2p-target-bft)） | Planned 单元与契约回归（承接 [S4 手册](../../testing/manual/testing-manual-s4-distributed-and-governance-drills.manual.md) 与 [testing-manual.md](../../../testing-manual.md) S9A）：核对最大合法状态编码 `<= 4096` 字节、旧轮 Polka 直接请求 `precommit` 被拒、同锁定值重签 `precommit` 必须推进 `locked_round`、`h+2` 未完成安装拒签、签名前后中断恢复及活跃租约不可被抢占 | 新切片 PR 的测试命令、退出码、状态编码字节上限断言与真实后端冷重开证据 | 当前仅为目标设计，尚未编写实现或执行测试 |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[product_requirement: req-dcs-001](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001) | [des-bft-storage-accounting-decoupling](#des-bft-storage-accounting-decoupling) | 签名器零私有载荷接收；伪造或未锚定归档不可绕过签名器提交证明（承接 [p2p-full-sync-capacity-checkpoint](../prd.md#p2p-full-sync-capacity-checkpoint)） | Planned 集成负例（承接 [S4 手册](../../testing/manual/testing-manual-s4-distributed-and-governance-drills.manual.md)）：构造异源或篡改摘要的私有归档目录尝试请求签名，验证签名器按 `private_store_instance_digest` / `retention_head_digest` 拒绝 | 请求拒绝记录与零冲突签名证据 | 当前未实现；不替代跨节点全量同步验证 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[product_requirement: req-dcs-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) | [des-bft-storage-accounting-decoupling](#des-bft-storage-accounting-decoupling) | 本地私有归档删库重建、截断或摘要落后于签名器 `retention_head_digest` 时启动 `fail-closed` 停签（承接 [p2p-full-sync-capacity-checkpoint](../prd.md#p2p-full-sync-capacity-checkpoint)） | Planned 恢复负例（承接 [S4 手册](../../testing/manual/testing-manual-s4-distributed-and-governance-drills.manual.md)）：提交非空私有候选后删除/回滚 Node 本地归档目录再重启，验证因签名器锚点领先而拒绝投票，补齐精确原件后恢复续跑 | 归档篡改/回滚前后的启动阻断日志、签名器锚点快照与恢复后精确读回结果 | 当前未实现；不替代跨节点状态同步全量容量演练 |
| Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[product_requirement: ac-dcs-004](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-004) | [des-bft-clock-liveness-separation](#des-bft-clock-liveness-separation) | 本地时钟快进/慢走不破坏共识安全性（零双签、不非法清锁），空闲等待零远程时钟写事务，离线 checkpoint 续期与撤销仍严格校验签名 `BftCheckpointTimeObservationV1` 与受保护 `utc_lower_floor_unix_ms`（承接 [轮次与锁定](../design.md#des-p2p-bft-round-lock-v1)、[checkpoint 更新](../design.md#des-p2p-bft-checkpoint-renewal-v1)） | Planned 状态机对抗测试与 I/O 计数断言（承接 [testing-manual.md](../../../testing-manual.md) S9A）：注入极端本地时钟偏移验证无冲突签名，统计空闲窗口签名后端写事务数为 `0`，并验证未签名服务端时间戳、回放旧观测或过期 checkpoint 均被签名器拒绝 | 状态机对抗用例输出、后端事务计数断言与 checkpoint/UTC 负例拒绝记录 | 当前未实现；不承诺极端网络分区期间的出块活性时延 |

## 12. 决策、长期风险与未决问题

- **核心架构取舍**：以“Node 本地耐久归档 + 签名器 `< 4 KiB` 摘要锚定（含 `schedule_digest`、`retention_head_digest` 与续期/撤销 floor）”替代“将数十 MiB 业务保留状态塞入签名器原子 CAS”。这消除了对不存在的“全能大存储硬件 HSM”的依赖，同时完整保留了防双签、防 `h+2` 集合转换遗忘、防续期回滚与防本地删库伪造的安全边界。
- **当轮/跨轮 Polka 验证开销**：由于签名器不再缓存完整历史高度日志，跨轮 `prevote` 解锁与每次非 `nil` `precommit` 请求须附带对应 `(height, valid_round)` 或 `(height, round)` 的去重 `prevote` 签名证明（256 成员上限内编码仍远小于数十 KiB，验签通过后无需落盘整包票据，仅原子更新 `< 4 KiB` 水位与锁字段）。
- **与 `#5518` 的衔接边界**：在后续专门立项实施本演进设计前，`#5518` 仍按现行 `BftSignerBackendV3` 接口推进交付；不得以本中期目标文档尚未实施为由阻塞当前任务，也不得以本文档合入宣称 `#5518` 已完成。
