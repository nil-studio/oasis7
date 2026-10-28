# P2P / 区块链签名策略与可恢复性硬化

历史编辑/阶段边界：原 MIG/T0/Phase 批次尚未逐项匹配唯一实施 Task，故原记录与约束映射保留；文档人工重写、章节/引用检查和完成日期只证明原编辑或测试窗口，不生成当前排期、不代签机制、部署或 QA readiness。当前技术与安全合同继续由本文及配对设计持有；后续治理 Task 不冒充全部原实施者。

> 历史整合说明：本专题整合 2026-02-16 至 2026-02-17 的 `blockchain-p2pfs-hardening-phase2` 至 `phase8`。它是这些已完成阶段的专业权威与追溯入口；阶段完成记录不构成当前 mainnet、生产恢复或 release readiness 结论。

- 对应设计文档：`doc/p2p/blockchain/p2p-blockchain-p2pfs-hardening.design.md`
- 对应GitHub Issue/Project task truth：GitHub Issue / GitHub Project

## 1. 目标

本专题固定 Node PoS、membership 与 sequencer 的签名兼容、信任边界和状态恢复合同：在不改既有 `signature` 线协议字段的前提下，保留 HMAC 兼容路径，同时为 ed25519、keyring 和规范化 allowlist 提供 fail-fast 策略。

## 2. 范围

这不是生产 signer custody、CA/证书链、HSM/KMS、多签、密钥轮换审批平台或创世 ceremony 的完成声明。当前系统级安全/readiness 仍以 `p2p-mainnet-security-governance-readiness.prd.md` 的 `not_mainnet_grade` 边界为准。

## 3. 接口 / 数据与当前运行合同

### Node PoS 状态与启动恢复

- 状态文件为 `<replication_root>/node_pos_state.json`，保存 next/committed height、slot、广播高度与 execution binding 等恢复所需状态。
- **缺失状态文件**本身允许默认初始化；没有可用 execution latest 或 latest 不属于当前 world 时，不创建伪造状态。
- **有效但落后的同 world 状态**可由 `execution_records/latest.json` 向前 reconciliation：最新 execution binding 完整时，committed/network height 与后续高度被原子更新。
- **损坏、不可读或不可解析的状态文件**为 fail-closed 错误，不能把重启、默认初始化或覆盖写入当作恢复替代。必须先保全证据并按运行手册/运行时恢复流程处理。
- 上述合同的验证锚点为 `crates/oasis7_chain_runtime/src/host/startup_reconcile.rs` 及其单元测试；Phase 2 的“损坏文件回退默认启动”表述仅是历史设计意图，已被当前行为取代。

### Action / Head 签名

- ed25519 签名串固定为 `ed25519:v1:<public_key_hex>:<signature_hex>`；公钥为 32-byte hex，签名字节为 64-byte hex；签名载荷为清空 `signature` 字段后的 canonical CBOR。
- `ActionEnvelope` 与 `WorldHeadAnnounce` 保持既有字段格式。`SequencerMainloopConfig` 可同时配置 HMAC 和 ed25519；迁移期间不应把历史 HMAC 路径错误描述为已删除。
- 当 `require_action_signature=true` 时，配置必须具备 HMAC signer 或非空的 action signer allowlist；否则配置校验 fail-fast。无 allowlist 时保持既有兼容语义，不得把它误写成无条件 ed25519 强制。

### Membership 签名、keyring 与恢复策略

- `MembershipDirectorySigner` 支持 HMAC 与 ed25519；snapshot/revocation 的签发和验签通过 keyring 支持 active key、多 key 验签、`signature_key_id`、吊销 key 拒绝及轮换窗口。
- restore policy 的 `require_signature`、`require_signature_key_id`、非空 accepted key-ID 或 signer-public-key allowlist 均为认证约束：必须有 signature 和可用且适用的 signer/keyring 完成密码学验证，否则在应用 membership 变更前拒绝。公钥 allowlist 非空时还必须为 `ed25519:v1` 且规范化 signer 公钥命中；声明公钥匹配不是认证，不代替验签。提供 verifier 时即使无额外策略也必须验证已有 signature。全部认证约束为空且未提供 verifier 的显式调用保留旧非认证诊断兼容，只说明策略检查通过，不能作为已验证身份或正式 BFT authority。该收紧改变此前 allowlist-only 或 require-key-ID-only 可无验证恢复的库 API 行为，不改变 wire、签名域或 BFT finality。revocation 同步的公开入口仍由其 keyring 验证，不能从 restore 的诊断兼容推导撤销权限。
- 公钥与 allowlist 统一 trim、非空检查、hex/32-byte 检查、小写规范化和去重；重复、空值或非法值在加载/验证阶段 fail-fast，错误保留字段或索引定位信息。membership 与 sequencer 必须使用同一比较语义。
- key-ID 信任、吊销同步、requester 信任与签名策略为组合边界；allowlist 不能替代 keyring、key-ID 或 revocation 校验。

### 复制请求与状态污染防护

- replication record 的 guard 只可在 hash 校验与存储写入成功后提交；任何失败都不得污染外部 guard 或伪造已推进状态。
- `SingleWriterReplicationGuard` 拒绝空 world/writer、`epoch < 1` 或 `sequence < 1`。同一 writer、同一 epoch 的 sequence 必须严格递增；epoch 前进或 writer 切换时必须从 sequence 1 开始，且 epoch 不得回退。
- `replay_replication_records` 必须按输入顺序逐条复用与在线写入相同的 `apply_replication_record` 路径；任一记录校验、hash 或写入失败即停止，不能跳过失败项或提前推进 guard。
- remote writer 必须同时通过签名、writer identity binding 与 allowlist 校验；fetch-commit/fetch-blob 请求必须接受受控鉴权，缺失或无效鉴权应拒绝。
- network subscription/outbox 必须有界；饱和时保留可诊断的失败或拒绝，不得以无界缓存掩盖背压。
- membership DHT restore 的默认路径保持签名要求；宽松策略只能通过显式调用方选择，不能作为静默 fallback。

## 4. 里程碑与历史阶段证据交叉表

| 历史阶段 | PRD-ID | 完成日期 | 保留的结果/决策 |
| --- | --- | --- | --- |
| Phase 2 | `PRD-P2P-MIG-046` | 2026-02-16 | gossip 签名闭环、PoS 状态持久化与重启续跑；损坏文件 fallback 仅为已取代的历史表述。 |
| Phase 3 | `PRD-P2P-MIG-047` | 2026-02-17 | Action/Head ed25519 signer、canonical 签名串、HMAC 双栈兼容与 sequencer 接线。 |
| Phase 4 | `PRD-P2P-MIG-048` | 2026-02-17 | membership snapshot/revocation 双栈 signer、keyring、publish/sync 接线。 |
| Phase 5 | `PRD-P2P-MIG-049` | 2026-02-17 | membership restore/revocation signer-public-key allowlist 的可选强制边界。 |
| Phase 6 | `PRD-P2P-MIG-050` | 2026-02-17 | membership policy 规范化、误配显式失败和大小写无关比较。 |
| Phase 7 | `PRD-P2P-MIG-051` | 2026-02-17 | sequencer action signer allowlist 规范化和配置 fail-fast。 |
| Phase 8 | `PRD-P2P-MIG-052` | 2026-02-17 | 共享 ed25519 规范化工具，以及 membership/sequencer/signature 一致接线。 |
| Security hardening | `PRD-P2P-MIG-053` | 2026-02-23 | guard 原子性、writer/fetch 鉴权、有界 subscription 与签名优先 restore 的稳定合同。 |
| Production-grade roadmap | `PRD-P2P-MIG-058` | 2026-02-18 | 链式哈希、签名结算与 P2PFS 路线的历史收口；“production-grade”只是一轮专题名。当前 commitment 对 canonical-CBOR `oasis7_proto::distributed::WorldBlock` 计算；字段为 `world_id/height/prev_block_hash/action_root/event_root/state_root/journal_ref/snapshot_ref/receipts_root/proposer_id/timestamp_ms/signature`，旧路线图不能另建 payload schema。 |
| Phase B | `PRD-P2P-MIG-064` | 2026-02-19 | commit 后 `NodeExecutionCommitContext/Result`、execution snapshot 字段与避免双执行的历史接线；当前执行入口是 node/chain-runtime，不是 `oasis7_viewer_live` 或 bridge fallback。 |
| Phase C | `PRD-P2P-MIG-065` | 2026-02-19 | 专用跨节点 DistFS challenge/proof networking 的历史目标。当前 gate 已通过 `ReplicationNetworkEndpoint` 探测 provider/DHT/fetch route，并区分 hard failure 与有界 degraded/fallback；但 Phase C 的 request/proof envelope、`aw.<world>.distfs.challenge.*` topic 与专用 challenge driver 未形成当前合同，仍是 future gap。 |

每一阶段的 HP 任务全部完成仅证明该阶段的实现/回归当时已收口；它不覆盖部署 inventory、节点拓扑、restore drill、生产密钥托管、治理 signer 外部化或 QA release verdict。

奖励结算只保留当前可验证的签名 mint/transport 边界，不由本专题推导生产结算或 mainnet 可用。Builtin WASM identity 继续由 `../consensus/builtin-wasm-identity-consensus.prd.md` 持有；production 工件由 world-runtime deterministic build authority 持有，历史本地 source compile fallback 不属于生产合同。

## 5. 风险、验证与运维要求

- 恢复改动至少覆盖：缺失状态、有效陈旧状态向前 reconciliation、异 world/latest 缺 binding 不写入、损坏文件 fail-closed。
- 签名策略改动至少覆盖：HMAC 兼容、ed25519 action/head 验签、membership snapshot/revocation keyring、allowlist 为空/命中/不命中、大小写规范化、重复和非法配置。
- replication guard/replay 改动至少运行 `env -u RUSTC_WRAPPER cargo test -p oasis7_distfs replication`，覆盖 epoch/sequence 单调、writer 切换、失败不污染与按序 replay。
- 运维诊断必须保留稳定的拒绝原因；不得以重启掩盖状态损坏、配置误配或签名/信任根不一致。
- 真实环境的 topology、健康基线、升级、rollback、state-sync/restore drill 仍须由相应 runbook 与 `doc/testing/evidence/` 记录提供证据。

## 6. 非目标与非 readiness

- 不新增线协议字段、密码学算法、CA/证书链、HSM/KMS、远程 signer、多签或全面身份治理。
- 不把 local/config signer、HMAC/ed25519 原语或本专题历史完成状态宣称为 production custody、mainnet-grade 或 mint readiness。
- 不替代 runtime 对恢复机制实现的权威、QA 的放行结论或 LiveOps 的对外口径。
