# Node identity and replication contract

## Node 专业接受与设计分配

Owner role：runtime_engineer；canonical repository：eng-cc/oasis7；审读 source baseline：f9d5a552d9af04c1b1398262808198a58e560230（2026-09-26）。本文的 current 指冻结源所记专业合同；本次未独立验证实现、部署或测试通过。target 是规范目标，historical 是 MIG/CCG/TASK 与 dated evidence provenance。GitHub Issue/Project 维护实际任务与候选证据。新增稳定条款是原义务的细化入口；原章节/常量/命令/失败边界仍有效。

<a id="nir-bootstrap"></a>
### Local identity

local/dev missing key 仅在启用节点启动前于当前 config path 创建；disabled 不创建；missing/malformed/conflict/unwritable 明确失败，不替换已有非法 identity。不是 custody/admission。

Trace ID: O7-REQ-b39ac7bb-609b-4784-9a94-7ef48ad4280e

同一初始化意图的并发执行、进程中止和重试必须至多发布一份有效节点身份，已发布或已绑定的公钥／PeerId 不能因重试改变。除现有 local/dev missing-key ensure 外，显式 `node init` 的本机身份准备也适用该持久性要求；formal runtime 的启动不能借此自动生成 authority、validator 或 governance signer。运营者只有在身份与初始化绑定已持久可恢复后才能得到成功结果。

进程异常退出遗留的锁标记不能永久阻塞合法重试，重试也不能仅凭等待超时夺取存活持有者的锁。写入中断不得把半份密钥发布为有效配置；恢复必须区分未发布临时材料、已发布但未完成绑定、完整身份以及损坏／冲突。完整材料重用原身份，损坏或归属不明保持明确失败，不以重新生成身份代替恢复。私钥、seed 和临时密钥不得出现在诊断或验证产物中。

系统设计依据：[原子初始化与恢复](node-identity-replication-contract.design.md#des-nir-bootstrap)。该 crash-safe 目标当前未实现完整闭环：基线 `b42a9e3fb8c8cd60679936c5f46c2fde6f62cf1e` 的 [ConfigFileLock 与写入 helper](../../../crates/oasis7_chain_runtime/src/host/node_keypair_config.rs) 使用 create-new 锁文件和直接配置写入；异常退出不经过 Drop，残留锁与部分写入不能由正常并发测试证明可恢复。此条为本机初始化专业要求，产品 AC 不适用；它不新增世界恢复、生产 custody 或 validator admission 承诺。

验收要求在临时写入、原子发布、绑定和目录同步各边界中断／重启，并覆盖并发及持锁进程 SIGKILL：有效重试保持原公钥／PeerId 或在尚未发布时完成唯一发布；失败没有新绑定或隐式身份替换。现有权限、symlink 和正常并发测试只证明各自子范围。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="nir-signer-binding"></a>
### Conditional signer map

配置时完整覆盖 validator set、无 unknown、normalized32-byte ed25519；proposal/attestation/commit 先正常验签再 binding，缺失或 mismatch 拒绝。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3。局部承接 [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)：启用对应配置/签名模式后，即使签名有效，missing/mismatch binding 也不能让任意 signer 冒充已受治理 validator。未配置 map 的部署不能据此声称完整绑定。治理 registry、轮换、activation、所有网络角色与被攻破服务组合不由本条证明。

<a id="nir-ingest-order"></a>
### Replication ingest

world/topic-isolated injection；验 signature/source→apply→persist guard→observe progress。stale/duplicate/invalid/失败不推进 peer/committed observation 或污染 guard；local/remote single-writer ordering 与持久 guard 保留。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="nir-recovery"></a>
### Authoritative recovery

corrupt/unreadable PoS state blocks start，不默认重置；保留诊断并纠正 state/config/deployment 根因。无自动 deploy/state-sync/restore/rollback 或 topology/readiness 承诺。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4。局部承接 [REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)：已定义的 PoS authoritative state corrupt/unreadable 必须阻断启动，不 default reset 或伪恢复，保留诊断并纠正根因。仅此负向边界；不证明合法激活链、checkpoint/snapshot/replay/root 全链、非空日志后缀、新请求/模块行动与业务结算正向续跑或当前可服务。


- Design: `doc/p2p/node/node-identity-replication-contract.design.md`
- Project record: GitHub Issue / GitHub Project

## 目标与权威边界

This is the current `node/` authority for local node-identity bootstrap,
validator-to-signer binding, and signed replication ingestion. It absorbs the
completed `PRD-P2P-MIG-088`, `PRD-P2P-MIG-092`, `PRD-P2P-MIG-095`, and
`PRD-P2P-MIG-099` triplets. Historical implementation detail remains traceable
through Git history and GitHub task evidence, not through a second live
authority.

The contract covers the node-runtime configuration and replication boundary. It
does not define consensus or governance admission, reward settlement, production
custody, public-network deployment, topology health, or QA release readiness.

## 范围

This scope is limited to the node-side identity and replication contracts
listed below. It does not widen the authority to consensus, governance,
settlement, deployment, or release decisions.

## Current contract

### 接口 / 数据：identity and signer binding

- A local development node may ensure a missing node keypair in its current
  config path before it starts. Disabled-node flows do not bootstrap identity.
- Missing, malformed, conflicting, or unwritable configuration fails with an
  explicit diagnostic. It must not silently replace an existing invalid identity.
- Crash-safe publication and retry follow [Local identity](#nir-bootstrap).
  This is a target contract; current create-new lock files and direct writes do
  not establish crash recovery. A completed local identity remains separate from
  network membership, governance authorization, and protected signer custody.
- When `validator_signer_public_keys` is configured, it binds every configured
  validator ID to one normalized 32-byte ed25519 public key. The map must cover
  the validator set, contain no unknown validator, and reject invalid keys.
- For proposal, attestation, and commit messages, normal signature verification
  precedes the enabled validator-to-signer binding check. A missing or mismatched
  public key is rejected.

The local config identity is a development/bootstrap convenience only. It is not
a production keystore, KMS/HSM custody mechanism, rotation or revocation
workflow, validator admission, governance signer, mainnet claim, or readiness
verdict. Private keys, seeds, mnemonics, and complete secret environments must
not enter documentation, inventory, monitor artifacts, or evidence.

Transport and consensus signing purposes must remain distinct. Deriving them
under separate domains from one local root does not create independent custody:
compromise of that root affects both derived identities. Governance signers and
formal authority are accepted only through their own trusted policy, never from
successful local initialization or a matching PeerId.

### 接口 / 数据：replication and recovery

- `NodeRuntime` can receive a `NodeReplicationNetworkHandle`; its replication
  path supports network injection and world/topic isolation.
- Signed `FileReplicationRecord` ingestion validates the applicable identity and
  source boundary, applies/persists successfully, and only then advances
  replication observation such as peer heads or committed progress. Errors remain
  observable; a failed apply must not be represented as a progressed peer.
- Local and remote single-writer guards, record ordering, and persisted guard
  state protect against stale or duplicate remote application. Invalid records
  do not enter local state.
- Corrupt or unreadable PoS recovery state blocks node startup rather than
  selecting a default state. A restart is diagnostic or temporary recovery, not
  a substitute for correcting the state, configuration, or deployment root cause.

## Transport and operational boundary

The runtime injection abstraction is not evidence that a libp2p deployment,
NAT traversal, public reachability, peer inventory, or mainnet-grade network
health exists. Transport labels do not establish topology truth: health and role
must be read from the current runtime status/evidence window. Current real-triad
sampling and bounded operator evidence remain under
`node-triad-operations-observability.*`.

The historical UDP fallback and `aw.<world_id>.replication` topic spelling are
not current transport authority. They may be inspected in history only unless a
future code-backed contract explicitly restores them. This document does not add
PKI distribution, multi-writer CRDT, DHT/Kad indexing, automatic deployment,
state sync, restore, rollback, or release automation.

## 里程碑

- M1: Four completed historical triplets are absorbed into this stable node
  contract without carrying their obsolete transport wording forward.
- M2: Shared routes and the file index are repaired by the integration owner,
  then the retired basenames pass the frozen-head stale-reference gate.
- M3: Runtime and QA owners retain targeted verification of signer binding,
  ingest ordering, recovery, and duplicate/stale replication handling.

## 风险

- A local bootstrap key can be mistaken for a custody or governance signer; the
  non-custody boundary must remain explicit.
- Transport labels, UDP fallback, or historical `aw.*` topics can be mistaken
  for a current deployed topology; they are not current authority.
- Restarting after corruption can hide the real config/state/deployment defect;
  startup failure must remain observable and fail closed.

## Verification and failure posture

Targeted node verification must cover signer-map rejection, missing/mismatched
signer rejection, apply-before-observe ingest behavior, corrupt-state startup
failure, duplicate/stale record rejection, and persisted replication recovery.
The specific command set is selected by the runtime and QA owners; documentation
migration itself does not claim runtime, integration, or release success.

The bootstrap target additionally requires crash injection across lock ownership,
private temporary writes, atomic publication, directory sync and binding recovery.
Test output compares public identity and file-state outcomes without exposing key
material; a normal concurrency pass does not prove kill/power-loss recovery.

For this authority migration, run:

```sh
./scripts/doc-governance-check.sh
./scripts/readme-link-check.sh
git diff --check
```

## Explicit non-claims

- A valid local keypair or signer map is not custody, governance truth, or a
  production/mainnet readiness signal.
- Signed replication is not a public-chain, topology-health, settlement, or QA
  release verdict.
- This document does not supersede runtime consensus semantics, the formal
  network-tier contract, or the mainnet security/governance authority.
