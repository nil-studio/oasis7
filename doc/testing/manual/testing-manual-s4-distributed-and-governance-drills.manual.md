# Testing Manual 分册：S4 分布式子系统与 S4B Governance Registry Drill

- 主测试手册：[`testing-manual.md`](../../../testing-manual.md)
- 本分册按 #6507 原样承接 `testing-manual.md` 中的 `S4：分布式子系统套件（L3）` 与 `S4B：Governance Registry Drill（L4 前置 / L4）`，不改变既有命令、验收层级或边界。

### S4：分布式子系统套件（L3）
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_node
env -u RUSTC_WRAPPER cargo test -p oasis7_distfs
env -u RUSTC_WRAPPER cargo test -p oasis7_consensus
env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib
env -u RUSTC_WRAPPER cargo test -p oasis7_net --features libp2p --lib
```
- 可选增强（涉及 runtime_bridge 改动时）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_net --features runtime_bridge --lib
```

#### 固定集合 NodeRuntime BFT 工程路径

当改动触及显式 `engineering_tendermint_v1` 路径时，在同一 source tree 串行执行：

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_proto --lib distributed_bft
env -u RUSTC_WRAPPER cargo test -p oasis7_consensus --lib tendermint_v1
env -u RUSTC_WRAPPER cargo test -p oasis7_node --lib node_bft
env -u RUSTC_WRAPPER cargo test -p oasis7_node --lib node_bft_integration_tests::node_runtime_needs_quorum_and_late_fourth_node_catches_up_then_reopens -- --exact --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib four_node_runtime_commits_and_replays_through_real_execution_drivers -- --test-threads=1
```

这些过滤器必须实际选中测试；零匹配不构成证据。覆盖固定集合上的 Alg1 proposer/lock/valid/nil/quorum 与 round 证据、任意值 unique-power phase timeout（同值 q 仍单独决定 lock/commit）、持久签名前意图和 exact-slot 重开、genesis-anchor 完整 QC 历史恢复、pending commit 与执行侧已提交的精确崩溃接续、持久高度计时基点、bounded gossip cursor 和 NodeRuntime 接线。`node_bft_integration_tests` 使用真实 loopback UDP/quorum/catch-up/restart 与明确标注的确定性 test execution hook；chain-runtime 的四节点过滤器再覆盖真实 execution driver 和运行时动作，二者不可互相代签。四节点/q=3 是集成 fixture，不是算法硬编码。BFT gossip 使用外层 `kind=bft` 与内层 BFT `kind`/`message`；容量判定针对完整 JSON envelope 的 65,507-byte 单 UDP datagram，不把协议的 16 MiB 或历史 256 MiB 上限当作 UDP 传输能力。HistoryRequest 仅为未授权 hint；`raw_bft_commit_history_page` 返回的 QC 与 `bft_status` 的 phase/lock/valid/计数/传输诊断均不能替代从固定 genesis 重验完整 QC 链；status 计数不是受保护或持久证据。以上本机测试不证明正式 authority profile、磁盘回滚抵抗、生产 signer custody、独立故障域或无分片大证书传输。

#### 动态集合 NodeRuntime BFT V2 工程路径

V2 修改须在同一 source tree 串行运行以下最小回归，并按实际影响完成相应 crate、required/full tier 与独立评审；过滤器零匹配不构成通过：

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_proto --lib distributed_bft::v2 -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_consensus --lib tendermint_v2 -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib governance_bft_parent -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib candidate_projection -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib protected_bft_v2 -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_node --lib node_bft_v2 -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib driver_bft_v2 -- --test-threads=1
```

核对 h/h+1 的旧集合证书、h+2 的新集合证书、真实签名者去重权重与严格 quorum，而不只比较状态中的 set hash。新成员安装确认须绑定当前适用转换、原 h+1 确认祖先、实际安装 head 和 journal/driver 回执；覆盖迟到安装、缺失或丢失 ACK、原请求恢复、移除及同 ID 换 key，旧 key 不再签名但合法历史可追赶。恢复正例须先停机确认原有高度，再证明此前不存在的新高度确实提交，不能把重启前已有证书当作恢复后续高。

Node 的 loopback UDP/TCP 用例验证真实 worker、remote signer service 和网络接线，其 test hook/内存 backend 不证明磁盘耐久性、实际 World 或硬件隔离。`driver_bft_v2` 另用真实 Runtime World、OS 生命周期锁与持久 execution ledger 验证 build/preview/commit/replay；冷重启用例须核对原证书前缀和已安装记录字节不变，并通过真实 Node 再提交新高度。两类用例的证明范围分别记录，不能互相代签。正式 profile、H/H+1 权威交接、checkpoint 消费者、长期可信 checkpoint 与跨故障域 custody 的整体验收仍须单独完成，不能据这些工程回归宣称 #4975 完成。


#### 公开 header V3 身份与私有载荷基础

合同见 [des-p2p-bft-header-identity-v3](../../../doc/p2p/design.md#des-p2p-bft-header-identity-v3)。运行 `env -u RUSTC_WRAPPER cargo test --locked -p oasis7_proto --lib distributed_bft::v3 -- --test-threads=1` 和 `env -u RUSTC_WRAPPER cargo test --locked -p oasis7_consensus --lib formal_bft_payload_v3 -- --test-threads=1`。核对独立身份域、公开编码不携带原始输入、精确原始字节摘要、1 MiB 边界、canonical header、版本拒绝和动作数组声明超限前置拒绝。确定性 hash 不提供隐藏性；这些测试不能证明低熵输入不可推断。

实际 World 非空动作回归另由 `driver_bft_v2_formal_header_identity_tests` 验证：已有 V2 execution preimage 绑定新 header 后，动作摘要、action root 与执行承诺任一不符必须拒绝。新类型不签名、不生产 QC、不改变 Node 或旧 wire=2 handoff；不能据此解除现有读取发布限制或宣布正式 profile ready。


#### Wire 3 公开 header QC 与独立 H 准入

合同见 [des-p2p-bft-header-certificates-v3](../../../doc/p2p/design.md#des-p2p-bft-header-certificates-v3)。执行以下 canonical 命令，过滤器必须实际选中测试：

```bash
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_proto --lib distributed_bft::v3 -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_formal_protocol --lib header_handoff -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib header_certificate_v3_tests -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib formal_read -- --test-threads=1
```

H fixture 在冻结与签名前选择并嵌入真实 target schema 2/wire 3 摘要，验证 qualified H 承诺的精确目标，拒绝旧 wire 2 target。后继 commitment 的根和集合转换是明确的协议向量：核对三高度 QC/valid-round、h/h+1 旧集与 h+2 新 key/成员、权重不足/重复/错 key/策略/轮次/phase、父 semantic root/activation/header ID、旧域签名移植，以及声明超限的 validator/vote 数组前置拒绝。它们不证明 V3 World 重执行、实际治理转换、Node canonical 提交、protected provider 安装或生产私有权限；这些接线与原始动作读取发布限制仍待后续闭环。旧 `formal_read` 回归同时核对重用 H fixture 与共享 schedule 数学未改变 V2 路径。

#### Wire 3 actual World execution preview

Trace refs: `O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46`; `O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39`. Contract: [des-p2p-bft-header-execution-v3](../../../doc/p2p/design.md#des-p2p-bft-header-execution-v3).

```bash
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib wire3_formal_read -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib formal_read -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib formal_bft_header_cli -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_runtime --lib governance_bft_parent -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_node --lib node_bft_v2 -- --test-threads=1
```

Every filter must select tests. The new `execution_tests` and `cli_execution_tests` are actual World cases: restore the independently qualified wire-3 H, execute a nonempty private action, verify separately signed header QCs and advance multiple actual heights. A self-consistent wrong execution root signed by a real quorum must pass protocol authentication and fail deterministic execution. The CLI test replays private history before preview, compares against independently driven execution, rejects mixed inputs, permissive files and symlinks, and checks no ledger or committed receipt is created. Private input canaries are checked inside the decoded payload and against encoded raw-input leakage; ASCII-only search over a CBOR integer vector is insufficient.

The earlier standalone QC/transition cases still use explicit protocol vectors; they do not claim actual governance transitions. Preview is not durable canonical publication, WorldServiceIntent execution, signer/provider installation, Node wire-3 production, latest freshness or public reading of ordinary actions. Those remaining paths must retain their existing gates.

#### Wire 3 private durable header-prefix import

Trace refs: `O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46`; `O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39`. Contract: [des-p2p-bft-header-durable-prefix-v3](../../../doc/p2p/design.md#des-p2p-bft-header-durable-prefix-v3).

```bash
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib formal_read -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib header_prefix_namespace -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib formal_bft_header_cli -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_formal_protocol --lib header_prefix -- --test-threads=1
```

Require actual selected tests. The durable cases execute a nonempty private action and three real certified World heights, then reread exact historical World ticks/semantic roots/result roots after GC with no external reader. Five independent child-process SIGKILL seams cover initialization staging, reservation staging, pins, intent and record link; recovery must preserve original bytes and exclusive format claims. A different valid QC round for the same header must authenticate as a public prefix yet fail to replace the original pending private record. Missing H receipts, missing per-height pins and corrupt Snapshots must reject both verify and import without repair. The retention callback case separately exercises real CAS pin publication followed by an initialization error, exact original intent recovery and rejection of a different H binding; it is not an extra process-kill test.

The CLI fixture dispatches both `bft-header install --config ABS --history ABS` and `bft-header verify --config ABS` using private absolute files and an independently fixed exact target. Config fields and bounds are defined in the linked design. Output is an `oasis7.formal_bft_header_local_ledger.v1` local report with a public-only prefix. It does not grant canonical Node, validator or public HTTP read authority. Physical power loss, protected signer/provider custody and WorldServiceIntent remain separate obligations.

#### Header-only fixed-cut read contract 4

Trace refs: `O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46`; `O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39`. Contract: [des-p2p-bft-header-read-v4](../../../doc/p2p/design.md#des-p2p-bft-header-read-v4). Read version 4 and consensus wire 3 identify distinct contracts.

```bash
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_client_api --lib formal_bft_header -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_game_client --lib formal_bft_read_v4 -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib wire3_formal_read_v4 -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib formal_read -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib formal_bft_prefix_http -- --test-threads=1
```

The actual shipping case installs a real nonempty-action three-height private ledger, deletes a known unretained GC canary, cold-starts `bft-header serve --config ABS`, and reads `/v4/world/read` with the shipping client. Check the exact tip and earlier World tick, full canonical header-prefix roundtrip, projection proof and private-input byte exclusion. Tampered nonce/H/target/head roots or commitment hash, trimmed/reordered prefixes and mismatched projection audience must fail. Previous read paths and signing domains remain rejected, while read-contract-3 nonempty-action rejection remains covered by the full regression suite. A self-consistent execution lie authenticated by real quorum signatures must pass protocol verification yet fail CLI installation and serving before a listener starts.

The independent current-authorization fixture verifies delegated read success, busy/unavailable 503, journalled revocation 403, owner success and a lock held across reply construction/signing/socket release. The CLI supplies no current authority and returns 503 for private scopes. Reuse the existing bounded HTTP writer tests for responses above the old 8 MiB limit, absolute write deadlines and oversized-body rejection before any header. These are local historical-read and authorization-boundary results; they do not prove current Node readiness, checkpoint freshness, protected provider installation or physical fault domains.

#### Wire-3 singleton Node and protected-provider integration

Trace refs: `O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46`; `O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39`. Contract: [des-p2p-bft-header-node-v3](../../../doc/p2p/design.md#des-p2p-bft-header-node-v3).

```bash
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_consensus --lib tendermint_v3 -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_distfs --lib protected_bft_v3 -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_node --lib bft_v3 -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib formal_read_node_private -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib node_execution_tests -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib formal_read -- --test-threads=1
```

The Node case must submit nonempty RuntimeAction payloads through NodeRuntime, obtain native TCP provider signatures, and check the committed batch against the actual installed World, exact canonical ledger record and provider applied receipt. Recreate Node, driver, service and remote-client objects, prune a known unretained CAS canary, then require a second successful nonempty action and read-contract-4 client consumption of the resulting history. The backend in this case is explicitly a test-only in-memory double; object restart does not establish physical backend durability or production key custody.

Native retry checks retain the original completed proposal and vote signatures across height advancement and cold service reopen; changing an old intent must fail without changing state. Timer checks must reject attempts to set or erase the height origin through generic Advance, while the protected StartHeightTimer path remains usable. Private-store checks remove original pins after intent/link publication and remove completed metadata while CAS/pins remain; both must fail without recreating original resources.

The one-shot consumer is `bft-header node --config ABS --actions ABS`. Its separate schema-1 configuration requires qualified-H trust/material, schema-2 target, disjoint existing private CAS/records/replication directories, validator/player/operator identities, provisioned public signing-key identities, timeouts, explicit provider endpoint and pinned V3 trust, and an existing requester authentication key. The actions file is canonical private action CBOR for that player. Public output contains only applied height/header/action-root/count receipts. This command admits one validator without transitions or peer gossip and supplies no software substitute for a production protected backend. Reusing an already completed input is a new submission; an original pending reservation is resumed before newly submitted work. A public receipt is not a global idempotency key.

For first use, run `bft-header node --config ABS --print-binding` without `--actions`. It initializes the original local origin and emits `oasis7.formal_bft_header_node_binding.v3` with a public binding and digest; no provider listener, acquisition or signing is needed. The provider must independently verify qualified H before trusted provisioning; the printed descriptor cannot grant authority. Then add explicit `provider_trust` and run the normal actions command. Repeating print mode must preserve the original digest. Normal submission without provider trust must fail before origin initialization.

长期离线配置准备使用独立入口 `bft-header prepare-checkpoint-trust --config ABS --node-config ABS --policy-pin HEX32 --consumer-binding HEX32 --original-c0-pin HEX32`，对应 [checkpoint renewal 合同](../../../doc/p2p/design.md#des-p2p-bft-checkpoint-renewal-v1)。新配置为 self-described canonical CBOR，字段为 `schema_version`、`formal_read_trust`、`target`、`consumer_binding_digest`、`initial_anchor`、`policy`、`policy_digest` 及合同四个 `initial_*_unix_ms`；只支持 `qualified_handoff_v2`，不接受 successor tag。旧 strict JSON trust loader 不变，没有格式猜测或 fallback。外层 2 MiB、嵌套 trust/policy 各 64 KiB（包括其独立 canonical 自描述 tag），typed 解码前扫描长度及 issuer 数量；完整重编码相等检查仍必需。

三个外部 pin 必须取自绑定同一 world/H/policy 的原独立部署授权或受保护保留记录，不能从当前候选提取作为“独立”参数。C0 checkpoint pin 不等于 H checkpoint digest 或 initial activation digest。SDK 返回的 preparation 仅表示配置一致性；host 进一步复用原 Node preparation，恢复实际 H 闭包并对照原 binding/material/schedule。该命令和 `--print-binding` 一样可能初始化或恢复私有 Node ledger/namespace，即使后续 source 对照失败也不能声称零文件副作用。它不启动 provider、不提交更新、不签写或服务；输出固定 `current_ready=false`、`renewal_ready=false`。C0→H 完整 lineage、当前 status、可信 protected time 与安装恢复尚非本入口的验收结果。

本切片检查：`env -u RUSTC_WRAPPER cargo test --locked -p oasis7_formal_protocol --lib checkpoint_trust -- --test-threads=1`、`env -u RUSTC_WRAPPER cargo test --locked -p oasis7_game_client --lib checkpoint_trust -- --test-threads=1`，并运行本节真实 shipping Node CLI 用例 `wire3_formal_read_shipping_node_cli_commits_private_input_via_native_provider`。后者在启动 provider 前消费完整新配置，核对外部 policy/binding/C0 错配、改 source 后重算摘要、过期上界、版本、旧入口拒新配置、实际 schedule 错配和修正后重读；不能用这些配置准备断言代签当前用途或 #5525 的完成。

| Node config fields | Contract |
| --- | --- |
| `schema_version` | 1 |
| `trust_config_path`, `target`, `handoff_material_path` | Private absolute trust/H files and the qualified schema-2 wire-3 target |
| `cas_directory`, `records_directory`, `replication_directory` | Existing, owned private absolute directories, mutually disjoint |
| `validator_id`, `player_id`, `operator_instance_id` | Explicit local validator, action submitter and operator identities |
| `provisioned_signing_keys` | Public `{validator_id, public_key_hex}` identities matching admitted signing keys |
| `requester_signing_key_path` | Existing private authentication key; never a local consensus signing key |
| `timeouts` | Positive `propose_ms`, `prevote_ms`, `precommit_ms`, `round_delta_ms` |
| `provider_address`, `provider_timeout_ms`, `completion_timeout_ms` | Explicit socket endpoint; 1–60000 ms RPC and 1–600000 ms completion bounds |
| `provider_trust` | Required for actions: schema 3, `provider_public_key`, exact `binding_digest`; may be omitted only for print mode |

The shipping CLI regression first prints this binding without a provider, then preserves an original private candidate before any provider WAL and submits a different new action. It requires the original candidate to commit first with unchanged bytes and the new action to finish afterward. The publication-failure case injects an error after canonical pins are durable, checks that no applied batch or new World is exposed, and cold-opens with an empty external reader to complete the same QC before a second write. A separate real Node/native-TCP/provider regression uses an explicit deterministic test execution hook: 1025 prequeued action IDs must produce independently decoded protocol-valid batches across multiple heights without loss or duplicate delivery. It exercises production queue selection and consumption, not 1025 actual World executions; the actual World, CLI and durable recovery cases above cover that execution boundary. Other regressions test exact CBOR framing and one-element admission boundaries, and require the original completion after both same-Runtime restart and a provider confirmation that commits before returning an unknown result. Fully applied history with no matching pending operation must not be replayed; notification-budget or lock failures must retain the original queue and eligibility. The uncertain-CAS test explicitly fences the old lease generation in its memory backend after the failed provider session ends, without changing committed protocol state; automatic production backend recovery is not supplied. These are local software paths with object-level failure injection, not physical power-loss evidence.

#### 原 covered H 与首个 Local/Received 的独立归档验证

本入口承接 [wire-3 原私有候选保留合同](../../../doc/p2p/design.md#des-p2p-bft-header-node-v3)，使用显式准备、原 Fresh activation 和首个 Pending Reserve，不启用普通 Node 的 covered-source 签名。`bft-header node --config ABS --source-config ABS` 的源配置为 schema 1，要求独立固定的 `creation_identity` 与 `attestor`；准备模式 `--prepare-private-source` 可先输出供独立 provision 审查的 binding/marker，`--activate-private-source` 再通过已配置的原生 provider 保留原 activation。后续 `archive_directory` 必须是与 live CAS、records、replication 分离的已有私有目录。

原 Local 使用 `--archive-first-private-source-input --actions ABS`；原 Received 使用 `--archive-first-received-private-source-input --package ABS`。Received 文件沿用 Node native private exchange 的 canonical package 编码和 66 MiB 上限。codec 不授予远程读取权限，已有文件也不证明来自获准 peer；实际 Runtime 必须从独立 qualified H 重现 header、execution commitment 和候选实际访问的精确依赖闭包后，才生成 schema-10 原件、原 publication metadata 与一次首价。此路径不能替代远端 private acquisition 的独立授权。归档报告包含 `source`、`candidate`、`original_source`、实际 roots 和原 reservation bytes。完整原源归档固定 H material、ledger Origin、schema-8 private Origin、原 activation capsule，以及最多 4096 节点／64 MiB 的完整 H 冷索引；未被候选访问的 H 分支由原源归档保全，不能填入 Received package 改变原闭包。本地目录 inode 只保留在原 Data 身份中，不授权独立归档目录。

把报告中的 `source`、`candidate` 作为 `reserve_input`，把 `original_source` 作为源配置中的同名 pin。`--verify-first-private-source-archive` 只打开独立 trust、requester key、provider trust/attestor 与归档；它不要求 live H 文件、CAS、records 或 replication 存在，也不创建或探测它们。只读验证按可信配置对旧路径做绝对路径和词法重叠检查，归档与 Reserve 的 live-source 模式仍解析实际目录，拒绝可解析别名绕过物理隔离。requester key 文件仍须可用，以重建 binding 中的公开身份；core reader 不读取或保留私钥，本入口不声称所有旧配置和密钥都可丢失。它从原件重建 H、实际执行候选并校验 metadata、全部依赖、private-origin 身份与原首价。manifest、H、activation 或依赖缺失／损坏必须失败，不能使用 live reader 或新生成的材料补齐。

`--reserve-first-private-source-input` 复用同一 `reserve_input` 和原 protected source。provider 的 `BftFirstReserveSourceV1` 可接入 `ArchivedOriginalSourceReaderV1`：独立配置的精确 reference 仅收窄已持有原件的本地读取范围，不提供远端 OriginalAdmissionInput 的 issuer/grant。单个 encoded/decoded input hold 必须持续至原 protected CAS/readback。Local 使用原 height WAL，Received 使用 height/header WAL，原 schema-10 首价同时保留未来 selection 的 footprint；这不表示 selection 已发生。provider 成功而本地 WAL 未完成时，重试须读取原 Pending receipt，保留原 Data 字节、首价、revision 与原目录身份，不重新读取或改写成另一候选。

```bash
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_chain_runtime --lib covered_source_tests -- --test-threads=1
env -u RUSTC_WRAPPER cargo test --locked -p oasis7_distfs --lib protected_bft_v3 -- --test-threads=1
```

`covered_source_tests` 必须实际运行 Local 与 Received 两项。两项均在测试进程内经 `crate::identity_receipt::dispatch` 调用真实 CLI 路由，使用实际 Runtime、新建的独立 reader 对象及原生 provider TCP service；未启动外层 shipping 二进制或独立 provider 子进程。Received 的输入来自实际 Node retain/export。验收比较原执行与独立冷读的 semantic/result roots，分别移除和损坏 manifest、H、ledger/private Origin、capsule、冷依赖与 admission input，拒绝无权 reference、错误 attestor、过期 deadline、提前释放／并发占用 transport hold、改变首价以及不匹配的 covered checkpoint。冷 CLI 用不存在的 live 路径和实际无法解析的 `/dev/null/old-*` 路径仍须成功；后者明确覆盖 ENOTDIR，不冒充 PermissionDenied 实测。原 Reserve→WAL 失败后，关闭 reader 的再次恢复仍须完成同一原字节。普通 covered-source Node 签名和复制目录冒充原 namespace 继续拒绝。

这些结果只覆盖原 H 到首个 Local/Received Pending 的软件消费链。完整 alternatives、Ready、唯一 Selection、Seal、跨段 carry 和 active successor 的 actual Runtime 消费仍是 #5516 后续义务；本节不把归档 helper、首价中预留 selection 字节或本地软件 backend 计为这些状态完成。测试 backend 是明确的软件事务替身，进程内对象重开与故障注入不证明 #5518 真实保护载体、不可导出密钥、反回滚或物理故障域验收。

#### 本地 CAS retention 与 GC 协调

合同见 [des-dh-local-retention-coordination](../../../doc/p2p/distfs/distfs-production-hardening.design.md#des-dh-local-retention-coordination)。执行 `env -u RUSTC_WRAPPER cargo test -p oasis7_cas --lib retention_tests` 和 `env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib formal_full_sync::cas_lifecycle::tests`。覆盖独立进程等待同一 OS 锁、pin 发布后 quota/orphan/delete 的原字节保留、并发 global pin 无丢失、杀进程后在原载体上重获锁、GC 先行时 pin 明确 missing、显式 unpin 后允许回收和非法载体拒绝。子进程等待测试使用启动标记和有界观察窗口，须先确认删去锁时同一回归判红，不将时间等待本身当作进入临界区的证明。

同步验证 `oasis7_node` 的 `replication::tests`、Runtime sidecar 回收以及正式 writer 的 `published_attempt_reclamation_retries_independent_readback_and_partial_unlink`：损坏修复不删已正确且 pinned 的对象，引用中的 attempt 输出不进入 Reclaimed，释放 pin 后原操作继续回收。仅本地合作进程/文件系统证据，不证明完整历史 GC 资格、远端故障域或掉电耐久性。

#### 正式 BFT 固定 H+1 读取交接

合同见 [des-p2p-formal-bft-read-handoff-v1](../../../doc/p2p/design.md#des-p2p-formal-bft-read-handoff-v1)。本切片只验证由外部固定 H 与 H+1 block ID 选择的历史读取，不把工程 V2 历史或服务自报 head 升级为正式信任。

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_formal_protocol --lib bft:: -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_world_service --lib formal_bft -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib formal_read::tests -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib formal_dependencies::tests -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib formal_storage::tests -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_game_client --lib bft_transport_tests -- --test-threads=1
./scripts/ci-tests.sh required --group cargo_tooling_contracts --repo-root "$PWD"
```

过滤器必须实际选中测试。纯 BFT 证明由 `oasis7_formal_protocol::bft` 提供，consensus facade 与宿主继续共用同一类型；保留既有 CBOR 字节（包括 self-describe tag）和摘要向量，普通客户端依赖闭包不包含 CAS/World/Node/执行后端。验收核对 qualified H 的完整 Snapshot/Journal/执行材料与结果索引、实际 World tick 等于 H position、H+1 隔离执行与 QC 绑定的原始 Snapshot/semantic/projection/result roots、持久提交后重开回读，以及新读取路径到实际客户端的证明校验。负例覆盖错误 H 目标、父引用、manifest、根与投影，工程历史冒充正式历史，交接前 World 不符且不得重写 tick，旧 controlled 权威在 H 后追加，以及当前授权撤销后的私有读取拒绝。

固定 cut 的读取证据不证明最新状态、自动追随、正式写入接线、完整 checkpoint/full-sync 消费者或真实 signer custody。缺少独立当前授权 World 时私有读取必须拒绝，不能使用历史 World 代替。#4975 的其余实施与验收义务保持独立。

#### 正式 BFT 连续前缀与后继候选

沿用上述正式交接合同，额外运行：

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_formal_protocol --lib prefix_tests -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib governance_bft_parent -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib formal_prefix -- --nocapture
```

协议前缀逐条核对从 qualified H 接续的 QC 与正式 execution preimage，覆盖缺高、重排、重复、有效 QC 搭配错误 parent semantic root、64 高度与 16 MiB 累计边界，以及集合 h/h+1 使用旧 key、h+2 才切新 key。实际 World 用例从真实 H+1 持久 cut 派生并执行隔离 H+2，保留 H 的冷历史 reader，拒绝父状态漂移、未提交队列和用旧 H restore 认证后继 Snapshot。父策略替换用例中的重新签名 World 只是协议/Runtime 策略 fixture；须同时观察实际 H+1 driver 拒绝它，不能把该 fixture 当作独立重执行或生产提交证据。

完整前缀重放用例从同一 qualified H 顺序执行三个实际高度，核对 tip Snapshot、Journal、全部 commitment 和结果树，且 H World 与旧 ledger 均不变。外部精确目标高度/ID 不符必须在执行前拒绝；合法 QC 签署伪造结果树根时，先确认协议验证通过，再确认完整重放拒绝。该用例不使用 provider 提供的后继 Snapshot 恢复世界。额外让返回的 replay reader 读取 source 中未种入旧 recorder 的 content-addressed probe，确认 fallback 后旧 recorder refs 完全不变；probe 只检验缓存隔离，不作为业务索引的语义证明。

这些候选执行证据不增加固定 H+1 ledger 或 `/v2/world/read` 的范围，也不证明多高度持久前缀、后继读取版本、正式 signer/provider/Node 写入接线或部署 readiness。候选 World 不得被包装为未持久化的 committed cut。

#### 正式 BFT 多高度持久 prefix 导入与恢复

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_cas --lib pin_store::tests -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib formal_prefix_ledger -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib formal_read::tests -- --test-threads=1
```

用例从 qualified H 构造三个实际 World 高度，通过独立 prefix ledger 及真实 `bft-prefix install/verify` dispatcher 安装，再用空外部索引 reader 在 GC 后重开；对照 Snapshot、结果树、原 durable digest 与较早的显式历史 cut。旧 H+1 目录不是初始化前置，旧解码器必须拒绝降级 fence，同目录其他格式被共同 OS carrier 排除。

负例须覆盖有效 QC 中伪造结果根、缺 pin 的重复重开、丢失/损坏 CAS、部分记录、符号链接、发布态额外硬链接、丢失 origin/marker/稳定锁，且不自动修复或覆盖原件。真实子进程覆盖 init-intent 与 reservation 完整写入后、rename 前的 staging，CAS/pins 发布后，以及 intent 发布后和 record 硬链接后五个耐久点，由父进程对其持有的 child handle 发送 SIGKILL，再核对原 staging/reservation 字节、单/双链接状态、跨格式排他和同一 lock inode 的恢复。不同签名 round 的同块 QC 也不能替代既有 reservation、staging 或 intent 的原证书；不同合格 H 不能替换初始化 staging 的身份。空账本完成初始化后丢失 origin 或任一 marker，fresh install 也必须拒绝且不修复。较早 cut 的 reader 必须拒绝仅未来高度保留的节点。只有真实覆盖的进程死亡、文件系统和固定 cut 可记为通过；这不是磁盘掉电、硬件 signer 或独立物理故障域证明。

连续前缀读取使用显式 V3 域，执行：

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_client_api --lib prefix_read_v3
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib formal_prefix_v3_shipping -- --test-threads=1
```

该用例通过真实 `bft-prefix install` 安装三高度原前缀、GC 后以 `bft-prefix serve` 重开，并由实际 HTTP V3 客户端验证完整前缀、精确高度、public inclusion 和 World tick；再重开显式较早 cut。负例包括错误目标、缺高中间前缀、伪造 view/root、旧 V2 路由与签名域、没有当前授权源的 private scope，以及缺 pin 后反复 serve 不补造凭据。现有 V2 block ID 认证原始动作，因此当前 V2/V3 light-read publication 只支持每高精确规范空动作；实际私有 ledger 接受并重放带动作块后，两个 publication 与 V3 verifier 仍须拒绝泄露这些输入。V3 DTO 另核对 H 后 64 高度范围和 40 MiB 编码计数提前停止；16 MiB CBOR prefix 的 hex 及 JSON 传输各有明确预算。HTTP 组件另以真实 TCP 传输 9 MiB JSON body、检查超限零写入以及不读数据的客户端在绝对截止处失败。`v3_private_read_holds_and_rechecks_the_current_delegation` 用真实 Agent/owner/delegation 与独立当前 World 检查私有正向读取、实际撤权后 403、授权锁忙 503 及写入前暂停时 guard 不释放。CLI 默认没有独立当前授权源，因此 private 仍阻断；本例不证明当前 private provider/正式 Node 接线、fresh/latest、硬件 signer 或物理故障域 readiness。


CLI 配置/角色边界见 [P2P 正式交接](../../../doc/p2p/design.md#des-p2p-formal-bft-read-handoff-v1)。多高度本机安装 receipt 不代表新读取服务、当前私有授权、自动追随或正式 validator 写入已接线。

#### 本机身份发布回归（NIR bootstrap）

针对普通启动 ensure 与专用身份 helper 的本机持久化修改，运行：

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib --no-default-features --features node-libp2p node_keypair_config -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib --no-default-features --features node-libp2p local_node_init_retains_identity_and_never_creates_world_or_authority -- --nocapture
```

`node_keypair_config::transaction::tests` 使用当前 test executable 的独立子进程，在锁持有、意图、候选文件、准备、发布与完成边界 SIGKILL；逐次重开核对同一公钥、锁 inode、原配置及候选／完成状态。另覆盖发布后注入错误、损坏候选／目标冲突拒绝、private-only 补公钥、完整旧身份迁移与原件丢失阻断、旧锁离线迁移边界。`process_worker` 是该套件的子进程入口，独立执行不构成场景覆盖。临时环境变量只在 `cfg(test)` 中生效，不提供生产故障开关，不输出私钥。

迁移消费者还须执行 `data_directory_migration_preserves_identity_and_retries_only_from_receipt` 与 `legacy_migration` 过滤组：核对 stopped 原事务、迁移后同一公钥／PeerId、完整意图路径重绑定、独立及嵌入 config、删除目标 key 后仍拒绝重生成。legacy fixture 的 `current_exe` 摘要入口拒绝 symlink；若显式配置的 target 符号链接被用于启动 test executable，记录该入口限制，并在同一已构建 artifact 的物理路径执行该过滤组，不能修改 target/cache 配置或把失败改成通过。

正式 v2 运行期身份租约还须验证：

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib --no-default-features --features node-libp2p node_keypair_config::runtime_lease -- --nocapture
env -u RUSTC_WRAPPER cargo build -p oasis7_distfs --bin oasis7_durability_endpoint
OASIS7_FORMAL_ENDPOINT_TEST_BIN=<absolute physical path to oasis7_durability_endpoint> env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib protected_endpoint_children_runtime_identity_lease_excludes_duplicates_and_survives_outer_drop -- --ignored --nocapture
OASIS7_FORMAL_ENDPOINT_TEST_BIN=<absolute physical path to oasis7_durability_endpoint> env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib protected_endpoint_children_signer_handoff_stops_exact_owner_and_qualifies_successor -- --ignored --nocapture
```

局部租约组核对锁 inode、SIGKILL、身份域与目录归属两个不可覆盖绑定的部分发布／同步失败恢复、错域／复制旧目录记录／不同 key 接管、损坏／超大记录和私钥诊断脱敏。真实正式运行时场景使用已签名初始锚与实际 P/R 子进程，验证同身份不同 writer 目录的重复启动被拒绝；外层运行时退出而实际 Node 仍被持有时租约不释放，全部执行所有者停止后原身份可重新打开，也可登记尚无身份归属记录的新 writer 目录。另执行已有合法 signer handoff：同 Node 身份切换新 writer 目录时，仍须通过签名恢复权限、精确旧 owner 停止和受保护 epoch 检查，不能让身份域绑定误拒绝合法连续性，也不能由目录登记授予权限。已有目录的路径／身份冲突仍拒绝。正常 CI 的 ignored 不算该进程场景通过，须记录明确非零执行与退出码。该场景证明本地实际调用链，不证明跨机 bare-key 排重、真实物理故障域或断电持久性；legacy／DevLocal 不由此获得新权限或运行保证。

这些测试证明实际覆盖的进程恢复与调用路径，不证明断电、整机旧备份回滚或全初始化绑定事务。目标文件系统断电／fsync 错误、完整 CLI enabled/disabled/read-only 组合及其未覆盖项仍按[身份设计](../../../doc/p2p/node/node-identity-replication-contract.design.md#des-nir-bootstrap)记录；不得用单元通过代签。

<a name="s4b：governance-registry-drill（l4-前置-l4）"></a>

### S4B：Governance Registry Drill（L4 前置 / L4）
- 适用场景：
  - 治理 signer rotation / revocation / failover runbook 首轮验证
  - `MAINNET-2` / `BENCH-G1` 需要留下 `pass + block` 审计证据时
- 推荐入口：
```bash
./scripts/governance-registry-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id msig.foundation_ops.v1 \
  --replace-signer-id signer03 \
  --replacement-public-key <replacement_public_key_hex> \
  --out-dir output/governance-drills/<run_id>
```
- finality slot 示例：
```bash
./scripts/governance-registry-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id governance.finality.v1 \
  --replace-signer-id signer03 \
  --replacement-signer-id signer04 \
  --replacement-public-key <replacement_public_key_hex> \
  --out-dir output/governance-drills/<run_id>
```
- 统一持久大世界的 committed execution-state 正式证据入口：
```bash
./scripts/governance-registry-live-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id governance.finality.v1 \
  --replace-signer-id signer02 \
  --replacement-signer-id signer05 \
  --replacement-public-key <replacement_public_key_hex> \
  --out-dir output/governance-drills/<run_id>
```
- finality multi-signer loss / rejoin 示例：
```bash
./scripts/governance-registry-live-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id governance.finality.v1 \
  --replace-signer-id signer02 \
  --replacement-signer-id signer05 \
  --block-remove-signer-id signer01 \
  --block-remove-signer-id signer02 \
  --replacement-public-key <replacement_public_key_hex> \
  --out-dir output/governance-drills/<run_id>
```
- finality non-baseline rejoin 示例：
```bash
./scripts/governance-registry-live-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id governance.finality.v1 \
  --replace-signer-id signer02 \
  --replacement-signer-id signer05 \
  --replacement-public-key <replacement_public_key_hex> \
  --out-dir output/governance-drills/<run_id>
```
- finality baseline rejoin 示例：
```bash
./scripts/governance-registry-live-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id governance.finality.v1 \
  --pass-manifest-mode baseline \
  --replace-signer-id signer02 \
  --out-dir output/governance-drills/<run_id>
```
- 产物约定：
  - `run_config.json`
  - `summary.json`
  - `summary.md`
  - `manifests/{rotated_pass_manifest.json,degraded_block_manifest.json}`
  - `logs/*`
  - live-world 额外包含 `world-backup-pre-drill/*`
- 判定口径：
  - baseline / pass case 应返回 `overall_status=ready_for_ops_drill`
  - negative block case 可能有两种合法阻断结果：
    - `audit_failover_gate`: `block_import_rc=0` 且 `overall_status=failover_blocked`
    - `import_policy_reject`: `block_import_rc!=0` 且后续对 block manifest 的审计表现为 `manifest_mismatch`
  - 若 `block_enforcement_stage=audit_failover_gate`，脚本还会继续产出 `rejoin_case`；其期望结果是 `overall_status=ready_for_ops_drill`
  - `pass_manifest_mode=baseline` 适用于 temporary offline / same-signer rejoin；`pass_manifest_mode=rotate` 适用于 replacement / revocation 恢复
  - clone-world 样本只证明 runbook/tooling 正确，不替代统一持久大世界 committed execution-state 的最终 QA 证据
  - `governance-registry-live-drill.sh` 会在真实默认 execution-state 上自动执行 `baseline -> pass -> block -> restore`
  - 当 block case 仍可导入时，`governance-registry-drill.sh` / `governance-registry-live-drill.sh` 会额外执行 `rejoin`
  - controller slot 可保持原 `signer_id` 仅替换公钥；`governance.finality.v1` 不行，必须显式传入新的 `--replacement-signer-id`
  - `--block-remove-signer-id` 可重复使用；当 block manifest 让 `finality signer_count < threshold` 时，默认预期是 `import_policy_reject`
  - 若对 finality slot 复用原 `signer_id`，真实导入会命中 `GovernancePolicyInvalid`，因为 finality signer 绑定到现有 node identity
