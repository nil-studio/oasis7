# world-runtime 模块设计总览

审计轮次: 6

- 对应需求文档: `doc/world-runtime/prd.md`
- 当前任务状态与变更过程：对应 GitHub task issue / Project 与 Git history
- 对应文件级索引: `doc/world-runtime/prd.index.md`

## 实现边界与共享值所有权

`oasis7_runtime` 拥有完整 World、权威状态、准入、事务、cognition、持久化与恢复；`oasis7_world_service` 拥有受权服务用例、已提交投影、结果与发布，依赖 Runtime 与合同层。`oasis7` 保留宿主组装及现有消费者需要的兼容门面。Runtime 与 Simulator 是并列的执行实现，不通过对方拥有共享领域值。资源库存和元素预算属于 `oasis7_agent_api::world_resource`，机器人规格和视觉实体属于 `world_entity`，chunk 坐标与纯几何派生属于 `world_geometry`；Runtime 专属 Agent/body 状态归 `oasis7_runtime::agent_state`。旧 `models` 和 Simulator 类型路径仅作为现有消费者的兼容导出。

资源清单 DTO、规范 hash 和纯上下文由 API 拥有；`runtime::chain_resource` 从权威状态派生，`simulator::chain_resource` 从内核状态派生。派生是两侧的 free function，不在共享 DTO 上挂接执行层固有方法。资源 commit 引用仍是既有资源关联合同，不扩大为共识最终性证明。

`runtime::service_admission` 拥有 owner/delegation、task fence、nonce、canonical result 和 hot/cold history 的校验。在线执行、snapshot restore、journal replay、retention staging 使用同一实现；Service 旧路径只重导出并调用 Runtime。`ValidatedStaleCognitionRejection` 由 Runtime 拥有，其构造和拆解限制在 Runtime 内，Service 不能从诊断文本构造凭据。

`World::canonical_agent_service_context` 从同一个 World 及其当前 binding 生成只读 provider context；Service 的 committed projection 和 task context 不再借 Viewer 构建。该接口不返回签名密钥、sidecar 缓存或提交凭据，准入后执行时的授权与幂等重检保持原合同。

Runtime 的源码编译通过 API 的 `ModuleSourceCompiler` 过程内能力注入。独立 Runtime 的缺省能力拒绝编译；原生宿主在新建和 snapshot/journal/formal generation 恢复验证完成后，经 `runtime_host` 显式安装既有编译器。clone 与同一执行实例的 rollback/attached-history 恢复保留该能力；serde 不编码该能力，宿主接管恢复对象时重新安装。该拆分不改变动作执行阶段、错误分类或 production release 禁止源码编译的 gate，replay 不调用编译器。依赖 NodeSnapshot 的观测、开发节点配置和进程/环境/文件编译适配同样留宿主。

## 1. 设计定位

`world-runtime` 是 world-infrastructure 的上层确定性执行层：它不决定产品规则或网络/共识部署，而是把已排序的世界动作确定性地执行为事件、状态根、receipt、checkpoint 与可验证回放。gameplay、Agent 与 Viewer 经版本化协议提交 intent 或消费已提交状态；它们不拥有世界推进权。

链节点进程组合由 `oasis7_chain_runtime` crate 拥有，世界执行归 `oasis7_runtime`，授权投影构造归 `oasis7_world_service`，跨实现与进程适配留在 `oasis7`。纯投影承诺、规范编码和包含证明由 `oasis7_client_api::formal_projection` 提供；世界适配函数 `world_service::formal_projection::commitments_for_world` 只枚举当前合法 audience、生成已提交投影并调用共享算法。客户端协议层不得反向依赖世界或节点；编码、hash domain、Merkle 结构和预算保持同一协议。端点持久化与恢复仍由 `oasis7_distfs` / `oasis7_node` 负责，进程入口消费这些接口，不形成第二套存储或恢复实现。

`oasis7` owns the shared world library and no executable targets. `oasis7_launcher_backend` owns game/Web launcher backends; `oasis7_viewer_runtime` owns viewer processes; `oasis7_client_cli` and `oasis7_ops_tools` own client commands and world operations. Newapi service, local provider bridge and provider diagnostics/benchmarks have separate package ownership. Observability, hosted-access policy and LetAI credit adaptation are shared libraries; libraries and executables must not borrow another binary's source. Client CLI production defaults exclude Wasmtime/libp2p. Entrypoints that execute local worlds, resolve authoritative simulator configuration or serve viewers retain explicit world-library dependencies instead of duplicating protocol, authorization or recovery logic. Executable names, CLI, serialized formats and persistence behavior remain unchanged.


Launcher bootstrap address parsing is shared by desktop and backend launchers in `oasis7_launcher_config`; world execution does not own CLI topology parsing. Viewer HTTP/WebSocket and offline socket hosting belong to `oasis7_viewer_runtime`, consuming the existing authoritative world/view protocols. `oasis7_wasm_materializer` owns artifact transport, external build invocation and cache resolution, while the world adapter preserves module identity, hash validation and admission failures. Independent formal read proof verification belongs to the client layer; world-specific projection construction and typed gameplay adapters remain with the world. These ownership boundaries preserve existing wire bytes, signer/trust configuration, recovery behavior and artifact fallback order.


## 2. 阅读顺序
1. `doc/world-runtime/prd.md`
2. `doc/world-runtime/design.md`
3. `doc/world-runtime/prd.index.md`
4. 对应 GitHub task issue（仅在需要当前任务状态、阻断或验收过程时）
5. 下钻 `governance/`、`module/`、`runtime/`、`wasm/`、`testing/` 等专题目录

## 3. 目标设计结构

- **分布式基础层（`doc/p2p/` authority）**：governance registry、validator set、Tendermint/CometBFT-style BFT finality、P2P、DistFS、checkpoint/source selection 与 state sync。它按合法激活 authority profile 决定何时 action batch 的正式提交证明及持久条件成立；首次开放目标为受控单权威，BFT 为后续经验证激活的 profile。
- **版本化 consensus-execution protocol**：请求绑定 `world_id`、protocol/runtime-manifest version、parent committed height/hash、ordered action envelope 与 `action_root`；结果绑定 execution block/hash、`state_root`、receipt/journal references 和结构化 reject/fault。in-process adapter 是当前部署选择，future IPC adapter 是同一协议的另一 transport；两者必须跑同一 conformance 与 replay fixtures。
- **确定性执行层（本模块 authority）**：从同一个 committed parent state 重放 action sequence，运行已治理激活的 runtime manifest，持久化结果与 checkpoint/replay anchors。proposer 的结果只是候选；`bft` 下每个 active validator 必须 independently re-execute，并只在 root/hash/result 相符时对 BFT vote/certificate 作出本地执行证明。
- **消费者层**：ordinary player game + light companion、operator full infrastructure node、dev/local game + embedded/full local node 都使用同一协议。消费者只投影 finalized/verified state；light companion 可提交 signed intent，但不模拟权威状态。
- **验证层**：determinism/replay, manifest/artifact compatibility, root mismatch, recovery and adapter conformance tests. 任何 root、artifact、certificate、continuity 或 replay mismatch 都 fail closed。

## 4. Runtime activation, deployment, and recovery target

- **Governed activation**：ordinary runtime upgrades are content-addressed manifests/version selected by governance and activated at a committed height. Validators prefetch and verify before activation; missing or mismatched artifacts block execution/voting. Node software delivery does not itself activate deterministic world semantics.
- **Four release lanes**：(1) rolling node-software patches, (2) governance-activated runtime manifests, (3) independent client applications, and (4) coordinated foundational protocol upgrades/forks. The last lane is required when consensus rules, the consensus-execution protocol, or host ABI become incompatible; it needs coordinated binaries and migration proof.
- **Fail-closed availability**：when finality is unavailable, player/light-companion profiles expose only the last verified state plus clearly pending intents with no world effect. Dev/local execution uses a separate `world_id` and is never reconciled into the global history.
- **Recovery trust chain**：fixed world/chain/genesis identity -> legal historical profile/epoch/manifest activation chain -> verified committed checkpoint/header bound to the authority active at that height -> hash-bound snapshot -> canonical committed-log replay -> state-root verification -> serve/vote. Snapshot and DistFS are transport/cache material, not independent authority.

<a id="runtime-authority-commit-recovery-target"></a>

### Authority-profile commit and recovery seam（目标）

[执行需求](prd.md#runtime-authority-profile-commit) 对接 [P2P authority profiles](../p2p/prd.md#p2p-authority-profiles)；同一 ordered-input/receipt/replay seam 接受合法单权威证明或 BFT 证书，不新建第二套状态机。prepare 与 committed decision 分开；原子校验 writer epoch、parent/head、顺序后，完整输入、结果、事件、里程碑、去重与正式证明/决定在本机及独立故障域达到持久条件，才发布 committed。实际首轮路径适用，局部 prepared publication 和 local fsync 不证明该端到端合同。

断连/超时而提交未知时查询原 identity 的持久决定，不能重新生成 action identity；恢复不从预备记录推导效果。已知无效果 pending 按当前权限/资源/manifest 重审，未知提交先核对。历史先验证只读，当前权威/head/版本/持久条件与旧 signer/写权隔离成立后再写入；replay 不回退 epoch、不重发外部行为。

H 交接记录的结果根必须与其正式证明相符；H 提交后 H+1 只允许新 BFT profile。新集合未产生首提交或重启不能回到旧权威。故障验证应分别注入副本写入、正式提交决定/证明写入、committed 回复前失败，并对账 root、receipt、资产、授权、里程碑及幂等；实际独立故障域恢复另取运行证据。

## 5. Current implementation boundary and target gap

Current code/doc contracts already bind ordered actions, roots, committed execution records, artifact hashes, ordered registry/module-lifecycle events, checkpoints, and canonical replay. Public direct module call/command, trusted capability command, and governed registry/module-lifecycle proposal apply now prepare typed projections against a borrowed immutable base and publish only after state/manifest/registry/artifact/schedule/cache-invalidation/journal/allocator/backpressure/consensus validation succeeds. The governed proposal seam preserves sorted lifecycle-event order followed by `ManifestUpdated` and `Governance::Applied`, then installs the prepared batch once; post-prepare failure injection proves that none of those projections become visible on failure. Native tick migration has also begun at the nested-reducer boundary: factory depreciation, native due-economy completion, and due material-transit completion now derive deterministically sorted event bodies from an immutable world view before canonical publication, with non-mutation regressions. Due-economy preparation preserves the build-before-recipe phase boundary and each phase's priority/ready-time/job-id ordering. Material-transit preparation additionally projects replay-deterministic SLA counters in the same due-job order and installs that projection only after every completion event has been appended. Agent-claim epoch processing uses a narrower prepared-decision seam: it prepares at most one event against the current state, publishes it, then re-reads claim and balance state before any follow-up so same-owner upkeep funding, grace, release, reclaim, and refund provenance retain their required sequential semantics. These are composable precursors, not a root batch commit; public step wrappers still provide their coarse cloned-`World` rollback boundary. Authority-drift and post-prepare failure injection also cover direct module state/output rollback; existing-format `ModuleCallFailed` audit events are recorded separately only after staged business mutations are discarded. The overall execution capability remains `partial`, because step/tick and remaining lifecycle/recovery entrypoints, persisted instance-state alignment, recovery/replay, durable external effects, and receipt/outbox publication are not one proven transaction boundary. The current contracts also do **not** yet prove the broader target end state: signed per-validator re-execution results; a persisted/verifiable >2/3-stake BFT commit certificate; prevote/precommit rounds, locks, timeout/view-change; protocol-versioned in-process/IPC conformance; governed runtime-manifest activation readiness/rollback; light-companion proof verification; or the complete checkpoint/disaster-recovery trust chain. These are implementation and verification gaps, not claims of present network readiness.

Restricted starter-grant expiry now follows the same immutable preparation pattern: eligibility, balance-derived expiry amounts, metadata, and BTreeMap account order are frozen before sequential canonical publication. This remains a nested precursor under the existing root rollback boundary.

Threat-heatmap preparation follows the same composable boundary: `prepare_threat_heatmap(&self)` derives a fresh `BTreeMap` from the immutable world view, while `refresh_threat_heatmap()` installs that prepared map once. The pure preparation is covered for snapshot, journal, and prior-heatmap nonmutation; this exposes composability only and does not claim new root transaction atomicity or alter War/Crisis semantics.

Emergency-brake activation and release now use a typed prepared sidecar delta: guardian validation and the existing activation-max/release-`None` reducer semantics are preserved, while event id/era, journal, backpressure, and tick-consensus preparation complete before one installation of the World brake field. A post-prepare failure leaves the snapshot, brake, journal, allocator, and consensus unchanged. The brake sidecar remains outside the canonical `WorldState` root schema, and this bounded publication seam does not claim a whole-World root transaction.

Finality epoch snapshot set/remove now uses a dedicated prepared governance sidecar delta. Predecessor, normalization, and removal-drift checks are shared with replay reduction; post-prepare failure leaves the persisted snapshot map, event allocator, journal, and tick consensus unchanged. The map remains outside the canonical `WorldState` root projection, so this closes one direct publication boundary without claiming a whole-World transaction.

Emergency-veto proposal publication now prepares the exact rejected proposal sidecar before the canonical event envelope, journal, allocator, and tick-consensus install. The existing guardian, approved-status, queued-state, reason, and replay checks remain authoritative; a post-prepare failure leaves the proposal map and all publication observables unchanged. Proposal sidecar data remains outside the `WorldState` root projection, so this is a bounded direct-mutator seam rather than a whole-World transaction.

Identity-penalty appeals now prepare an immutable replacement record before canonical event publication, and replay uses the same replacement helper. Existing appellant/reason, Applied-status, deadline, legacy detection-field backfill, stage-evidence hash, and chain-extension semantics remain unchanged; a post-prepare failure leaves the penalty map and publication observables unchanged. The penalty sidecar remains outside the `WorldState` root projection, so this is a bounded direct-mutator seam rather than a whole-World transaction.

Identity-penalty appeal resolution now prepares both the replacement penalty record and target identity profile before publication; the canonical borrowed state projection overlays the profile map entry for the next tick-consensus root. Accepted resolution retains slash refund/status restoration, rejected resolution retains timestamp-only profile updates, and both paths reuse the exact replay helper and evidence-chain hashes. Missing profiles and post-prepare failures leave all observables unchanged; this remains a bounded seam, not a whole-World transaction.

Identity-penalty application now previews the next penalty id and prepares the penalty record, profile mutation, and allocator successor before publication. The borrowed profile-map projection preserves sorted insertion for an absent profile, while retry/replay use the same staged result and consensus root. Invalid stake, duplicate, signer, and post-prepare failures do not consume the allocator or publish partial state; this remains a bounded seam, not a whole-World transaction.

Governance proposal creation and shadowing now use the same prepared publication seam: proposal creation previews the proposal id and id era, while both the proposal sidecar replacement and event publication are installed only after journal, allocator, and tick-consensus preparation succeeds. Existing manifest/patch, module-change, status, and manifest-hash validation remains in the public callers; replay retains the raw Proposed/ShadowReport payloads and proposal-id rollover semantics. The proposal sidecar remains outside the canonical WorldState root, so this closes two direct public mutator boundaries without claiming a whole-World transaction.

Approval and queueing now prepare the `Approved` and `Queued` event pair against an immutable proposal view and install the final proposal replacement, retained journal suffix, event allocator, backpressure count, and tick-consensus candidate together. The pair remains ordered and retains the existing bounded-journal eviction semantics; reject decisions retain their single `Approved` event shape. This is a bounded direct-mutator publication seam, not a whole-World transaction.

Standalone public action-module routing now uses the same borrowed
`TrustedCommandStage` for the complete sorted invocation set. Module state,
effects, emits, runtime charges, cache-miss additions, allocators, journal, and
the single tick-consensus candidate are prepared against the staged context and
installed once. A durable grouped base-head (state, journal, authorization,
policy, artifacts, scheduling, consensus, and governance) is checked before
any install write; a stale head returns `DistributedValidationFailed`. A module fault discards staged business output before one existing
`ModuleCallFailed` audit; an infrastructure failure after preparation installs
nothing and emits no audit. This closes only the direct action-route seam and
does not claim a root `ExecutionTransaction` for `step()` or other nested
event/tick paths.

Standalone public event-module routing now uses that same borrowed stage for
the sorted subscribed invocation set. The input event bytes and post-event
context are prepared together with module state, effects, emits, runtime
charges, cache-miss additions, allocators, journal, and tick consensus before
one install. The same durable base-head check runs before publication, while
live process-local cache entries and wall-clock telemetry are preserved.
Module faults discard staged business output before one existing
`ModuleCallFailed` audit; post-prepare infrastructure failures install
nothing and emit no audit. Tick schedule and routing metrics remain a separate
lifecycle boundary and are not implied by this event-route guarantee.

Standalone public tick routing now stages due schedule removals, wake
directives, deterministic routing metrics, module business output, cache-miss
additions, allocators, journal, and backpressure together. Its prepared envelope carries
no consensus candidate: `run_modules_for_current_tick()` remains the owner of
the single final tick record. A snapshot taken after direct routing but before
that finalization is not a replayable canonical checkpoint. A finalized
checkpoint is a valid starting cut; current event-suffix replay still lacks
complete schedule/metric reconstruction for a later target. This limitation
does not remove the [earlier-checkpoint recovery target](#runtime-journal-suffix-recovery-design).
Due-record preflight errors retain the compatibility exception of
recording metrics directly while leaving the schedule unchanged.

Gameplay-cycle migration is stage-scoped: economic-contract expiry now freezes the due atomic-contract set, contract-id order, status-derived reputation deltas, and event bodies from an immutable view before publication. Governance proposal finalization is prepared only after those economic events apply; it freezes the sorted due proposal keys and the vote-derived winner, quorum, threshold, and final event payloads from that immutable post-economic state before publication. Crisis lifecycle then prepares at most one deterministic auto-spawn event from the post-governance state and publishes it before freezing the crisis-id-sorted timeout vector from the resulting state. War conclusion deliberately uses a narrower one-event decision: it prepares the lexicographically first due active war from the current post-crisis state, publishes that outcome, then re-reads resources and reputation before preparing the next due war. This preserves both contract-expiry effects and earlier war participant outcomes in later scoring and settlement. Active gameplay-module lifecycle directives follow the same one-directive boundary after their envelope is decoded: each directive is translated against the current immutable state into zero or one domain event, published canonically, and only then may the next directive resolve state-derived fields such as the war loser fallback; zero-point meta grants remain no-ops and malformed envelopes retain their existing module-failure path. These stage-local prepared decisions and vectors remain composable precursors under the outer cloned-`World` rollback boundary, not a whole-cycle or root transaction commit.

## 6. Architecture status, migration proof, and execution boundary

The runtime uses four explicit status labels so that a protocol surface is not
mistaken for a completed end-to-end capability:

| Status | Meaning | Required evidence |
| --- | --- | --- |
| `current` | Present in the current production or compatibility path. | Code, persisted shape, or an existing regression identifies the path. |
| `partial` | Only some entrypoints, lifecycle stages, or identity scopes satisfy the target. | The missing command path, consumer, or identity scope is named. |
| `target` | Normative destination, not a current availability or release claim. | Invariants, ordering, and failure semantics are explicit. |
| `proven` | Target behavior is demonstrated by repeatable execution, rejection, snapshot, recovery, and replay evidence. | A rerunnable fixture/test and its receipt/state-root or migration evidence are available. |

The present architecture is therefore `current` for the Kernel boundary,
ordered action/replay, and persisted module identity; `partial` for
Institution migration, unified transaction coverage, and command-path instance
authorization. A local staged command path or a compatibility record does not
upgrade those surfaces to `proven`.

### 6.1 Institution Migration Test

The first migration proof follows product SC-32: a governed activation
boundary selects exactly one Alliance/EconomicContract pilot. Producer,
runtime, and WASM roles supply evidence; they do not replace governance
approval. The test is `proven` only when all of the following hold:

1. A governed manifest binds artifact hash, schema/version, stable
   `instance_id`, activation, owner/subject, and capability limits.
2. The command traverses the same permission, budget, quote/resolve,
   affordability, staged Kernel apply, receipt, and journal path as native
   actions. The module cannot write canonical `WorldState`, the canonical
   journal, or an external effect directly.
3. Accepted state, event, receipt, and checkpoint commitments are consistent;
   a confirmed precommit invariant, budget, persistence, or artifact failure
   has no partial business effect and publishes a stable rejected/fault
   disposition only when it can be committed. Unknown persistence status is
   reconciled before retry; postcommit maintenance cannot revoke the result.
4. Restart, snapshot restore, canonical journal replay, and adapter
   conformance reproduce the same state root/receipt for the same accepted
   input, while legacy shapes remain readable only through an explicit
   compatibility adapter.
5. Two instances of the same artifact, with different `instance_id` values,
   can execute concurrently without state, event, receipt, or descriptor
   cross-write. A command target may never fall back to another instance with
   the same `module_id`.
6. The `institution-migration-v1` evidence bundle binds manifest, inputs,
   roots, receipts, snapshot/replay/recovery, and legacy-conformance reports.
   Preview/stale/deny/no-effect outcomes debit nothing; an accepted effect has
   exactly one debit and receipt; retries and replay neither double-charge nor
   create replay credit.
7. Post-commit external effects first enter a durable outbox/effect ledger.
   `effect_id` is deterministically derived in the receipt domain from effect kind,
   canonical target/payload, and an effect ordinal; the ordinal only distinguishes
   intentionally repeated equal effects in one receipt. `(world_id,
   execution_receipt_id, effect_id)` is the idempotency key. Same-key/same-payload
   redelivery is deduplicated; same-key/different descriptor, intent, or payload
   fails closed as `effect_id_conflict` without overwrite or dispatch. Crash recovery
   may redeliver an unacknowledged record, while canonical replay never performs the
   external business effect again.

Until this test is proven, Alliance/EconomicContract remain compatibility
surfaces, War remains deferred, Governance remains a Kernel guardrail, and
open Institution extensibility remains `target`.

### 6.2 Unified ExecutionTransaction (architecture P0)

`ExecutionTransaction` (or an equivalent staged boundary) is an architecture
P0 because it establishes one state-transition meaning before further module
expansion. P0 here describes dependency ordering, not an assertion of an
active production incident and not a requirement for a one-shot `WorldState`
rewrite. The boundary covers every world-effecting `step()`/
`step_with_modules()` action, native compatibility action, module command,
tick directive, direct/trusted command, install/upgrade, and recovery or
migration write.

The transaction stages parent state, logical time, resource reservation/debit,
module instance state, pending effects, tick schedule, journal/event data, and
sequence counters. Quote/resolve may run as a read-only preflight, but it must
bind parent, manifest, input root, and freshness and cannot mutate world state.
Module calls and Kernel/schema/capability validation operate only on the staged
view. A successful transition publishes state, events, receipt, and execution
commitment at one commit point. A confirmed precommit invariant, budget,
artifact, serialization, or persistence failure discards the staged business
view; a rejected/fault disposition is published only if that disposition can
itself be committed. Persistence errors whose commit status is uncertain must
be resolved before rollback or retry; a postcommit maintenance error cannot
revoke the committed result. External non-rollbackable effects
are receipt-driven after commit through a durable outbox/effect ledger. Each
record binds the canonical effect descriptor, execution receipt, roots, intent
hash, dispatch/ack state, idempotency key, and observed external receipt. The
fixture covers commit-before-dispatch crash, dispatch-before-ack crash,
same-key/same-payload duplicate, same-key/different-payload conflict, duplicate
ack, restart redelivery, and replay with no external adapter call. Recovery may
redeliver an unacknowledged record; canonical replay rebuilds ledger state without
repeating the external business effect.

The migration is incremental: first route one existing command family and its
tick/replay fixtures through the boundary, then widen coverage while keeping
the legacy readers and deterministic ordering. This design does not promise a
big-bang ECS conversion, independent shard finality, cross-shard commit, or a
dynamic World Database.

#### 6.2.1 Explicit transaction and typed-delta model

Module-state updates and runtime-charge events use the prepared event-publication
boundary. Charge preparation owns one replacement payer cell and only touched
treasury entries, applies compute then electricity debits locally, and preserves
negative-fee/missing-payer/error ordering, saturating credits, zero-fee activity
timestamps, and shared-resource aggregation. Replay invokes the same pure
preparation followed by infallible installation. Publication hashes the prepared
entries through the existing borrowed `CommandStateOverlay`; it does not clone
`World` or `WorldState`. Consensus validation and the post-prepare failpoint precede
state, journal retention/backpressure, event id/era, and consensus installation.
`ModuleEmitted` remains a no-state event in the existing output order. This closes
these event seams only, not all nested operations or the root transaction.

`ModuleInstalled` and `ModuleUpgraded` prepare sparse instance/payer/treasury
replacements plus the module-keyed target and install-counter successor. Fee
validation retains priority over upgrade instance/owner/module/version checks.
The direct state reducer prepares legacy world-ledger/cache normalization only
after validation, so rejected events cannot leak compatibility migration writes.
The world reducer also prepares registry-backed schedule lookup before installing
anything; replay shares those preparations. Install state keys keep blank-to-module
and trimmed-nonblank compatibility, while nonblank schedule keys and upgrade lookup
remain raw. Inactive events remove schedules without requiring registry records;
active events require a record even when it has no tick subscription. A borrowed
projection merges the touched instance, target, material ledger/cache and fee
entries, including exactly one routed mailbox event, into the consensus root.
Publication installs only after consensus checks and the existing failpoint.
Rollback uses the same prepared state reducer and exact rollback validation
messages, preserving the historical absence of instance-key schedule updates.

Governed install/upgrade/rollback extend `PreparedGovernanceProposalApply` with
the final lifecycle event before installation. The tail checks its proposal id
and applied-manifest hash, prepares the owned instance delta against unchanged
state, and resolves scheduling against the prepared registry. Its borrowed state
projection uses the prepared manifest hash; event id/era, retained journal,
eviction accounting, and final consensus are extended locally. Both logical
post-prepare failpoints (governance and final lifecycle) precede any installation.
Only then are governance sidecars, cache invalidations, instance state and its
single mailbox event installed. Governance errors retain the caller's existing
`ActionRejected` conversion; lifecycle-tail errors propagate without installing
the governance batch. Local certificate policy/build ordering and explicit
certificate validation are unchanged. Proposal/shadow/approval prelude remains
outside this bounded commit; it is not full action/root atomicity.

Release completion supplies an owned typed context to the install path instead
of inferring its result from the last live journal entry. Both governed and
already-registered installs extend their unpublished envelopes with sorted
product, recipe, factory events and `ModuleReleaseApplied`. Each stage validates
its root/consensus candidate and logical failpoint before any canonical install.
The sparse release delta owns only changed profile/request/mapping entries,
affected agents and legacy world-material normalization, never a `World` or
`WorldState` clone. Its agent overlay begins with the fee-debited install cell
and install mailbox, so a shared installer/operator cannot overwrite the debit
or lose/duplicate events. Borrowed projections merge both instance and release
entries under the prepared manifest hash. Final request/status/mapping checks
retain their order without partial request mutation. Raw profile/final-status
reducers and publication reuse those preparations, preserving replay and direct
event semantics. Input-order prevalidation, profile overwrite rejection,
proposal-id-zero rejection for nonempty profiles, and empty-profile success are
unchanged. This adds release business completion, not prelude deduplication or
global action/root atomicity.

The three release precursors (`ModuleReleaseRequested`,
`ModuleReleaseAttested`, and `ModuleReleaseRolesBound`) also use the sparse
release projector for raw, live-action, and replay publication. It additionally
projects the request allocator and role-binding updates, while touched agent
cells carry activity and the routed projection supplies exactly the
operator/requester mailbox. Attestation mapping lookup remains late in reducer
error order but now fails only against the candidate. Role binding updates both
operator and a distinct target's activity while routing only to the operator.
Legacy world-material normalization and the final root are projected without a
`WorldState` clone before the publication failpoint.

Release review events use that same sparse request/mapping/agent preparation.
Shadowing validates request and status, then requires and clones the mapping
before changing either copy. Role approval preserves normalized-role,
required-role and existing-approver priority; rejection preserves terminal
status and nonblank-reason priority. Approval and rejection continue to update
a mapping only when one exists, while raw events still tolerate a missing actor
and therefore omit activity/mailbox delivery. The prepared request, optional
mapping, actor replacement and legacy material normalization drive the borrowed
root projection before journal, allocator, retention, consensus and failpoint
installation. Action handlers retain their stricter actor/policy validation and
continue to propagate a valid Shadow event's required-mapping reducer error.
This review-event boundary does not include request submission, attestation,
role binding or outer-action idempotency.

Marketplace listing and bid actions prepare their order and optional immediate
sale as one publication batch. The event-bound delta owns only affected agents,
one artifact's owner/listing/bid entries, fee-resource replacements, market
allocators and legacy world-material normalization. An absent staged listing or
bid entry means deletion; the borrowed serializer excludes it while preserving
unaffected entries and canonical key order. Raw listed/bid/sale reducers use the
same pure preparation before legacy normalization, so failed buyer lookup,
debit, seller credit or bid-reference validation cannot remove agents or change
ownership. Matching reads the staged fee-debited cells and updated order book;
listing price, highest-bid/lower-order-id selection and exact-tie vector order
remain unchanged. Every logical event stages its routed mailbox, journal
retention/allocator and consensus root and failpoint before one infallible
install. Artifact ownership does not transfer module instances or schedules.
This is a marketplace business boundary, not whole-World cloning or general
action idempotency; delist, cancellation, destruction and deployment are not
newly composed into it.

Artifact deployment extends the same one-hash sparse state preparation with
publisher fee settlement, owner replacement and listing/bid deletion. Source
and binary action validation still finish first; a validated owned registration
then holds the hash and immutable bytes beside the prepared deployed event.
Journal retention, cursor, consensus root and logical failpoint validate before
the event state and artifact set/byte map install. Standalone artifact
registration retains immediate validate-then-install behavior by using the same
owned registration. Same-hash redeployment therefore continues to charge the
fee, overwrite ownership and clear market orders while leaving the module cache
untouched. External source compilation is outside rollback. Because the
deployed event records only hash and byte length, event-only replay reconstructs
fee/owner/market state but not new byte content; durable bytes remain the
artifact persistence sidecar contract.

Marketplace teardown completes the one-hash sparse transition for delist,
bid-cancel and destroy events. Their validation, fee settlement, agent
activity, owner/listing/bid deletions, legacy world-material normalization,
journal retention, allocator and consensus root are prepared before one
infallible install. Raw destroy remains a canonical-state-only event and does
not remove artifact membership, bytes or cache entries.

The `DestroyModuleArtifact` action additionally carries a prepared retirement
sidecar bound to the identical destroyed-event hash. Only after publication
preparation succeeds does installation remove artifact membership and bytes
and reset the complete module cache while retaining its configured capacity.
Registry, instances and tick schedules are outside the retirement delta. A
failed tail installs neither canonical teardown nor sidecars, so retry charges
and publishes once. This does not change action validation priority, raw-event
sidecar compatibility, persistence, or general action idempotency.

The standalone `apply_module_changes` compatibility API now reuses the governed
lifecycle projector but owns a separate batch publication boundary. It keeps
the stable category order register, upgrade, activate, deactivate and the
stable module-id sort within each category, while projecting registry records,
artifact membership, tick schedules and targeted prepared-subscription cache
invalidations before allocating any durable event. Every `ModuleEvent` retains
the caller's proposal id and actor fields with no `caused_by` value. Journal
retention, event-id era and the final tick-consensus record install once after
all events validate; an empty changeset is a strict no-op. This does not alter
the raw single-`ModuleEvent` path or governed proposal application.

The target production boundary is an explicit `ExecutionTransaction` holding
a read-only canonical `World` base, a `TransitionBuffer`, and a `Live` or
`Replay` mode. `TransitionBuffer` is a typed overlay rather than a cloned
`World` or reflection/JSON patch. It groups deltas for world state, non-state
runtime authorities, rolling sequences, journal batches, pending/inflight
queues, schedules, replay-deterministic metrics, consensus, effects, and the
persistence plan. Scalar changes use typed value replacement and collection
changes use typed map/set operations. Wall-clock observability and process-local
caches never participate in deterministic commitments.

Every public world-effecting entrypoint creates exactly one root transaction.
Nested module routing, reducers, and `append_event` share that root buffer;
`append_event` is transaction-internal and never mutates canonical `World`
directly. A nested operation may create a lightweight savepoint containing
overlay mutation marks, event-batch length, sequence cursors, queue/schedule
operation marks, and deterministic-metric marks, but it cannot independently
commit. A child fault aborts the root. Only an existing domain rule that
explicitly permits a stable business rejection may restore a savepoint and
append that rejection inside the same root transaction.

The transaction base head binds at least the state root, manifest and module
registry roots, journal length and commitment, event id/era, logical time,
queue roots, tick-consensus head, and capability-authorization root. Prepare
performs every fallible validation, allocation, capacity check, commitment,
serialization, and persistence-staging operation against the base plus overlay.
It yields a `PreparedCommit`; installing that commit is a deterministic,
non-fallible typed write. A changed base head rejects the commit as stale and
discards the buffer. Public readers observe only the previous canonical
generation until publication.

#### 6.2.2 Disposition and idempotency contract

Execution returns an accepted, rejected, or faulted disposition when it can
durably form a stable outcome. Accepted atomically publishes all business
state, journal, sequence, queue/schedule, receipt, commitment, deterministic
metric, idempotency, and outbox records. Rejected represents a deterministic
input, authority, budget, resource, or freshness refusal and contains no
accepted business effect. Faulted represents traps, schema/artifact mismatch,
invariant failure, or confirmed precommit serialization/persistence failure
for which a stable fault disposition can be committed; it cannot be converted
into a recoverable domain rejection. Commit uncertainty is not itself a
durable fault disposition. If no stable disposition can be committed and
non-commit is confirmed, the API returns an infrastructure error and the old
canonical projection remains byte/semantically unchanged. If commit status is
unknown, publication stays fenced until the exact attempt is reconciled; if
commit is confirmed, its result remains authoritative. Ordinary indexed saves
follow the [generation outcome contract](#runtime-generation-persistence-design).
An audit record for
a rejection or fault, when required, is part of that same root commit and is
never appended by a second best-effort transaction.

The first stateful Phase 1 queue slice fixes the public effect-emission
disposition more narrowly. Capability missing, expiry, or kind mismatch is a
preflight rejection with no intent/event allocation or audit. A deterministic
policy deny advances the intent allocator and atomically publishes exactly one
`PolicyDecisionRecorded(Deny)` disposition, without `EffectQueued`. After an
allow decision, queue admission, both journal events, rolling event/intent
sequences and eras, eviction metrics, and tick consensus are one prepared
batch: hard queue-full or a post-prepare infrastructure failure installs none
of them. A full queue may still deterministically evict an existing unlinked
intent, preserving the established bounded-queue rule; authorization-linked
intents are not evictable. Raw `EffectQueued` and `ReceiptAppended` now also use
a typed sidecar replacement for
pending/inflight queues and pending-effect eviction accounting before canonical
event publication. The raw seam preserves queue-full, FIFO eviction,
unknown-intent, duplicate-removal, journal, allocator, retention, and consensus
ordering, and installs only after the post-prepare failure boundary. These
slices do not make durable outbox or replay allocation part of the unified root
buffer yet.

The next Phase 1 stateful slice migrates public receipt ingestion away from a
cloned `World`. A typed prepared receipt delta validates the known intent,
previews the optional authorization-closure event, computes the authorization
audit/link/root overlay, signs or verifies against the virtual closure journal,
removes the pending or inflight intent, and prepares the final receipt event,
journal limit, event allocator, deterministic metrics, and one consensus
candidate. The install step is infallible. A signature, authorization,
consensus, or post-prepare failure therefore closes no link, consumes no queue
entry, and publishes no event or sequence. Existing event order, `CausedBy`,
receipt DTO, and replay-only reducer behavior remain compatible.

This receipt slice does not establish durable duplicate-receipt idempotency or
outbox acknowledgement. The current DTO lacks the approved
`(world_id, execution_receipt_id, effect_id)` identity and canonical descriptor
hash ledger, so same-receipt retry and conflicting-receipt rejection remain a
Phase 3 authority/schema migration rather than an inferred `intent_id` rule.

The first bounded CapabilityAuthorization slices cover public grant
registration, invocation-context installation, budget-account installation,
agent-identity installation, and proof-bearing authority/revocation
administration. For a System subject, the
optional `SystemIdentityInstalled` and required `InvocationContextInstalled`
events are prepared as one typed batch; `BudgetAccountInstalled` uses the same
batch with a typed budget-map projection, while `GrantRegistered` stages the
canonical grant JSON map. `AuthorityInstalledWithProof` reuses the canonical
reducer transition validator while staging the authority record, proof,
revocation, supersession, and finalized receipt projections; revoke,
supersede, and trust-root rotation delegate to this seam. Preparation projects
only the affected authorization maps, previews event ids and eras, retained
journal/backpressure accounting, the final authorization root, and one
tick-consensus candidate; installation then publishes them without a fallible
step. A post-prepare failure therefore publishes no system/agent identity,
context, grant, budget account, or authority transition and consumes no
allocator, journal, consensus, root, or deterministic metric state. Identical
installation remains a no-op, and committed events retain the existing replay
reducers and event order.

Raw agent-intent lifecycle publication uses a full-event-bound sparse
projection shared with replay. It clones only the affected agent cell and at
most one intent-ledger entry; completed transitions first bind their receipt to
the prospective event id and existing journal witness. The projected root
combines the candidate intent/ledger state with exactly one routed mailbox
event before allocator, journal, retention, consensus, and the post-prepare
failpoint are installed. Provider-advisory and historical no-op reducer
semantics remain intact, while the public multi-event chat workflows retain
their existing outer sequencing boundary.

Raw `CommandCommitted` replay-equivalent publication uses a separate,
full-event-bound prepared delta. It validates against the current state,
manifest, journal head, invocation context, authority state, and durable effect
queues while staging only the five maps it can change: grants, nonces,
authorization receipts, budgets, and effect links. Revocation and invocation
context maps remain borrowed inputs to the full authorization-root projection.
All link checks finish before those staged maps and the root are installed with
the event envelope. This path does not execute a command or replace the
`TrustedCommandStage` live command boundary.

Raw `EffectReceiptCommitted` likewise has a replay-equivalent, full-event-bound
prepared delta, but owns only authorization receipts and effect links. It
preserves the reducer's blank-field, missing-link idempotency, link-binding,
and audit lookup order, accepts the event's arbitrary non-empty effect receipt
id, and derives the full authorization root with the other five maps borrowed.
It neither consumes effect queues nor signs or appends an external receipt;
those remain coupled exclusively in specialized `ingest_receipt` publication.

The trusted capability-command executor and public direct module call/command
now use the same architectural seam without cloning canonical `World`: a
borrowed-base typed stage owns only the
module-state, agent/resource, queue, allocator, journal/backpressure, and
process-local cache projections touched by the command. Authorization budget,
grant, nonce, receipt, and effect-link candidates are validated alongside that
stage; state and authorization roots plus the single tick-consensus candidate
are computed before publication. A post-prepare failpoint precedes one
infallible install sequence, so sandbox/output, budget, receipt, effect, event
ids, journal, and consensus cannot become observably half-published. Direct
module failures discard the business stage before the existing standalone
`ModuleCallFailed` audit is atomically appended. This is a bounded Phase 2
migration, not yet the shared root `ExecutionTransaction` for
all nested command/event paths. Durable receipt/outbox/idempotency, persistence
generation commit, remaining public mutation surfaces, and full replay/restore
closure are still outstanding, so CapabilityAuthorization and the overall
ExecutionTransaction capability remain `partial`.

Each retryable public root operation binds a stable operation id to world,
parent identity, canonical input hash, manifest/activation binding, and target.
The same identity and binding returns the original disposition without new
events, debits, queues, or outbox records. Reuse with any different binding is
an idempotency conflict and fails closed. Nested operations inherit the root
identity and use a deterministic child path; provisional event ids are not
external retry identities. External effects retain
`(world_id, execution_receipt_id, effect_id)` as their durable idempotency key:
same key/same descriptor is a retry, while same key/different descriptor is a
conflict.

#### 6.2.3 Replay, generation persistence, and durable outbox

Replay uses the same typed reducers through a root transaction but may only
consume existing canonical events. It validates event id/era, logical time,
parent, commitment, and order; it never creates an event or calls a sandbox,
LLM, effect adapter, or dispatcher. The complete journal suffix commits once;
validation or reconstruction failure before publication discards every reconstructed delta.
Persistence outcomes follow the [generation contract](#runtime-generation-persistence-design).
Effect events only rebuild ledger/outbox state.

Persistence publishes an immutable generation containing the snapshot,
journal, module store, sidecars, outbox, manifest, hashes, and completion
metadata. All files are staged, synced, and cross-validated before the atomic
latest-generation pointer/index replacement. Durable commit confirmation
includes the required index-file and parent-directory sync; observing the new
index after rename is not by itself durability evidence. Confirmed non-commit
leaves the old generation authoritative; after durable commit is confirmed,
recovery loads the complete new generation even if the process died before
in-memory installation. An uncertain replacement fences publication and
preserves the old and candidate recovery material until resolved. Installation
after confirmed commit contains no fallible work, and generation garbage
collection is outside the business commit seam. Missing, mixed, or
hash-inconsistent candidates are rejected rather than spliced; selecting a
healthy older candidate does not undo a newer commit or open serving.

External dispatch begins only after the durable generation commit. The outbox
uses at-least-once delivery with stable idempotency and independently atomic
lease/ack updates; it does not claim physical exactly-once behavior in an
external system. Receipt ingestion is a new root transaction that atomically
validates identity/signature/authorization linkage, updates outbox and
pending/inflight state, applies deterministic receipt effects, appends the
canonical receipt event, and advances commitments/sequences. Unknown or
conflicting receipts cannot consume a queue item before commit.

<a id="runtime-journal-suffix-recovery-design"></a>
##### Event-suffix control-state recovery

This target implements the [runtime continuation requirement](prd.md#runtime-journal-suffix-recovery).
Checkpoint C and target T bind the same world and legal historical versions; both are completed canonical cuts, with `C<T` for suffix acceptance.
A checkpoint taken between direct routing and final tick publication remains invalid. A complete T snapshot is not a substitute for the C→T fixture.

| Recovery lane | Source and execution boundary | Evidence boundary |
| --- | --- | --- |
| Complete-snapshot roundtrip | Load the already recorded T snapshot and its matching journal position. | Proves serialization/loading of T; no nonempty suffix claim. |
| WorldJournal event suffix | Apply recorded events after C through T using typed reducers, without action, sandbox, LLM or effect dispatch. | Must reconstruct J1/J2 from recorded facts and pass continuation checks. |
| Committed-input re-execution | The [storage replay lane](runtime/runtime-storage-footprint-governance.design.md#storage-history-dependency) executes ordered committed payloads with their historical bindings. | Proves that lane's deterministic result; cannot substitute for event-suffix evidence or replay external effects. |

The runtime owns the recovery projection and its installation. The two control records have separate producers and reducers:

| Obligation | Durable source → restored projection | Detailed owner contract |
| --- | --- | --- |
| J1 | Host allocation/submission provenance → action sequence/era and the associated preallocated queue boundary. Business event counts and maximum numeric IDs are not allocation evidence. | [Action allocator replay](runtime/runtime-numeric-safety.design.md#numeric-action-allocator-replay). |
| J2 | Successful host tick outcome → one stable instance's final absolute wake tick or explicit clear, even with no new module state. | [Module tick journal replay](runtime/runtime-integration.md#module-tick-journal-replay). |

Recovery validates the checkpoint, journal prefix/target binding, record version and required control-record coverage before it can expose a candidate.
C and its suffix must belong to the selected candidate's explicitly bound complete recovery material. Verified `C<T` recovery does not require a complete T snapshot; [G3 candidate reads](runtime/runtime-storage-footprint-governance.design.md#storage-generation-g3-design) forbid assembling independently selected disk generations and remain detached and read-only.
It stages C's business and control projections, consumes the suffix in canonical order, and validates each control transition with the same pure transition semantics used by its producer.
Allocation provenance is applied once per allocation, not once per emitted event; a preallocated ID is checked against its original allocation boundary.
Tick outcomes install their recorded final schedule, not a newly computed result from invoking the guest.
Each control record publishes with the mutations owned by its declared transaction; incomplete or conflicting groups fail without partial installation.
A prior allocation/enqueue transaction and later action consumption/fees remain separate boundaries, joined by provenance; a later step failure must not undo an already durable allocation.
The entire suffix follows the existing single-commit replay target. Validation or reconstruction failure before publication retains the original live world, checkpoint and journal for diagnosis.
If recovery also publishes a durable generation, [G1 outcome handling](#runtime-generation-persistence-design) governs confirmed commit and uncertainty; postcommit maintenance failure cannot restore the old World, and an unknown attempt stays fenced until reconciled.

Matching the current WorldState/tick root alone does not establish control-state completeness: action cursors and module schedules live outside that projection.
Recovery must also validate their provenance, transition continuity and target projection. This does not redefine the existing consensus state-root schema or assign serving authority to a new local digest.
Only a complete candidate can proceed to the existing [recovery service gates](#runtime-recovery-design); historical reconstruction does not renew authority or reinterpret pending business requests.
Ordinary `World::from_snapshot`/load, supported signed rollback v2 and `oasis7_world_repair_rebuild --source-world-dir` share this contract.
The repair tool's generated-world bootstrap mode is a separate creation path; a repaired directory merely loading successfully does not prove J1 continuation.

<a id="runtime-journal-suffix-compatibility"></a>
##### Control-record compatibility and migration

The producer and reader must bind the control-record format version, its complete baseline checkpoint/journal position, and the required record set to the existing versioned recovery material.
This is a target persistence contract, not an already deployed DTO or a change to the optional WASM output ABI.
Activate a new record format only with compatible producers/readers and a verified complete baseline; preserve original history bytes/hashes and record explicit adapter provenance.
An old reader must reject an unsupported required record rather than silently skipping it. Retaining extra data is a safe fallback, but downgrade cannot reinterpret history written in the new format.

| Historical material | Permitted recovery outcome |
| --- | --- |
| Known complete finalized snapshot at T, with no missing-record interval to cross | Preserve existing loading compatibility and verify its control-state completeness. |
| Complete C control baseline plus a suffix conforming to the declared record version | Validate all required transitions and reconstruct T before continuation. |
| Legacy suffix missing records, with other sufficient and verified historical material | Use an explicit legacy adapter that identifies the material and proves the same control projection; never infer success from deserialization alone. |
| Legacy interval whose allocation or tick outcomes cannot be determined | Return the structured compatibility-fault/diagnostic outcome required by the existing compatibility contract; retain material and do not expose it for new effects. A separately verified complete checkpoint may provide another supported route. |
| A required new-format record is missing, conflicting or outside its bound group | Reject as incomplete/mismatched recovery input and leave the prior world unchanged. |

Old ModuleOutput omission means suspend only when decoding that actual output. Absence of a schedule record in an old journal is not evidence of suspend, no invocation or unchanged wake.
Likewise, an absent allocator record does not prove no allocation; `serde(default)` for an era/map only supports decoding and must not fabricate recovery evidence.
J1/J2 define their own record interpretation; one obligation passing never makes the other's legacy interval recoverable.
An explicit legacy adapter or alternative checkpoint does not bypass G3's source/consumption error rules or release a G1 write fence.
These changes preserve existing artifact/signature/finality checks, guest failure disposition and infrastructure-failure atomicity; they do not require a global ID ABI or consensus migration.

<a id="runtime-journal-suffix-validation"></a>
##### Independent continuation acceptance

The following are planned scenarios, not existing test names or executed results. Use one fixed candidate/world/manifest/authority and compare an uninterrupted control at T with C plus its genuine nonempty suffix.
Compare action cursor/era and applicable preallocated queue facts, instance schedules, business state and journal; then apply the same valid future inputs and compare disposition, job identity, invocation instance/tick/order and fees/settlement.
Fixtures must retain the existing signed stable C→T rollback path where specified; no T-snapshot substitution or relaxed negative checks is permitted.

| Planned case | Stimulus and decisive assertion | Existing source to extend; verification scope |
| --- | --- | --- |
| J1-CONTINUE | After C, settle recipe k by T; recover and complete another legal recipe once with the same next ID/era as control, without reusing k. | [Persistence recovery](../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs) and [signed rollback v2](../../crates/oasis7/src/viewer/runtime_live/tests/authoritative_rollback_v2_exact_retry_tests.rs); local deterministic fixture, including supported signed C→T. |
| J1-BOUNDARIES | Independently cover multi-event action, business rejection, explicit/preallocated ID, rollover and checkpoint pending IDs; no lost/double cursor advancement. | [Numeric validation](runtime/runtime-numeric-safety.design.md#numeric-action-allocator-validation); required targeted cases. |
| J1-REPAIR | Rebuild from source journal, save, load again and submit a legal action; cursor and next outcome remain equal to control. | [Repair source-journal path](../../crates/oasis7_ops_tools/src/bin/oasis7_world_repair_rebuild.rs); no generated-bootstrap claim. |
| J2-WAKE/CLEAR | Real WASM returns WakeAfterTicks, Suspend and omitted directive in separate runs; compare restored schedule and subsequent calls/state/fees. | [Tick regressions](../../crates/oasis7_runtime/src/tests/module_tick_routing_transaction_regressions.rs) and supported signed C→T fixture; retain post-route checkpoint rejection/roundtrip coverage. |
| J2-INSTANCE | Cover Pure/no-new-state success and two instances with only one due; only the correct instance runs at the expected next tick. | [Host tick verification](runtime/runtime-integration.md#module-tick-journal-replay-validation); required targeted cases. |
| J-COMPAT/ATOMIC | Exercise each compatibility-matrix row, incomplete groups and existing precommit publication failures; replay guest/dispatcher invocation count is zero and no new charge is computed. Durable publication retains the G1 outcome classification. | Existing persistence/rollback/tick negatives plus new record cases; preserve event/intent allocator and signature/artifact/finality rejection assertions. |

Actual candidate, commands, per-obligation outcomes and unexecuted cases belong in the PR/test results. Documentation checks establish these mappings only, not runtime recovery or release readiness.

<a id="runtime-generation-persistence-design"></a>

**普通 indexed generation 保存合同（目标）**

本条承接 [Runtime 持久化要求](prd.md#runtime-generation-persistence-contract)，
细化已有 generation 提交点在普通 `World` 保存及 reserve/settle/release、余额更新等
附着持久化目录的事务中的含义。owner 为 runtime_engineer；审读基线为
`eng-cc/oasis7@27db6f894bd106e9a4fee9ee3062cd8fd86e1056`，缺陷入口为
[#4452](https://github.com/eng-cc/oasis7/issues/4452)。现有 staged command、authoritative
recovery 和认知生命周期的局部合同不证明这些普通入口已经覆盖；总体仍按 §6.2.4 标为 partial。

持久化调用必须把本次精确候选与原业务返回值一起保留，按
[G1 提交判定与安装](runtime/runtime-storage-footprint-governance.design.md#storage-generation-g1-design)
返回或传播 `NotCommitted`、`Committed`、`StatusUnknown` 的含义。只有确认未提交才允许
丢弃候选并继续使用旧 World；确认提交后安装同一候选并返回原 lease/receipt，维护错误单独报告；
未知时在普通持久化发布边界阻止旧 World 的后续写入，核对完成后才解除。沿用已有业务幂等 key，
同 key 重试不得新增扣账、lease 或 receipt；generation identity 不替代业务幂等 identity。

[G2 保留闭包](runtime/runtime-storage-footprint-governance.design.md#storage-generation-g2-design)
保护替代 index 提交前旧 index 的全部恢复依赖；
[G3 完整候选读取](runtime/runtime-storage-footprint-governance.design.md#storage-generation-g3-design)
让 snapshot/journal 组装和同代 metadata 校验共同决定候选是否可用；涉及事件后缀时还须满足
[J1/J2 控制状态完整性](#runtime-journal-suffix-recovery-design)，不能以材料可读代签重建，也不据此推断 G1 提交事实。
[提交核对与回退选择](runtime/runtime-storage-footprint-governance.design.md#storage-generation-cross-boundary-design)
分别回答是否提交、何者可读；后者不能代替前者，也不能绕过
[同世界恢复服务闸门](#runtime-recovery-design)。这只细化本地 single-writer generation seam，
复用既有格式、root 和 operation key，不新增正式 authority、业务 ledger 或网络持久性承诺。
具体验证见 [G1/G2/G3 及交叉矩阵](runtime/runtime-storage-footprint-governance.design.md#storage-generation-validation-matrix)。

#### 6.2.4 Incremental delivery and proof levels

Implementation proceeds with Phase 0 plus five reviewable migration phases: (0) transaction/delta types,
savepoints, deterministic projection, failure injection, and a test-only
clone-backed oracle; (1) `append_event`, reducer, sequence, journal, schedule,
queue, commitment, consensus, and deterministic-metric staging; (2) step,
action, direct/trusted command, module lifecycle/routing/tick, observation, and
capability paths; (3) effects, receipts, durable outbox, and idempotency; (4)
atomic replay/restore; and (5) generation persistence and crash recovery.
Production clone-and-publish is removed only for a path whose replacement has
passed its failure-injection and replay gates.

Capability is `design-ready` after this contract and its test oracle are
frozen, remains `partial` while any public mutation entrypoint uses a legacy
boundary, becomes `target-implemented` only after every entrypoint is migrated,
and becomes `proven` only after execution, rejection, fault, replay, recovery,
idempotency, outbox, persistence, serde, and WASM compatibility evidence all
pass. No single staged step test or in-memory swap proves general atomicity.

<a id="module-instance-command-design"></a>
### 6.3 Command-path module-instance completeness

Instance identity is an authorization and addressing key, not merely a
persistence field. The stable logical key is `(world_id, module_id,
instance_id)`; `artifact_hash/schema_version/activation_epoch` is a separately
versioned binding at an execution height. Install, upgrade, tick, event routing,
and restore already preserve part of this identity; direct/trusted command lookup,
state updates, emitted events, receipt linkage, and machine descriptors remain
`partial` until every path carries the same target.

The target command path validates the logical key and active artifact/schema/
activation binding, owner/subject, capability, and budget before fixing the
instance target and binding digest into the staged command. Upgrades append a
version-lineage record keyed by execution height; restore/replay uses the
historical binding rather than today's artifact. State lookup, event, receipt,
snapshot, replay, and descriptor projection then use that fixed target. Missing,
conflicting, inactive, unauthorized, or state-mismatched targets fail closed
before the first effect; there is no global `module_id` fallback. Agent-facing
semantic interpretation and tool policy remain Agent/WASM authority; this
document only requires runtime to expose a verifiable instance-bound machine
descriptor input.

#### Compatibility routing and validation plan

This design accepts the instance-isolation obligation in
[REQ-DWE-001](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)
and [AC-DWE-001](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-001),
through the [runtime command-path contract](prd.md#command-path-module-instance-completeness).
The following rules are targets for [#3555](https://github.com/eng-cc/oasis7/issues/3555),
not a claim that the current command paths already enforce them.

1. Both direct and trusted entrypoints resolve the target against the same active
   registry and instance view before invoking the sandbox. An explicit target
   must match its logical identity, active binding and authorization audience.
   A legacy module-id-only call resolves to the sole active instance when one
   exists and rejects multiple active instances before execution. The existing
   manifest-only singleton remains an explicit legacy compatibility case when
   no persisted instance exists: a named compatibility adapter validates its
   sole active manifest and existing legacy state binding, then freezes that
   legacy target. It must not select this route after an explicit-instance
   lookup or authorization failure, or when any instance makes the target
   ambiguous. Missing active bindings are rejected. This preserves old singleton
   callers without claiming that the legacy state shape is instance-complete.
2. Freeze the resolved instance state key and binding in the command stage.
   State lookup, output application and emitted events use that target. Trusted
   audience validation and execution cannot select different instances; a
   missing, stale or mismatched binding is rejected before the first effect.
3. Preserve the existing staged publication boundary for business state, events,
   resource effects and receipts. Failed commands discard these staged business
   projections; target-resolution and ambiguity rejection leave the journal
   unchanged. Existing sandbox failures retain their established disposition:
   where the entrypoint records `ModuleCallFailed`, it atomically records that
   audit once after discarding the business stage, without adding a second
   failure event. Reuse the existing instance-key and event/replay conventions; this
   bounded routing change must preserve the `wasm-1` wire ABI and persisted
   snapshot shape. It does not complete the broader receipt/catalog lineage or
   whole-runtime transaction migration by itself.

The implementation should extend the existing runtime test modules selected by
[testing-manual.md](../../testing-manual.md). Relevant current locations are
`crates/oasis7_runtime/src/tests/module_command_provenance.rs`,
`crates/oasis7_runtime/src/world/module_runtime_tests.rs` and the
`crates/oasis7_runtime/src/tests/capability_grant_v2.rs`,
`capability_grant_v2_regressions.rs` and `capability_grant_v2_recovery_regressions.rs`
tests in that same runtime test directory. Their
existence is not evidence that the following target cases already pass.

| Target case | Required input and assertion | Acceptance boundary |
| --- | --- | --- |
| Direct two-instance command | Same artifact, two active instances with distinct state; target each instance in turn and assert only its state and emitted events change. | No cross-instance write or event attribution. |
| Trusted target and audience | Two active instances; execute matching authorized commands against each in turn and assert only its state and emitted events change. Also exercise mismatched command/authorization targets and reject them before sandbox effects. | Authorization and execution bind the same instance, with no cross-instance writes. |
| Legacy ambiguity and missing target | Call the module-id-only entrypoint with two active instances, or with neither an active instance nor a valid legacy singleton binding; assert rejection with unchanged state, resources, journal and receipts. | No silent target substitution or partial publication. |
| Single-instance compatibility | Resolve one active instance, then separately exercise the existing manifest-only singleton through its explicit legacy adapter; compare established command results, output bytes and business effects. | Unique resolution and preserved legacy singleton behavior; no new snapshot fields. |
| Failure audit and replay compatibility | Exercise an existing sandbox failure and assert zero published business delta and exactly the established failure audit, then round-trip existing snapshot/wire fixtures and replay successful instance updates; repeat target rejection after restore. | Existing error disposition and formats remain readable; target attribution survives replay and broader migration proof remains separate. |

Run the affected direct/trusted and restore/replay regressions on the implementation
candidate, preserving the existing error-order and transaction tests. Record the
actual commands and results in its PR; documentation-only checks do not establish
instance isolation or release readiness.


### 6.4 Gradual subsystem state encapsulation

State ownership is split by stable boundaries without changing the canonical
timeline in one step. The implementation sequence is:

1. Name a subsystem owner and its staged read/write surface while retaining the
   existing `WorldState` compatibility representation.
2. Move module instance state and its journal/replay anchors behind that
   surface, then apply the same pattern to jobs, schedules, and other bounded
   subsystems only after focused determinism and persistence fixtures pass.
3. Persist keys with world/instance/schema identity and keep legacy adapters
   explicit; no subsystem may silently reconstruct state from process-local
   cache.
4. Measure replay, recovery, footprint, and long-run behavior at each step;
   rollback means reverting the bounded adapter/migration, not rewriting
   historical receipts.

This gradual path preserves fixed execution order and snapshot compatibility.
ECS, sharding, and dynamic database work may be future implementation options,
but are deliberately deferred and are not runtime promises in the current
design.

## 7. 集成点
- `doc/p2p/prd.md`
- `doc/world-simulator/prd.md`
- `doc/testing/prd.md`

## 8. 专题导航
- 核心治理进入 `governance/`
- 模块发布与实体进入 `module/`
- 运行时行为进入 `runtime/`
- 执行器与 ABI 进入 `wasm/`

## 9. ModuleStore persistence / restore boundary

- The default world directory owns one module-store closure: registry, manifest metadata, and content-addressed wasm artifacts. `save_to_dir` / `load_from_dir` are the normal route; compatibility `*_with_modules` entrypoints do not establish a parallel format.
- Restore accepts legacy directories without a store; an existing store validates registry/manifest/content identity and reports structured corruption or missing-dependency errors. The [retirement/shared-content design](module/module-lifecycle.md#module-retirement-and-shared-content) adds verifiable retired-state handling and preserves distinct manifests sharing H; these targets are not satisfied by ignoring missing bytes or using one H-keyed metadata slot for every manifest.
- Recoverable action retirement requires the [versioned journal fact](module/module-lifecycle.md#module-artifact-retirement-journal) to commit atomically with its matching destroy event and sidecar transition. Pre-destroy-checkpoint plus successful-action-tail recovery must rebuild the same retired closure as a post-destroy checkpoint, without repeating fees; legacy raw destroy replay remains state-only. This writer/replay-reader extension is a target, not an existing `ModuleArtifactDestroyed` guarantee.
- `load_module_store_from_dir` uses a sparse prepared replacement: it loads and validates every sorted registry record into owned registry/set/byte values, then installs those three values once. No `World` clone is required, validation error priority is unchanged, failed hydration preserves the live cache and all world authority, and successful hydration preserves the existing cache policy.
- Raw logistics topology, direct transfer, and material-transit events use separate prepared transition modules. Start and completion preserve the reducer's validation order while staging only touched ledgers, route reservations, pending/settled identifiers, path authority, receipts, affected agents, and progress. The borrowed state projection includes the one generic actor mailbox route before hashing; installation is infallible and replay invokes the same transition helpers. A settled duplicate therefore remains a business-state no-op while its published root still commits the one routed mailbox event.
- Raw factory and recipe lifecycle events use separate sparse prepared transitions. The recipe transition stages only touched pending/settled jobs, factory state, material and power ledgers, logistics paths, actor activity, progress, and quote sinks. Replay shares the same pure preparation and infallible install path; idempotent completion and terminal validation-block events preserve business state while the prospective root still includes the single generic actor mailbox route.
- Raw core agent/body, observation, gameplay-policy, and material-profile events use a full-event-bound sparse transition over one touched agent, compatibility world materials, optional policy/progress, and one optional profile. Preparation owns all fallible body-item and profile validation; installation is infallible and replay shares the same transition. `ActionRejected` remains a no-state, no-route event, while every routed success is included once in the prospective agent mailbox root.
- The canonical append seam is fail closed: every world-event body must resolve to an explicit prepared classification before any reducer, event allocator, journal, retention, backpressure, or consensus work. Intentional no-state deltas retain and compare the complete body, and the former unbound generic route-only delta has been removed. This is an exhaustive publication-classification boundary, not a claim that every outer multi-event workflow is one transaction.
- Factory lifecycle publication uses a separate prepared transition with optional sparse factory upsert/deletion, build-job replacement/removal, settled and retired identity insertion, touched ledgers, affected actor, and projected progress. Build completion and retired recycle duplicates retain raw idempotency while prospective roots include their one declared mailbox route; maintenance/recycle validation and material preflight finish before any prepared state is installed.
- Raw single-event `ModuleEvent` publication reuses the governed lifecycle projector against owned registry/artifact/schedule values and records targeted subscription-cache invalidations; `ManifestUpdated` prepares the replacement manifest and its state-root projection. Both pass allocator, journal retention, consensus validation, and the post-prepare failure seam before an infallible install. Missing-record activation errors no longer leak an active mapping, while legacy informational fields and overwrite behavior remain unchanged.
- Restricted-admin registry updates and validator admission submit/approve/activate/revoke events use a full-body-bound prepared governance delta. Preparation preserves legacy validation priority and synthetic revocation behavior, and a borrowed state serializer overlays only the four touched registry/admission/identity fields when deriving the prospective consensus root; installation is infallible and does not clone `WorldState`.
- Raw proposal status events use one full-body-bound sidecar delta and shared pure status projectors. Because proposals remain outside canonical `WorldState`, their prospective consensus root is unchanged; the normal Approved-to-Queued pair and the governed module/manifest/Applied batch retain their dedicated multi-event publication paths.
- Raw resource/data/access events use a dedicated sparse projector. Validation reads canonical state, stages only touched agents plus one permission-owner or authenticated-nonce player entry, and projects compatibility material normalization without cloning `WorldState`. Root derivation overlays the routed AgentCells while installation remains unrouted, allowing the existing publication/replay router to deliver each mailbox event exactly once.
- Raw power redemption outcomes use a separate full-event-bound projector. A redeemed outcome stages one node balance, the reserve, one nonce and touched AgentCells; a rejected outcome stages only optional activity. The borrowed root overlays the target-only route and compatibility materials, while replay and publication share the same unrouted infallible install.
- Raw node-points settlement uses a dedicated full-event-bound compound projector. It preserves settlement, mint-policy, epoch-budget, bridge-distribution, treasury, account and supply validation order while staging sparse node/account/treasury/budget/bridge replacements plus the full append-only mint list and supply value. Publication computes the prospective root from borrowed untouched fields, crosses the failure seam, then installs infallibly; replay and action preview share preparation, and the event intentionally has no mailbox route.
- The five raw main-token monetary core events share a full-event-bound sparse projector while retaining separate reducer semantics. Genesis may replace the empty account/bucket maps; vesting and transfer touch only their accounts, bucket and nonce; epoch issuance touches supply, its issuance record and treasury entries, while fee settlement touches supply and its treasury entry without changing issuance records. Root serialization preserves canonical field order and projects only the existing beneficiary/source route, while installation stays unrouted and replay/action previews reuse the same preparation.
- Policy scheduling and treasury distribution use a separate full-event-bound governance-monetary projector. It stages scheduled-policy or treasury/account/supply/distribution-record updates with compatibility materials at their canonical root fields, installs without routing, and is reused by replay and action previews.
- Restricted starter-claim publication uses a dedicated full-event-bound projector for pool top-up and grant issue/expiry/revoke. It stages sparse treasury, beneficiary balance, supply, grant/top-up record and compatibility-material updates, projects the existing controller/issuer route exactly once, installs unrouted, and shares preparation with replay and action previews.
- Raw starter OC claim publication uses a standalone full-event-bound projector. It stages the claim, account, supply, optional treasury entry, target activity, and compatibility materials; the borrowed prospective root includes the existing raw-agent route exactly once, while publication and replay share the same unrouted infallible install.
- Claim release requests, grace entries, and idle warnings share a lightweight full-event-bound projector. It stages one claim plus release-only claimer activity and compatibility materials, projects the existing claimer route once, and leaves target activity, monetary state, and the outer epoch cursor untouched; release action preview reuses pure preparation.
- Agent claim creation and upkeep settlement share a full-event-bound economic projector. It stages the claim and processed-epoch cursor together with the claimer account, supply, sparse treasury, claimer activity, and compatibility materials; the borrowed root projects the existing claimer-only route once, while publication installs the unrouted delta infallibly and replay and claim-action preview reuse the same preparation.
- Claim release and reclaim terminal outcomes use a separate full-event-bound projector. It validates refund provenance and destination before staging claim deletion, sparse account/treasury/supply changes, the processed-epoch cursor, claimer activity, and compatibility materials. The prospective root includes the claimer route exactly once; tick event derivation remains sequential and raw reclaim reasons remain informational.
- Economic-contract open, accept, settle, and expire events share a full-event-bound sparse projector. It preserves raw overwrite and payload authority while staging the contract, overlapping participant AgentCells, world treasury resources, reputation, pair cooldown, reward windows, and compatibility materials. Action preflight and replay reuse preparation; expiry remains contract-id ordered and event-sequential, and the existing raw actor alone receives the routed event.
- The six simple capability-admission events reuse the batch projector through a standard full-body-bound publication delta containing grants, revocation/identity state, invocation contexts, budget accounts, and the derived authorization root. Grant projection shares the replay reducer's body, issuer, revocation, parent-chain, and immutability validation order. Compound command and effect-receipt events are deliberately excluded because their atomic boundaries include world state, journal-head, and effect queues.
- Module instance identity/version/hash, owner, install target, active state, and install time are persisted for replay routing. The [instance-upgrade design](module/module-lifecycle.md#instance-upgrade-version-reuse) separates exact registered-target reuse from instance migration and validates the source against that instance. Existing compatibility/prepared-publication slices do not prove rolling A/B upgrades; [independent recovery cases](module/module-lifecycle.md#module-lifecycle-recovery-validation) remain required.
- Gameplay validation, active-slot planning, readiness and restored projections share the [derived mode key](module/module-lifecycle.md#gameplay-mode-key-design), without rewriting raw manifest bytes or their identity. This closes a target consistency gap, not a claim that current raw-string comparisons are already unified.
- Runtime anchors are `crates/oasis7_runtime/src/module_store.rs`, `runtime/error.rs`, `runtime/tests/persistence.rs`, and `runtime/world/module_store_load_transaction_regressions.rs`.

## 设计目标

### Chain Observer 本地只读加载

launcher 的 `--chain-execution-world-dir` 固定本地 execution root；托管 chain 的 writer 和 Viewer 使用同一解析结果，未指定时保持 `output/chain-runtime/<node_id>/reward-runtime-execution-world`。独立 `oasis7_viewer_live --chain-status-bind` 必须同时指定该本地目录，远端 status 的 `execution_world_dir` 不参与任何文件访问。

Observer 只接收带不可变 generation index 的 checkpoint。capability reader 对中间目录逐组件 no-follow、对末文件只读 no-follow，并检查普通文件、输入、解码、累计、单缓冲分配和元素预算。捕获的 generation 固定 manifest、segments、CAS archive 与恢复元数据哈希；writer 发布新代不会使仍然完整有效的旧捕获失效。缺少可验证 checkpoint 时等待 writer，禁止回退到无绑定的多文件加载。

候选从一开始不附加 persistence，不写审计、缓存、GC 或索引，不执行 journal 尾部恢复或裁剪队列。cognition 必须修改才能恢复时返回 `RecoveryRequired`；经济配置保持 checkpoint 原值。每个 Viewer 最多一个加载 worker 和一个最新待处理目标；source epoch、逐块取消检查与安装前来源/水位检查阻止旧来源结果安装。加载失败保留已有展示，同一水位不同权威内容被拒绝。

部署目录仍由受信 writer 管理；目录能力和 no-follow 不提供对具有本地写权限的恶意进程、hardlink 或原地内容修改的隔离。默认输入单文件为 64 MiB、单 blob/缓冲为 128 MiB、累计读取/解码/组装为 512 MiB；预算不足拒收而不修改世界。

- 提供 `world-runtime` 模块的总体设计入口，并明确其在 world-infrastructure 中的确定性执行责任、版本化协议与可恢复性边界。

## 设计范围

- 覆盖模块级结构、主链路、分层、target/current gap 与专题导航。
- 不替代 `doc/p2p/` 的 consensus/topology/finality authority、专题 `*.design.md` 的细化设计，或产品规则与客户端交互定义。

## 关键接口 / 入口
- 需求入口：`doc/world-runtime/prd.md`
- 当前任务入口：对应 GitHub task issue / Project
- 索引入口：`doc/world-runtime/prd.index.md`

## 设计演进计划
- 历史建档（2026-03-09，ROUND-006）：模块级 `design.md` 入口来源；原 Task 未唯一定位，记录保留，不作为当前排期。
- 持续结构要求：高复杂度专题在对应 `*.design.md` 维护细化合同；不恢复旧 M2 任务。

Alliance/war raw publication uses a full-body-bound sparse projector. It prepares touched
alliances, wars, participants, reputation, and compatibility materials, derives the canonical
root through borrowed untouched state plus the existing route, and installs only after the
publication failure boundary. Replay and action preflight reuse the same reducer semantics.

Governance proposal, crisis, meta-progress, and product-validation events use four narrow
prepared projections behind one publication dispatch type. Each stages only its touched records
and AgentCells; borrowed canonical serialization adds the existing route before the failure seam,
while installation remains unrouted and replay shares the same raw reducer semantics.

## 设计风险
- 若专题级设计未及时补齐，模块级 `design.md` 可能承载过多导航职责。
- 若 legacy redirect 未明确标注为兼容跳转，读者可能误判历史入口为当前执行入口。

## 基础设施设计承接与验证视图

本视图补充既有 §§3–6/9，保留其边界、阶段、接口、错误优先级、兼容与原子/非原子区别。owner runtime_engineer；原视图输入审读基线为 eng-cc/oasis7@f9d5a552d9af04c1b1398262808198a58e560230，本次普通 generation 增补与失败语义修订采用[保存合同](#runtime-generation-persistence-design)所列基线。这里只固定目标设计和验证要求；现有实现仍由 §5/§6.2.4 的 partial 和具体入口限定。它不定义 BFT 算法、外部 DTO、工业 schema、成本/容量数值、UX 或 release authority。外部 P2P 的未合并草案不是本设计输入。

<a id="runtime-deterministic-design"></a>
### 确定性执行与证书绑定
输入 producer 是已认证的 canonical committed context：world/parent/ordered action、action root、governing manifest 与适用 proof；proposer output 仅候选。runtime 从同 parent 准备执行结果并绑定 execution hash/state root/receipt-journal，P2P 验证同 world、合法历史 authority profile/epoch 与适用提交证明；`bft` 额外验证 validator epoch/threshold/round certificate，每个 active validator 在 attestation 前重执行并比较。缺 proof/artifact、root mismatch、越权、超限或输出 fault 不能 vote/commit 或本地修补越过。§6.2.1 buffer 统一暂存业务状态；§6.2.2 rejected/faulted 不冒充成功或业务可继续。in-process 和未来 IPC 使用相同 conformance/replay 输入，transport 不授予额外权威。已有 bridge/local tests 仅证明各自有界场景，完整所有活动验证者及 BFT 是 target。

<a id="runtime-pending-design"></a>
### 待决持久化与恢复重审
提交接收与 canonical execution 是两个状态边界。无 finality 的 signed intent 必须由 runtime/P2P 批准的持久化 owner 保留 identity、原 payload、关联和 truthful no-effect disposition；字段/schema 尚待专业合同批准，不能用 effect queue/cognition journal 冒充通用实现。入队/重连/status read 不扣资源、不授予资格、不推进工业/W 或后续依赖。恢复必须先通过服务闸门，再从 fresh canonical snapshot 读取当前权限、资源、期限、manifest 和前置条件，在 canonical 顺序重审，原子产生生效 receipt 或无效果拒绝/过期/重规划处置。crash 前后不能静默丢失、结算或改变原请求；pending 持久状态、winner 和 receipt 应落在 §6.2.3 同一完整 generation/历史边界，不能拼接混合 generations。消费者读取 committed receipt 与 truthful pending，无完成时间或退款新承诺；withdrawal/replacement 不获单方面取消权。

上述“无效果、恢复后当前条件重审”只适用于尚无固定 execution continuation 且已核实无效果
的 pending；没有 finality 回应或原回应丢失不构成该事实。Unknown 先核对原持久记录，
不得释放原 nonce 或据此新开动作。已固定原决定的执行中崩溃，适用下节的原输入恢复。

<a id="runtime-controlled-candidate-recovery-receiver"></a>
### 受控权威原决定恢复的接收边界（目标）

Runtime 接收 [P2P 原 candidate 恢复 v1](../p2p/design.md#des-p2p-original-candidate-recovery-v1)：
精确原 Snapshot/Journal、签名输入、nonce/reservation、已决定 context、时间/随机输入及
native/WASM/规则工件必须在执行前形成双端持久闭包。恢复先处理已决定、qualified 或
published 事实；只有缺原 candidate 且闭包完整时，才在 detached World 以相同原 parent、
版本和决定输入重建。原 admission 及 reservation 不在当前 snapshot 上重算，也不再次扣除。
缺材料、冲突 root 或旧工件不可执行保留原身份并 `recovery_required`。

本目标中的 candidate 重建可调用精确原版本的纯执行器/WASM；它和 §6.2.3 的 canonical
history replay 分属不同阶段。后者继续只应用已提交历史，前者只构造未发布的内存候选；
二者都不得新调 LLM、读取外部实时条件或触发 dispatcher。重建的业务状态、结果、nonce、
里程碑和 outbox 沿一个完整 generation 发布，外部派发仍由提交后的既有幂等合同主责。
已经固定的 continuation 必须在后继版本/epoch activation 前收敛，不被 activation 重新选版。

正式 authority、控制记录类型及五路 API 的 proof/version 规则由
[P2P 正式受控权威 v1](../p2p/design.md#des-p2p-formal-controlled-authority-v1) 主责。
Runtime 仅从验证后的 ActiveAuthority 和同记录 proof 构造正式 CommitRef/可发布结果；
工程 scope、配置、WorldService 服务签名和本地重执行成功都不能代替正式证明。
现有 guarded-local 在完整 candidate 未落盘时仍停写；本节是尚未实施的接收目标，
不把原工程回归、普通 v1 客户端或部署状态升级为正式能力。

<a id="runtime-lineage-design"></a>
### 互斥胜者与原子处置
专业域负责标记同 lineage 的互斥成员及允许的 withdrawal/replacement；runtime 不从相同 payload、ActionId 或 retry 推断互斥。canonical ordering 后在一个 root transaction 内检查尚无 effect-bearing winner、当前成员有效，暂存业务效果、winner、其 receipt 和其余成员终止处置，全部 prepare 成功后统一 publish；没有 inner commit 或业务效果先行的 loser repair。若首个成员仅被 rejected/expired，终止它自身，不占 effect winner，不取消其他/独立请求。失败/commit uncertainty 服从 §6.2.2；retry/replay 读取原 winner/disposition，不产生新 effect/receipt。持久重启必须恢复一致 winner+effect+loser 状态，否则 fail closed。既有单操作 idempotency/签名验证是必要子能力，不能代替此目标仲裁；具体标记/schema/排序依赖 P2P、domain owner 与 QA 评审。

<a id="runtime-version-design"></a>
### Manifest activation 与历史回放
治理负责 canonical activation boundary，P2P 提供 finalized block/activation proof，WASM 提供 admissible ABI/artifact/schema。节点预置兼容工件后只证明可用性。首次已 committed/finality-verified execution block 按 boundary 前/at/after 决定 governing version；candidate 或 client compatibility declaration 仅非权威兼容输入，不绑定世界规则。执行按该版本当前权限/资源/前置条件重新 preflight 和 prepare；激活前 pending 在新版本下不能继承旧 quote/资格，不能静默翻译 payload、先应用旧部分再套新规则。不兼容原子拒绝/过期，主体可明确重规划新兼容请求；linked replacement 仍走互斥 winner，独立未关联请求需要专业确认。
执行与 receipt 的历史 binding 必须可回读其 block manifest/artifact；保留高度的依赖由 storage pin/replay 合同保护。历史 replay 永远按原 binding，不用当前 manifest/price 改写旧成本、权限、资源、责任；缺失/冲突 activation proof、artifact 或 compatibility 同时阻断执行与恢复。§6.3 instance history 与 §6.2.3 replay/generation 是承载边界，完整 activation ledger/readiness/rollback 仍未证明。四条 release lane 保持独立，协议/不兼容 host ABI 变动要协调升级/fork 与迁移证据。

这里“激活前 pending 在新版本重审”同样限于未固定 execution continuation 的请求；
已固定原决定的重建沿 [原决定接收边界](#runtime-controlled-candidate-recovery-receiver)，
不能借 activation 变更原执行版本、nonce 或结果。

<a id="runtime-recovery-design"></a>
### 同世界恢复与服务闸门
恢复输入链逐环验证固定 world/chain/genesis 身份、各高度合法 profile/epoch/manifest 激活链、同 world 已确认 checkpoint 与 authority transition、hash snapshot、canonical committed log/replay、root；BFT 历史另验证当时的 active registry 与 certificate。snapshot/DistFS/CAS 是材料，不能独立赋 finality。错误 world、缺失或矛盾材料保留原历史供诊断，进入 unavailable/isolation，无新权威效果；不得换 endpoint/缓存/local 世界伪造连续恢复或静默处理未 final 请求。
历史链通过仅足以进入 verified readonly 的候选。开放 serving 必须同窗口证明 append/finality/versioned execution compatibility/monotonic head continuity 全部成立；stale/catching-up 或 unavailable 不开放写入。每个闸门失败时的新 intent 原子拒绝或无效果 pending，committed receipt=0；开放后按当前条件和 canonical 顺序重审既有 pending，不继承停机期限/优先级，每个被接受新 intent receipt≤1 且无第二效果。提交/恢复过程中任一 guard 回退则停止新世界效果、回 readonly/isolation，已 confirmed receipt/root 不撤销/重放/改写。Agent/Viewer/API 投影等级、主要 blocker、下一步；scope/状态 DTO 依赖消费者 authority。
验证采用 [state-sync 执行 lane](../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md) 与 [现有证据 envelope](../testing/templates/state-sync-closure-evidence-packet-template.md)。同候选/同窗口 ops topology/inventory/health/status/peer-head/state-sync/restore 事实必须与 runtime guard、QA 判定关联；覆盖状态转换、manifest/head 负例、逐 intent receipt0/1、blocker/next step 的结构化 attachment/schema 尚须 owner 批准并提供，缺失阻断完整服务验收，本设计不伪造该 schema。

<a id="runtime-industrial-design"></a>
### 工业因果、容量与背压
代表性流水线的 immutable root/revision/parent/child/stage/edge/batch 与目的地身份在每个首个不可逆 sink 前从 fresh authority 校验；错误/缺失不扣 input、不建立非法 hold/child。accepted 是 intent admission，reservation 必须另经 domain capacity/资源条件，在同事务中绑定有界独占 hold；每条中间 edge/destination buffer 满载，只保留未消费 input、接收仍有容量的已结算 output 或原子拒绝/延期新的承诺。runtime 不选择无限缓存、丢弃、瞬移、隐式改道或伪造 terminal。
同一 release/arrival event+hold 的 release 与 fresh-snapshot recheck 各一次；重复 arrival/retry 读原 disposition；后续不同 event 只重审仍有效的 unmet residual。input join、output branch 和 production-versus-delivery settlement 按 M4/domain contract；因果改变才建立 parent-linked revision/child root，checkpoint/retry 不新建 root。hold/consumption/receipt/remaining 必须持久并进入 replay/root 比较，不能只在 UI/log。§工业矩阵当前尚无通用 root/join/window/bundle identity，具体字段/算法未获批准；现有 path reservations 和 job replay 是 partial 子能力。domain owner 拥有容量、lease/window、W reset；runtime 执行原子处置，消费者显示 earliest blocker、held/consumed/unmet/residual 与 recheck。

#### 首产物提交来源 schema 边界


新 v2、origin-bearing 首产物结算还把已验证 pending 的批次、投入材料、配方用电需求、产出与 requester 复制到既有 milestone 的可选 settlement summary；不从当前余额或可变配方估计首产物成本。该 summary 随 milestone 保留，独立于 bounded receipt pruning。旧 origin=None job 与旧 milestone 不反填，缺省字段不序列化，保留旧重放 state-root；新生产者和执行器仍须同候选升级。Viewer 的既有 starter profile 仅向当前控制的、factory builder/site owner/output ledger 一致的 Agent 发布已结算 outcome；界面分别显示 production_only 首产物、Assembler 候选资格和仍未证明的稳定/交付，不把配方用电需求称为现金扣费。

纯 API 的 chain 提交 ack 中 `runtime_action_id` 是提交侧 consensus action ID，不等于执行世界重新分配的 recipe job ID。新 `ScheduleRecipe` 提交使用 consensus payload envelope v2：来源身份只取已通过既有 gameplay 签名和持久 nonce 校验的玩家、公钥及 nonce；ack 的 `consensus_action_payload_hash` 取实际提交 CBOR 字节的哈希。执行桥从已验证 committed action 取得同一 payload hash、consensus ID、node transport submitter、committed height 与 action root，将其作为 `committed_recipe_origin` 沿 pending action、RecipeStarted、pending recipe、RecipeCompleted、完成 receipt 和首产 milestone 持久保留。browser 玩家和 node transport submitter 是两个事实，来源字段不授予权限。

v1 无来源 envelope 和旧 snapshot 的可选来源默认 `None`，序列化省略该字段，不对历史重放反填来源；v2 必须有严格匹配的来源，缺失、null、畸形或非 ScheduleRecipe 来源拒绝。旧 v1 执行器会明确拒绝 v2，不可混跑后声称相同 state root。启用新提交入口前须将 producer 与执行器升级到同一候选版本；本地验收使用全新独立 world，不向已存在 world 自动启用新 profile。完成事件须与 pending 来源全等；receipt 和 milestone 从 pending 复制来源，receipt history 清理不得抹掉 milestone 的来源。没有原来源的历史结算仍可读取，但不能由严格首产物 gate 认定为新提交的精确关联证据。

该字段证明提交与执行任务的因果关联，不能代签正式网络 finality、完整持久 commit 合同、持续稳定生产、终端交付或真人验证。具体候选、运行、结果与未证明范围由同 source 的执行 evidence 单独记录。

<a id="runtime-industrial-outage-design"></a>
### 四边界成对 outage 处置
每个 cell 固定同 world/root/revision/child/stage/edge/batch，requested/committed/executed/held/consumed/unmet/residual quantities、canonical bucket、window/lease、receipt、W/progression、next action/recheck。以下八格独立判定，不能以一条 outage 测试合并：
| 边界 | 权威 finality/append/execution/industrial service outage | 非权威 Viewer/API/hosting/read outage |
| --- | --- | --- |
| stage_finish | A-SF：pre-finality 零新效果；已 committed root 保留 stage/input/WIP；fresh snapshot 后单处置 | B-SF：世界继续，读取 stale/unknown 后仅对账实际 stage result |
| transit | A-TR：零新 transit credit；已 debit/WIP/in-transit/arrived/receipt 保留，禁止第二 child/到达 | B-TR：不改运输量/root/receipt；重连读取实际 committed 到达 |
| buffer_admission | A-BA：无非法 hold/credit，已 hold/remaining 保留，fresh recheck 服从容量 | B-BA：不改 buffer/reservation，显示 stale 后对账权威 bucket |
| terminal_settlement | A-TS：无新 terminal/reward；已 committed receipt/milestone 保留，恢复至多一次 settlement | B-TS：不 reset W/renew lease/重复 reward；呈现 outage 期间实际完成结果 |
A 每格都测提交前无新 root/hold/sink/WIP/credit/output/qualification/W；已 committed root 至下一 child 前保留 root/window/lease/input/WIP/transit-arrived/receipt，不新 child/credit/隐式续租/优先级刷新/reward。恢复仅一次 fresh-snapshot continuation/hold/defer/reject/expire/compensation；只有 gameplay canonical interruption 允许当前未完成 candidate 的 W reset 一次，历史 milestones/receipts 不变，因果改变才 child revision。B 每格 injection 自身不改任何 world/root/quantity/window/lease/W/receipt/progression，也不制造 outage disposition；若权威已完成，只 reconcile 真实结果。duplicate arrival/submit、retry/reconnect/restore/replay 不重复 hold release/sink/credit/receipt/W/reward。
每格 Agent/Viewer/pure API 比较 state class/blocker、lineage/quantities、receipt/effect、W/progression/next action/recheck。required 是八格 deterministic protocol matrix + active-LLM/provider-backed pure API parity；full 另需真实本地 stack/provider、external headed S6/Playwright desktop+narrow screenshot/console 与 provider-backed Agent parity，记录 decision source/backend/contract/transport。provider_local_mock 仅 plumbing/fixture；现有历史 parity evidence 不关闭新八格同候选验收。

<a id="runtime-world-scope-design"></a>
### 跨世界提交与 receipt/replay 边界
admission 前验证 target world 身份，receipt 与 parent/history/result 保持该 world，local/development 与 global 使用 distinct identity。缺失或不匹配 fail closed；local receipt reuse、错误世界 restore/replay 不创建 global effect，consumer 也不得呈现为 global。消费者在提交前表明 scope，表达形态由 consumer owner；替代世界/迁移另行产品决策。现有 bridge 错 world restore 是局部入口，未证明全部 consumer scope。

### 状态、事务与持久化的独立边界
语义状态包括 no-effect pending、有效 effect winner/terminal disposition、版本历史 binding 及 readonly/serving/isolation；这是接受语义而非新增 wire enum。事务边界复用 §6.2.1 root buffer/prepare/infallible install/无 inner commit，将业务效果及适用 winner/receipt/loser 一次发布；当前 cloned step、raw event/specialized command、旁路字段及单独失败 audit 的限定保持原文，不从目标推导全部当前 atomic。
持久化边界复用 §6.2.3 generation pointer commit，完整 journal/module/receipt/outbox/history 一致恢复，GC 在业务 commit 外；canonical replay 只消费历史、不调用 LLM/sandbox/dispatcher，不再发送外部效果。现有 ingest_receipt DTO 缺完整 receipt-domain descriptor ledger，at-least-once external dispatch 不声称物理 exactly-once。字段、schema/migration 未批准时保留 target gap，不能从状态图推导实现。

### 接口、容量、可观测与演进
接口输入仍是 §3 的版本化请求/context 与既有 API，输出为绑定 execution/root/journal/receipt 的 accepted/rejected/faulted；新增 pending/lineage/service 外部 DTO 尚待 owner。Kernel 校验权限、资源预算、capability/output/schema/artifact，WASM owns ABI；工业 buffer/hold 和 queue 以现有 domain/limit authority 作界，不复制数值。status/metrics 只投影真实 committed/pending/blocked/partial，read 不推进；高基数/敏感 payload 保持原有限制。incremental migration、savepoint、failure injection、serde/legacy replay、retain-more rollback 服从 §6.2.4/§6.3/§9，不能把当前 snapshot shape 强改成 target-v2。
风险是 generic pending/lineage/version/service/industrial schemas 和跨域执行证据尚缺。解除条件为 runtime/P2P/WASM/domain/consumer/QA 同候选审读及真实证据；SN1 后续变更必须消费精确已批准 delta 重验证；没有相关算法/字段/消费者批准时不开放目标能力。

### 2.1 需求承接与分配表

原承接视图输入身份为 eng-cc/oasis7@f9d5a552d9af04c1b1398262808198a58e560230；本次普通 generation 增补按[保存合同](#runtime-generation-persistence-design)所列基线。local anchors 是本文技术接受关系，非机器 schema。每行范围独立，外部未决保留。

| 上游 requirement / acceptance | 具体 obligation 与条件 | 本设计条款 | 外部 owner / dependency | 排除或未覆盖 |
| --- | --- | --- | --- | --- |
| [普通 generation 持久化](prd.md#runtime-generation-persistence-contract) | 普通保存按精确候选区分提交结果，提交前保留旧闭包、完整候选回退与服务资格分开 | [generation 合同](#runtime-generation-persistence-design) | runtime storage/cognition 现有操作 key；QA 本地故障 oracle | 目标合同；不证明全部 root transaction、正式 authority 或真实故障域恢复 |
| [单权威提交](../p2p/prd.md#p2p-single-authority-durable-commit) | 固定原决定按精确输入恢复，与当前重审 pending 分开；正式证明先于发布 | [接收设计](#runtime-controlled-candidate-recovery-receiver) | P2P continuation/proof 与存储；WASM/native 工件；QA 故障 oracle | 新接收协议待实施，现有 guarded-local 拒绝边界不变 |
| [事件后缀恢复](prd.md#runtime-journal-suffix-recovery) | 合法 finalized C<T 的非空事件后缀分别还原 J1/J2，并对同一未来输入正常续跑；旧历史缺记录显式处置 | [控制状态恢复](#runtime-journal-suffix-recovery-design) | runtime allocator/tick producer；WASM 输出；既有 rollback fixture；QA | 不代签 committed-input replay、完整 pending 产品或正式网络可服务 |
| Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[条款](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-001) | 同 version/ordered inputs/parent，全 active validator re-execute，missing/mismatch/越权零部分效果 | [设计](#runtime-deterministic-design) | P2P finality/集合/round；WASM artifact；QA 同候选 full | 单节点不代签 SC-1/4 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[条款](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-002) | 无 finality durable no-effect pending，恢复 fresh 当前权限/资源/期限/canonical order，只有 committed receipt 更新结论 | [设计](#runtime-pending-design) | P2P pending/order；Agent/Viewer/API 可读状态 | effect/cognition queue 不代签通用 signed pending |
| Trace refs: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[条款](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-003) | effect-bearing 首 winner+业务+loser termination 原子；reject/expire 自身，independent intent 并发 | [设计](#runtime-lineage-design) | domain 标记互斥/安全替代；P2P order；consumer association | 单操作 retry/签名不等于 lineage 仲裁 |
| Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[条款](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-004) | 首次 finalized canonical block 相对 boundary 选版，current rejudge、linked replacement、历史原版本、缺证阻断 | [设计](#runtime-version-design) | P2P activation proof；WASM ABI/artifact；consumer replan | local release/signature 不证明 activation history |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[条款](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | same-world identity/checkpoint/hash/log/root 各环，wrong/missing/conflict isolate、不迁移 pending/receipt | [设计](#runtime-recovery-design) | P2P/ops registry/checkpoint/window；QA/Viewer restore verdict | bridge CAS/root 非 BFT/disaster proof |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[条款](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005) | readonly/serving/isolated与一次 gate regression，append/finality/version/head全通过，failed gate receipt0、serving≤1 | [设计](#runtime-recovery-design) | P2P/ops 同窗口；consumer blocker/next；QA attachment | 结构化 attachment/schema 未提供仍阻断完整验收 |
| Trace refs: O7-REQ-05c84021-ee48-41ef-b94a-8aa9ceb8048c；[条款](../product/world-infrastructure/prd.md#sc-infra-008) | SC-8 root/revision/parent pre-sink，有界exclusive hold，满buffer背压，release+fresh recheck一次、因果child、retry零第二effect | [设计](#runtime-industrial-design) | M4/gameplay容量/quantities/window；Agent/Viewer/API；QA整链 | 现ActionId/path/job仅partial，root/join/window/bundle字段尚缺 |
| Trace refs: O7-REQ-4792541c-aaa4-4ac1-9a2d-374df44eba6a；[条款](../product/world-infrastructure/prd.md#sc-infra-009) | SC-9 A-SF/A-TR/A-BA/A-TS及B-SF/B-TR/B-BA/B-TS全部独立 baseline/progression/一次处置 | [设计](#runtime-industrial-outage-design) | gameplay W；runtime effect；Agent/Viewer/API parity；QA real provider/S6 | historical/provider_local_mock 不代签八格 |
| Trace refs: O7-REQ-2b7cfef9-d917-46ea-a3a9-e3a4fb96d730；[条款](../product/world-infrastructure/prd.md#sc-infra-010) | SC-10 submit前global/local identity，intent/receipt/result同world，wrong/missing/reuse/replay零global | [设计](#runtime-world-scope-design) | Agent/Viewer/API scope表达；QA required | local restore test未覆盖所有consumer |
| Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[条款](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-003) | SC-2非权威role不获finality/write；pruning可重建/hash/root/冗余archive | [设计](#runtime-deterministic-design) | P2P role/store/archive；ops exposure；QA full | runtime只贡献执行绑定/材料，不关闭role/DA |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[根 SC-1](../product/world-infrastructure/prd.md#sc-infra-001) | 合法 profile/epoch 与持久提交、BFT/交接组合，非单叶证明 | [接收条款](#runtime-deterministic-design) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[根 SC-3](../product/world-infrastructure/prd.md#sc-infra-003) | bootstrap/snapshot/replay/state-sync/pruning/灾备信任链各环及失败隔离 | [接收条款](#runtime-recovery-design) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607, O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[根 SC-4](../product/world-infrastructure/prd.md#sc-infra-004) | 两 profile、全部活动验证者、升级/混合版本与原历史重放 | [接收条款](#runtime-version-design) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405, O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[根 SC-5](../product/world-infrastructure/prd.md#sc-infra-005) | pending/未知结果区分、有效胜者原子处置、拒绝/过期自身与独立并发 | [接收条款](#runtime-lineage-design) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[根 SC-6](../product/world-infrastructure/prd.md#sc-infra-006) | 原世界恢复、缺失/冲突/异世界隔离、历史与未 final 请求连续性 | [接收条款](#runtime-recovery-design) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4, O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[根 SC-7](../product/world-infrastructure/prd.md#sc-infra-007) | 同候选同窗口服务三态、回退、receipt 0/1 与独立正向续跑 | [接收条款](#runtime-recovery-design) | 原 SC §5.2 的 P2P/runtime、运维、消费者及 QA 分工共同接收 | 仅本专业子范围；原 SC 全部组合和状态卡剩余不被叶子或本行代签 |
| Trace refs: O7-REQ-a15a1b8e-8b7f-4c6a-9cb5-2f949730496c；[入口 SC-9 独立义务](../product/player-entry-distribution/prd.md#sc-entry-009) | 会话中断不重开原 admission；未知先核对，固定原决定按原身份续接；不把缺响应当无效果 | [原决定恢复](#runtime-controlled-candidate-recovery-receiver) | 入口/session/读取权限/Agent owner 与 QA；本专业只接收原请求恢复子范围 | 不定义登录、授权字段或 UI；全入口状态/授权组合尚未闭合 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405, O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[根 SC-5](../product/world-infrastructure/prd.md#sc-infra-005) | 已知无效果 pending 与未知提交/固定原决定分开；不因无响应而重新 admission | [已有接收](#runtime-controlled-candidate-recovery-receiver) | 原 SC 的 P2P/runtime、运维、消费者及 QA 分工 | 本条只承接所列子义务；不替代根全部组合或未批准字段 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4, O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[根 SC-7](../product/world-infrastructure/prd.md#sc-infra-007) | 合法非空后缀后的新请求身份与模块行动时点独立续跑，对照结算和合法新配方 | [已有接收](#runtime-journal-suffix-recovery-design) | 原 SC 的 P2P/runtime、运维、消费者及 QA 分工 | 本条只承接所列子义务；不替代根全部组合或未批准字段 |

### 11.1 验证映射表

所有下列行为验证在本次文档编辑中未运行。定义/计划与当前实现、实际执行、发布分别成立；执行须另固定 source/integration/tested tree、config/world/entry/environment/window、exit/result/artifacts，并回 PR 与实际验证记录。target场景尚无完整runner时明确保持待证明，现有test/manual只是有界接收入口，不能伪称已实现或通过。

| 上游 requirement / acceptance | 本设计条款 | 独立 obligation / 条件 | 验证 source / ID、层级及候选环境 | 证据目标 | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [普通 generation 持久化](prd.md#runtime-generation-persistence-contract) | [generation 合同](#runtime-generation-persistence-design) | G1/G2/G3 分项及提交核对与候选回退交叉场景按 [精确矩阵](runtime/runtime-storage-footprint-governance.design.md#storage-generation-validation-matrix) 各自成立 | [现有局部 persistence 测试](../../crates/oasis7_runtime/src/tests/persistence.rs)；目标为同候选单 writer 临时目录中对比返回值、活 World、重启 World、索引与完整引用闭包；现有测试不能代签目标组合 | PR 中实际运行的故障窗、命令、结果与对账；文档检查不代签 | 本次未实现/运行行为验证；不证明跨主机、真实掉电、provider 或正式 serving |
| [单权威提交](../p2p/prd.md#p2p-single-authority-durable-commit) | [接收设计](#runtime-controlled-candidate-recovery-receiver) | 原决定恢复不重算 admission；正式 proof 才允许共同发布 | Planned CA-R1/R2/R3/CA-C1，承接 P2P 的原 candidate/普通消费者验证计划；现有基线 [execution transaction regressions](../../crates/oasis7_runtime/src/tests/execution_transaction_regressions.rs) 不覆盖新协议；同候选比较 parent/版本/nonce/roots 与外部调用计数 | 未来 task evidence 中的原身份、故障 seam、恢复 disposition、proof 和一次发布结果 | 本次未实现/运行；不证明 live、BFT 或任意旧 native 可移植 |
| [事件后缀恢复](prd.md#runtime-journal-suffix-recovery) | [控制状态恢复](#runtime-journal-suffix-recovery-design) | J1 allocator 与 J2 schedule 独立恢复、同输入续跑和缺证据拒绝 | [现有恢复测试](../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs) 扩展本页 J1/J2 planned cases；required 局部确定性恢复，既有 signed C→T 路径按其测试层级；同候选对照 cursor/schedule/下一动作与费用 | PR 中分别记录每项真实命令、fixture、结果与未运行项 | 仅文档/完整 snapshot 往返不证明非空 suffix；不扩大共识或外部服务范围 |
| Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[条款](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-001) | [设计](#runtime-deterministic-design) | 同 version/ordered inputs/parent，全 active validator re-execute，missing/mismatch/越权零部分效果 | [现有 test/manual](../../crates/oasis7_runtime/src/tests/execution_transaction_regressions.rs)；required 局部 failed_step/committed_context/snapshot-replay 定义；target full 同候选全 validator+transport conformance，compare roots/receipt 与零副作用 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 单节点不代签 SC-1/4 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[条款](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-002) | [设计](#runtime-pending-design) | 无 finality durable no-effect pending，恢复 fresh 当前权限/资源/期限/canonical order，只有 committed receipt 更新结论 | [现有 test/manual](../../crates/oasis7_runtime/src/tests/execution_transaction_regressions.rs)；required 局部 queue/receipt publication 定义；target pending outage+restart+权限/资源/期限变化，观察零资格/效果及执行/拒绝/过期/replan 真实状态 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | effect/cognition queue 不代签通用 signed pending |
| Trace refs: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[条款](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-003) | [设计](#runtime-lineage-design) | effect-bearing 首 winner+业务+loser termination 原子；reject/expire 自身，independent intent 并发 | [现有 test/manual](../../crates/oasis7_runtime/src/tests/effects.rs)；required 局部 anchor/receipt 定义；target original/withdraw/replacement race、rejected-first、independent、crash/retry/replay，winner effect≤1、loser0 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 单操作 retry/签名不等于 lineage 仲裁 |
| Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[条款](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-004) | [设计](#runtime-version-design) | 首次 finalized canonical block 相对 boundary 选版，current rejudge、linked replacement、历史原版本、缺证阻断 | [现有 test/manual](../../crates/oasis7_runtime/src/tests/module_action_loop_release_controls.rs)；full target pre/at/post activation，candidate/client/time 非权威、无静默translation/旧quote/mixed effects，old manifest replay与missing/conflict；局部 release controls 是测试定义 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | local release/signature 不证明 activation history |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[条款](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | [设计](#runtime-recovery-design) | same-world identity/checkpoint/hash/log/root 各环，wrong/missing/conflict isolate、不迁移 pending/receipt | [现有 test/manual](../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/driver_authoritative_recovery.rs)；full target bootstrap/snapshot/replay/state-sync/prune/disaster 各例；局部 restart_fails_closed_when_authoritative_cas_is_missing/rejects_stale_restore_from_other_world 定义 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | bridge CAS/root 非 BFT/disaster proof |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[条款](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005) | [设计](#runtime-recovery-design) | readonly/serving/isolated与一次 gate regression，append/finality/version/head全通过，failed gate receipt0、serving≤1 | [现行 manual](../../testing-manual.md)；精确局部 source ../testing/templates/state-sync-closure-evidence-packet-template.md（定义/入口，非执行证据）；full target 同 candidate/window 三恢复类+提交/恢复回退，manifest/head negatives、新intent0/1、pending无期限/优先级继承；template仅envelope | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 结构化 attachment/schema 未提供仍阻断完整验收 |
| Trace refs: O7-REQ-05c84021-ee48-41ef-b94a-8aa9ceb8048c；[条款](../product/world-infrastructure/prd.md#sc-infra-008) | [设计](#runtime-industrial-design) | SC-8 root/revision/parent pre-sink，有界exclusive hold，满buffer背压，release+fresh recheck一次、因果child、retry零第二effect | [现有 test/manual](../../crates/oasis7_runtime/src/tests/economy_priority_logistics_network_tests.rs)；full target代表性整链+fan-in/out、wrong-root、full edge/dest、duplicate/later release事件、remaining、terminal-late/replay；局部whole-path reservation是定义 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 现ActionId/path/job仅partial，root/join/window/bundle字段尚缺 |
| Trace refs: O7-REQ-4792541c-aaa4-4ac1-9a2d-374df44eba6a；[条款](../product/world-infrastructure/prd.md#sc-infra-009) | [设计](#runtime-industrial-outage-design) | SC-9 A-SF/A-TR/A-BA/A-TS及B-SF/B-TR/B-BA/B-TS全部独立 baseline/progression/一次处置 | [现有 test/manual](../../testing-manual.md)；required 八格deterministic+active-provider pure API；full real local/provider external headed desktop+narrow screenshots/console+provider-backed Agent；每格对量/root/receipt/W/next，target/unrun | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | historical/provider_local_mock 不代签八格 |
| Trace refs: O7-REQ-2b7cfef9-d917-46ea-a3a9-e3a4fb96d730；[条款](../product/world-infrastructure/prd.md#sc-infra-010) | [设计](#runtime-world-scope-design) | SC-10 submit前global/local identity，intent/receipt/result同world，wrong/missing/reuse/replay零global | [现有 test/manual](../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/driver_authoritative_recovery.rs)；required target distinct global/local submit/replay/receipt reuse+consumer scope，missing/mismatchfail closed；局部other-world restore test定义 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | local restore test未覆盖所有consumer |
| Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[条款](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-003) | [设计](#runtime-deterministic-design) | SC-2非权威role不获finality/write；pruning可重建/hash/root/冗余archive | [现行 manual](../../testing-manual.md)；精确局部 source ../testing/templates/state-sync-closure-evidence-packet-template.md（定义/入口，非执行证据）；full target role权限负例及pruning/reconstruction/archive same-window；仅envelope，无运行事实 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | runtime只贡献执行绑定/材料，不关闭role/DA |
| Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[根 SC-1](../product/world-infrastructure/prd.md#sc-infra-001) | [接收条款](#runtime-deterministic-design) | 合法 profile/epoch 与持久提交、BFT/交接组合，非单叶证明 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-FINALITY`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[根 SC-3](../product/world-infrastructure/prd.md#sc-infra-003) | [接收条款](#runtime-recovery-design) | bootstrap/snapshot/replay/state-sync/pruning/灾备信任链各环及失败隔离 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-RECOVERY`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607, O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[根 SC-4](../product/world-infrastructure/prd.md#sc-infra-004) | [接收条款](#runtime-version-design) | 两 profile、全部活动验证者、升级/混合版本与原历史重放 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-EXECUTION`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405, O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[根 SC-5](../product/world-infrastructure/prd.md#sc-infra-005) | [接收条款](#runtime-lineage-design) | pending/未知结果区分、有效胜者原子处置、拒绝/过期自身与独立并发 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-EXECUTION / V-CONSUMER`；根 `test_tier_required` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[根 SC-6](../product/world-infrastructure/prd.md#sc-infra-006) | [接收条款](#runtime-recovery-design) | 原世界恢复、缺失/冲突/异世界隔离、历史与未 final 请求连续性 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-RECOVERY`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4, O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[根 SC-7](../product/world-infrastructure/prd.md#sc-infra-007) | [接收条款](#runtime-recovery-design) | 同候选同窗口服务三态、回退、receipt 0/1 与独立正向续跑 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-RECOVERY / V-CONSUMER`；根 `test_tier_full` 与叶子局部层级分别保留，同候选/同窗口 | 按原 SC §5.1 记录各独立判据、原始 artifact 和未执行项 | 本轮未运行；不能用单叶局部绿色或历史文档接收关闭根组合 |
| Trace refs: O7-REQ-a15a1b8e-8b7f-4c6a-9cb5-2f949730496c；[入口 SC-9 独立义务](../product/player-entry-distribution/prd.md#sc-entry-009) | [原决定恢复](#runtime-controlled-candidate-recovery-receiver) | 原结果与会话正交，查询/原身份重试不产生第二效果 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：入口 U1～U5（../product/player-entry-distribution/prd.md#player-session-unknown-outcome-acceptance） 中的原请求/回复丢失/错误归属子范围；原 test_tier_full，每个 primary mode 同候选/世界/主体分别成案 | PR 中原请求、持久结果、故障窗口、查询返回及副作用次数 | 本次未运行；不代签 Agent 委托、读取授权或真实 hosted/player surface |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405, O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[根 SC-5](../product/world-infrastructure/prd.md#sc-infra-005) | [已有接收](#runtime-controlled-candidate-recovery-receiver) | 已知无效果 pending 与未知提交/固定原决定分开；不因无响应而重新 admission | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：QA 场景（../testing/prd.md#qa-infrastructure-scene-families） `V-EXECUTION / V-CONSUMER`；根 test_tier_required 同候选条件，局部层级不代签组合 | PR 独立记录前置、故障/续跑条件、原始结果和未执行项 | 本次未运行；产品与专业原剩余边界继续有效 |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4, O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[根 SC-7](../product/world-infrastructure/prd.md#sc-infra-007) | [已有接收](#runtime-journal-suffix-recovery-design) | 合法非空后缀后的新请求身份与模块行动时点独立续跑，对照结算和合法新配方 | [验证执行入口](../../testing-manual.md)；仅复用该手册的分层/实际执行记录要求，以下为未运行目标用例：日志后缀独立验收（#runtime-journal-suffix-validation）与GWSC 续跑设计（../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-journal-suffix-recovery）；根 test_tier_full 同候选条件，局部层级不代签组合 | PR 独立记录前置、故障/续跑条件、原始结果和未执行项 | 本次未运行；产品与专业原剩余边界继续有效 |
