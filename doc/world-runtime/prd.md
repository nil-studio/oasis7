# world-runtime PRD

> 专业域 authority：本文件拥有世界基础设施上层的确定性执行、WASM、事件、receipt、checkpoint 与 replay 契约；它向 [`doc/product/world-infrastructure/prd.md`](../product/world-infrastructure/prd.md) 提供执行层证据，并向 [`doc/product/world-rules-core-gameplay/prd.md`](../product/world-rules-core-gameplay/prd.md) 及 gameplay、Agent 与 Viewer 等消费者提供已提交世界状态的技术合同。它不拥有产品规则、共识/网络部署或玩家交互承诺。

审计轮次: 7

## 目标
- 建立 world-runtime 模块设计主文档，统一需求边界、技术方案与验收标准。
- 确保 world-runtime 模块后续改动可追溯到 PRD-ID、GitHub task、测试和 Git history。

## 范围
- 覆盖 world-runtime 模块当前能力设计、接口边界、测试口径与演进路线。
- 覆盖 PRD-ID 到 GitHub task 与测试证据的可追溯映射。
- 不覆盖实现代码逐行说明与历史过程记录。

<a id="runtime-authority-profile-commit"></a>
### 合法 authority profile 的执行、提交与恢复（目标）

runtime 接受 [P2P 合同](../p2p/prd.md#p2p-authority-profiles) 已合法激活的 `controlled_single_authority` 或 `bft` 下的有序输入，复用同一确定性状态机、receipt 与 replay。单权威正确性信任包含运营权威；BFT 继续要求每个活动验证者独立重执行和完整证书。`ExecutionReceipt` 不自行授予提交权，兼容 tick/threshold certificate 不得改名取得正式权威。

执行候选先准备，在合法 profile 提交证明、原子 epoch/parent/head 追加和规定持久条件全成立后发布 committed。实际开放链中输入扣除、产出、事件、结果、持久里程碑、nonce/幂等身份共同提交；预备记录不产生权威效果。正式提交决定/证明、head/epoch、完整输入结果及引用工件必须在本机和独立故障域持久保存后才能确认 committed，以保证单主机/单存储故障域失效不丢已确认效果。现有 prepared publication 只证明局部无中间态，不能代替跨故障域持久提交。

结果未知时保留同一请求身份：断连/超时无法证明未提交，显示“提交状态未知、待核对”，从持久判定查询或完成原提交；不能当确定失败或全新动作重做。已知无效果且已终止才可判失败。已知未生效 pending 在恢复后按有效权限、资源、manifest 重审；未知提交先核对原提交决定。外部行为只沿既有提交后派发/幂等边界执行，replay 不重新调用 LLM 或重发副作用。

恢复沿固定 `world_id`/`chain_id`/`genesis_hash`、各历史高度合法 profile/epoch/manifest 激活链、已确认 checkpoint、hash-bound snapshot、连续日志和 state-root 核对；授权、Agent 归属、资产、目标、pending、里程碑与去重共同核对。固定身份不冻结所有未来 manifest hash；历史按当时合法版本重放。先开放验证只读，当前 authority/head/版本/独立持久确认重新成立才开放写入，不能回退已生效 epoch。

H/H+1 切换由 P2P 决定：H 的旧 profile 正式交接证明绑定交接执行后的结果根，H+1 起只接受新 profile。H 已提交后不可撤回，重启或新 BFT 集合未启动时保持停写；runtime 不以旧 snapshot 恢复旧追加权，也不重发资产或自动执行失效 pending。当前 prototype、局部执行接线与故障域/消费者验收仍分开记录，文档合并不声明首次开放条件已满足。

### 世界基础设施执行目标与当前差距

- **模块身份（目标）**：`world-runtime` 是区块链/分布式世界基础设施的上层确定性执行层。下层的合法 authority profile、validator registry、BFT finality、P2P、DistFS 与状态同步由 `doc/p2p/` 拥有；本层把已排序、已验证的动作确定性地转化为事件、状态根、receipt、checkpoint 与可重放历史。工业设施、市场、gameplay module 与 Agent 行为是本层的消费者或执行负载，不定义本模块身份。
- **稳定边界（目标）**：共识到执行必须是独立、版本化的协议，而不是对进程内 Rust 类型或私有存储的依赖。每个请求至少绑定 `world_id`、协议/运行时 manifest version、parent committed height/hash、确定排序的 action envelope 与 `action_root`；每个结果至少绑定 execution block/hash、`state_root`、receipt/journal references 和结构化拒绝或 fault。当前可先由 in-process adapter 承载；未来 IPC adapter 必须通过同一份 conformance/replay fixture，进程拆分不是协议生效前提。
- **复制执行（BFT 目标）**：proposer 只提出候选。每个 active validator 从同一已提交 parent state 重新执行同一 action sequence，并且只在自己的 `action_root`、execution hash、`state_root` 与候选一致时投票；任何不一致、缺 artifact、超限或不可解释 fault 都不得被签名、提交或以本地修补越过。最终性证书和 BFT round/vote 语义由 p2p authority 定义，本层负责可复现执行结果及其持久化绑定。
- **升级与发布（目标）**：普通确定性 runtime 升级由治理最终确认的 content-addressed runtime manifest/version 在 committed height 激活；节点 release 仅预置兼容实现，不是激活权威。改变 consensus、该协议或不兼容 host ABI 的升级属于 coordinated foundational protocol upgrade/fork，必须另有迁移证明。发布分类固定为四条 lane：rolling node software、governance-activated runtime manifest、independent client application、coordinated foundational protocol upgrade/fork。
- **部署与降级（目标）**：game 与基础设施持续通过同一版本化协议协作。ordinary player 为 game + light companion，operator 为 full infrastructure node，dev/local 为 game + embedded/full local node。普通 player/light companion 在 finality 不可用时只能显示最后一个已验证状态，并可保留尚未产生世界效果的 signed pending intent；不得本地推进权威世界。dev/local 必须使用独立 `world_id`，不得把离线历史合并回 global world。

| 主题 | 当前能力事实 | 目标差距 |
| --- | --- | --- |
| 共识执行绑定 | 已有 ordered action/root、`NodeExecutionCommitContext/Result`、execution record 与 committed replay 的局部绑定。 | 尚未将独立版本化协议和 in-process/IPC adapter conformance 固化为实现合同。 |
| 复制执行与 finality | 当前是 stake-weighted proposal/attestation threshold prototype；正常默认不自动替所有 validator 生成 vote。 | 尚未提供每个 validator 重执行后的 signed >2/3-stake commit certificate、prevote/precommit round、lock、timeout/view-change 或可验证 partition recovery。 |
| runtime 升级 | 模块 registry/manifest/artifact hash、兼容检查、顺序 lifecycle event 与 replay 已有基础合同；governed proposal apply 已改为 borrowed-base typed prepared publication，在 registry/artifact/schedule/cache invalidation、manifest/proposal、event allocator、journal/backpressure 与 consensus 全部预演成功后一次安装，register/upgrade/activate/deactivate 的 authority-drift 与 post-prepare 回归覆盖零中间态发布。 | 其他 lifecycle 入口、registry 与 instance state 的一致迁移、runtime manifest activation height、validator prefetch/readiness、持久化/replay、外部 effect 与 receipt/outbox 仍未收敛为端到端 transaction 合同。 |
| 消费者与不可用状态 | chain-linked consumers 已要求 committed-only visibility，replay/restore 不一致会阻断。 | 尚未完成 light companion proof verification、pending-intent durable queue、stale/unavailable projection，或 local/global `world_id` 隔离的完整实现与验证。 |
| 恢复 | checkpoint + canonical log + state-root comparison 是现有 runtime recovery contract。 | 尚未完成 fixed world/chain/genesis identity -> legal historical profile/epoch/manifest activation chain -> verified committed checkpoint -> hash-bound snapshot -> replay -> root verification 的全链路恢复/灾备证明。 |
| 工业 operation identity | 当前 `ActionEnvelope.id` 是单次 action identity；部分异步过程把该 `ActionId` 复用为 `job_id`，`WorldEvent.caused_by` 只提供可选的 action/effect 审计原因。因此当前能追踪单个 action、job 与直接 cause，但不能表达跨 stage/join/bundle/branch/transit/buffer/terminal/window/checkpoint/receipt 的 immutable root、owning revision/segment、直接 parent/child role 或 terminal finality。 | 在 authoritative accepted outcome 边界原子签发一次 immutable root operation identity；atomic reject 且无 accepted intent 时不签发。所有 child effect/receipt 持久化 root、owning revision/segment 与直接 parent/child role，并在 first sink/credit/progress 前对缺失或冲突 identity fail closed；retry、recovery 与 replay 重读同一 identity 和 terminal disposition。 |

Threat heatmap preparation is a pure runtime projection: `prepare_threat_heatmap(&self)` reads the immutable world view into a fresh `BTreeMap`, and `refresh_threat_heatmap()` installs it once. Its nonmutation/reuse behavior is a composability seam only; it does not claim root transaction atomicity or change product rule semantics.

Emergency-brake activation/release use the canonical prepared publication seam for their World sidecar: existing guardian checks and activation-max/release-`None` behavior remain unchanged, and post-prepare failure publishes neither brake state nor event id/era, journal, backpressure, or tick consensus. The sidecar remains outside the canonical `WorldState` root hash schema; this is a bounded event-publication guarantee, not a new whole-World atomicity claim.

Finality epoch snapshot set/remove use the prepared governance sidecar seam: predecessor, normalization, and removal-drift validation remains identical for live publication and replay, while a post-prepare failure publishes neither the persisted map entry nor event id/era, journal, backpressure, or tick consensus. The map remains outside the canonical `WorldState` root hash schema; this is a bounded direct-mutator guarantee, not whole-World atomicity.

Emergency-veto proposal publication prepares the rejected proposal sidecar before one canonical install; guardian authorization, Approved-plus-queued eligibility, exact rejection reason, cleared queue fields, and replay semantics remain unchanged. Post-prepare failure publishes no proposal mutation or event/allocator/journal/backpressure/consensus change. This bounded guarantee does not claim whole-World root atomicity.

Identity-penalty appeals prepare an immutable replacement record before one canonical install, with replay reusing the same helper. Appellant/reason validation, Applied-status and deadline checks, legacy detection-field backfill, exact appeal evidence hash, and evidence-chain extension remain unchanged. Post-prepare failure publishes no penalty-map mutation or event/allocator/journal/backpressure/consensus change; the sidecar remains outside the canonical `WorldState` root schema, so this is a bounded guarantee rather than whole-World root atomicity.

Identity-penalty appeal resolution prepares the penalty replacement and target identity profile together, using the borrowed canonical profile-map projection for the next state root. Accepted refund/status restoration, rejected timestamp updates, legacy backfill, exact `resolve_accept`/`resolve_reject` evidence hashes, and replay semantics remain unchanged. Missing-profile and post-prepare failures publish no partial penalty/profile or event/allocator/journal/backpressure/consensus change; this is a bounded guarantee, not whole-World root atomicity.

Identity-penalty application previews the next penalty id and prepares the penalty record, profile mutation, and allocator successor before publication. Its borrowed profile-map projection inserts a missing default profile in sorted order, and the prepared consensus header/digest commits the resulting root; invalid stake, duplicate, signer, and post-prepare failures consume no allocator or publication state. This is a bounded direct-mutator guarantee, not whole-World root atomicity.

Governance proposal creation and shadowing now preview proposal allocation and prepare the replacement proposal sidecar before canonical publication. Existing public manifest/patch, module-change, proposal-status, and shadow-hash validation remains unchanged; post-prepare failure leaves proposal state, proposal id/era, event id/era, journal, backpressure, and tick consensus unchanged, and retry/replay preserve the original event payloads and rollover semantics. The proposal sidecar remains outside the canonical `WorldState` root schema; this is a bounded direct-mutator guarantee, not whole-World root atomicity.

Approval and queueing now prepare the ordered `Approved`/`Queued` pair before one canonical install, while rejection keeps its single `Approved` event semantics. Post-prepare failure leaves the proposal, event allocator, bounded-journal retention/backpressure, and tick-consensus record unchanged; retry and replay preserve both event payloads and the final consensus counters. The proposal sidecar remains outside the canonical `WorldState` root schema, so this is a bounded guarantee rather than whole-World root atomicity.

Module output event publication prepares `ModuleStateUpdated` and
`ModuleRuntimeCharged` state before publishing the journal, event allocator/era,
backpressure, and consensus record. Runtime charging validates both ordered
debits on an owned payer cell and touched treasury entries; a failed second
debit leaves canonical balances unchanged, including when both fees share a
resource kind. Replay uses the same charge preparation. This is an event-level
guarantee, not completion of the whole root transaction.

Module instance install/upgrade events likewise prepare the payer fee, instance,
module-keyed install target, install counter, and legacy world-material cache
normalization before mutation. Active schedule lookup must succeed before state
or publication changes. Invalid upgrades and post-prepare failures leave state,
mailboxes, schedules, event allocators, journal/backpressure, and consensus intact.
Successful retry/replay preserves the existing identity, fee/error ordering, and
schedule semantics. Rollback uses the same prepared instance-state boundary,
without introducing instance-key schedule synchronization.

Governed install, upgrade, and rollback now compose proposal application with
the final instance event before one canonical install. After the proposal is
approved/queued, a lifecycle-tail failure publishes neither applied governance
state nor the instance change, fees, cache invalidations, business journal batch,
or consensus candidate. Proposal creation/shadow/approval audit remains outside
this boundary. Governance-preparation errors keep their action-rejection handling;
tail errors propagate. Release application extends that same boundary through
the sorted product/recipe/factory profiles and final release request/mapping
status. Already-registered modules use the same completion tail without a
governance batch. Any tail error leaves installation, fees, profiles and release
status unpublished; proposal/shadow/approval audit remains outside. The enclosing
action/root transaction is still a separate boundary.

Raw profile and final-release events also use prepared state/publication paths,
including replay. Preparation preserves actor/proposal/field validation order
and request/status/mapping error priority, and touches only affected map entries,
agents and legacy world-material normalization. Installer/operator identity
overlap preserves both the fee debit and exactly-once mailbox routing. A matching
module with proposal id zero still rejects nonempty governed profiles; empty
profile completion remains permitted.

Shadowed, role-approved and rejected release-review events must prepare their
request, optional or required mapping, affected actor and publication metadata
before canonical installation. Failed validation or publication leaves review
status, hash/reason/approvals, actor activity/mailbox, legacy material state,
journal cursors/retention and consensus unchanged. Shadow requires its mapping;
approval and rejection intentionally tolerate one being absent. Raw-event
normalization and missing-actor behavior remain distinct from stricter action
validation, and replay must reproduce the published root without cloning the
world.

Release request、attestation 与 role-binding 三类 precursor 也必须进入同一 sparse prepared release delta。request/mapping、request allocator、role-binding 增删、受影响 agent activity、requester/operator mailbox 与 legacy material normalization 必须先形成 borrowed root，再经过 publication failpoint 一次安装。attestation 的 late mapping 错误不得留下 request attestation；role binding 必须更新 operator 与不同 target 的 activity，但 mailbox 只投递 operator。raw reducer 的验证顺序与 action 层更严格的规范化/拒绝边界保持不变。

Marketplace listed, bid-placed and sale-completed events now prepare affected
state before publication, including replay. A listing/bid and its immediate
matching sale commit together or leave fees, agents, owner, order book, market
allocators, mailboxes, journal and consensus unchanged. Existing raw-event
validation order and action policy remain distinct; sale price and bid ordering
are unchanged. Sparse deletion-aware projections preserve published/replay root
equality without cloning the world. This does not migrate other marketplace
actions or alter module-instance ownership.

Validated source or binary deployment bytes and the corresponding deployed
event must install together. A publication failure preserves the artifact hash
set and byte map as well as publisher fees, ownership, market orders, mailboxes,
journal and consensus. Raw deployed events reuse sparse state preparation but
do not validate or reconstruct artifact bytes. Standalone registration remains
available, same-hash redeployment retains its fee and overwrite semantics, and
the module cache is unchanged. Artifact-byte recovery remains the persistence
sidecar contract because the event carries only hash and byte length.

Module artifact delist, bid-cancel, and destroy events MUST validate their
complete canonical transition before publishing state, journal, event
allocation, retention, or consensus changes. Failed publication MUST preserve
legacy material-ledger representation and all touched marketplace and agent
state. Raw destroy events MUST retain their state-only compatibility and MUST
NOT remove artifact membership, bytes, or cache entries.

A successful `DestroyModuleArtifact` action MUST bind its retirement sidecar
to a destroyed event for the identical hash. Canonical teardown and removal of
artifact membership, bytes, and the complete module cache MUST either all
become visible or all remain absent. Cache capacity, registry, instances, and
tick schedules MUST remain unchanged; action validation priority and supplied
reason bytes remain compatible.

This publication boundary alone does not establish recoverable retirement.
The [module persistence contract](#module-store-persistence-and-recovery-contract)
also requires the accepted retired state to remain readable without making
unrelated live modules unavailable or deleting historical registry records.

The standalone multi-module change API MUST commit its sorted register,
upgrade, activate, and deactivate events as one failure boundary. A validation,
publication, retention, allocator, schedule, cache-invalidation, or consensus
failure in any event MUST preserve the complete pre-batch world. Equal module
ids retain input order; proposal id, actor fields, exact upgrade/deactivation
payloads, and `caused_by = None` remain compatible. Empty changesets MUST be
strict no-ops, and raw single-module events and governed proposal behavior MUST
remain unchanged.

Standalone public action routing now stages the complete deterministically
sorted module invocation set against a borrowed `World` base and installs one
prepared result containing module state, effects, emits, runtime charges,
cache-miss additions, allocators, journal, and tick consensus. A grouped
durable base-head check rejects stale prepared installs before any write, while
live process-local cache entries and wall-clock telemetry remain local. Module faults discard staged
business output before retaining one existing `ModuleCallFailed` audit;
post-prepare infrastructure failures install neither business output nor an
audit. This is a bounded direct action-route guarantee, not a claim that
`step()` or all nested event/tick paths already share the root transaction.

Standalone public event routing now uses the same borrowed prepared stage for
the sorted subscribed invocation set. Event bytes, post-event context,
module state, effects, emits, runtime charges, cache-miss additions, allocators,
journal, and tick consensus are installed once after preparation. The same
durable base-head guard runs before publication. Module faults discard
staged business output before retaining one existing `ModuleCallFailed` audit;
post-prepare infrastructure failures install neither business output nor an
audit. This closes only the direct event-route seam; tick scheduling and
metrics remain a separate lifecycle boundary.

Standalone public tick routing stages due schedule removals, wake directives,
deterministic routing metrics, module output, cache-miss additions, allocators, journal, and
backpressure together. Its prepared envelope deliberately carries no
consensus candidate; `run_modules_for_current_tick()` owns the single final
tick record. A snapshot taken after direct routing but before that finalization
is not a replayable canonical checkpoint. A finalized checkpoint is a valid
starting cut, but does not by itself prove that a later event suffix reconstructs
schedule/metric sidecars. That reconstruction remains a current limitation;
the [event-suffix recovery target](#runtime-journal-suffix-recovery) requires a
complete recorded path from an earlier finalized checkpoint to the target.
Due-record preflight errors retain the compatibility exception of recording
metrics directly while leaving the schedule unchanged.

### Kernel、governed physics 与 institution module 边界

本节把 issue #3370 的架构建议落为 world-runtime 的执行约束。它不定义产品规则、WASM wire ABI、Agent tool schema 或 p2p finality 算法；产品规则仍由产品/gameplay authority 拥有，ABI 细节由 [`wasm-interface.md`](wasm/wasm-interface.md) 拥有，finality 由 [`doc/p2p/`](../p2p/) 拥有。

运行时采用四层边界：

| 层 | world-runtime 责任 | 允许的扩展方式 | 明确禁止 |
| --- | --- | --- | --- |
| L0 Kernel | 身份与 world 边界、逻辑时间、空间/占用合法性、资源守恒、权限与执行 provenance 的强制校验；统一状态提交与拒绝 | 只通过经过版本化的 kernel command / event 进入 | WASM、native gameplay 或 Agent 直接写 canonical state、跳过校验或凭 host wall clock 改变结果 |
| L1 Governed physics parameters | 受治理、版本化、可回放的数值模型与限制，例如移动成本、速度、容量和损耗参数 | 通过 content-addressed runtime manifest、activation boundary 与配置版本更新 | 把 L0 不变量降级为可选 module；运行中任意改变参数或在 receipt 外隐式读取配置 |
| L2 Institution modules | module command/event/state 的路由、沙箱、预算、权限、生命周期与持久化；为文明制度提供可组合扩展点 | WASM module 的 namespace、schema/version 与受限 host effects | 为每个新制度继续扩大核心 `Action` enum / `WorldState` 专属字段，或从 module 绕过 Kernel 直接结算 |
| L3 Agent strategy | 接受 Agent intent，提供受治理 capability discovery 的输入边界，并把调用送入同一执行管线 | 安全 Kernel commands 与已激活 module capabilities | Agent 成为第二执行权威、携带未声明字段改变状态、绕过 command/event/replay 合同 |

L0 的物理不变量至少包括：时间只能按确定性逻辑推进；位置、world 和占用关系必须合法；移动或其他物理效果不得无成本瞬移或制造未记录的资源；所有 debit/credit、费用、产出与销毁都必须在同一可审计状态转换中有明确来源；模块超限、权限失败、缺少 artifact 或输入不确定时必须原子拒绝并留下结构化原因。L1 可以调整模型参数，L2 可以增加收费、许可、道路、租赁或其他制度效果，但不能豁免 L0。具体数值与玩家后果不在本节定义。

这一区分不否定现有可玩的 native vertical slice。当前 Alliance 与 EconomicContract 作为 `Compatibility keep` surface 冻结新增语义，War 为 `Defer`，Governance 的授权、政策、最终性与反滥用护栏为 `Kernel keep`；这些现存能力均不因此成为未来新增制度的默认扩展方式。任何新增文明制度 action/state 进入实现前，必须先经跨角色分类并选择迁移策略。

#### Native surface 分类与冻结规则

| 分类 | 当前/示例 | 迁移规则 |
| --- | --- | --- |
| Kernel 保留 | time、space、identity、ownership、资源守恒、通用执行成本，以及 Governance 的授权/政策/最终性/反滥用护栏 | 由 runtime 强制，不能由 module 关闭；具体治理产品规则不因护栏保留而转入 runtime authority |
| Module candidate | market、tax、currency、bank、corporation、reputation、crisis 等已经跨角色分类的制度语义 | 需 command/event/state、权限、预算、snapshot/replay 与 receipt conformance 证据；Alliance/EconomicContract 只能按产品 authority 择一转入 |
| Compatibility keep | 当前 native Alliance 与 EconomicContract 及其旧 snapshot/event/replay shape | 两者在选定前均冻结新增语义；治理批准的 activation boundary 只能择一转为唯一 pilot `Module candidate` |
| Defer | War、big-bang ECS、完整动态 World Database、independent region/shard finality | War 不进入本版本试点或新增 native action/state；其余项不作为当前产品或 runtime 承诺 |

冻结规则是“先分类、后实现”：未通过上述矩阵的制度需求不得新增核心 enum、专属 `WorldState` 顶层字段或第二套执行路径。首个试点必须在治理批准的 activation boundary 中按产品 authority 从 Alliance/EconomicContract 择一；producer、runtime 与 WASM 只提供分类与执行证据，不代替治理批准。War/Governance 不得被试点隐式改类。

#### Open extension 的 runtime 合同（不展开 ABI）

目标扩展由 module-owned namespace 中的 versioned command、event 与 state 组成。runtime 只负责校验 module identity/manifest、确定性排序、权限与资源预算、调用结果的结构化收集、Kernel apply、持久化及 replay；namespace 的 wire shape、schema 编码、capability/export 名称与 ABI 迁移规则由 WASM 专题定义。module state 必须随 module instance identity、artifact hash、schema/version 和 activation 状态持久化，不能只依赖全局 `module_id` 或进程内 cache。

扩展事件与 native compatibility event 必须进入同一 canonical journal；任何 module effect 都必须经过 Kernel apply，失败不得留下半个 debit、credit、event 或 schedule。当前 public direct module call/command、trusted capability command 与 governed module lifecycle proposal apply 已使用 borrowed-base typed stage：前两者在 module state/resource fee/effect/emit、event/intent sequence、queue/backpressure、journal 与 consensus 全部 prepare 后才不可失败安装；governed proposal 额外预演 registry/artifact/schedule/cache invalidation、manifest/proposal、顺序 lifecycle events 与最终 consensus 后一次安装。authority-drift 与 post-prepare 回归证明失败不发布中间态，`ModuleCallFailed` 会在丢弃 staged 业务变化后以现有格式单独原子记录一次。公开 `step()`、`step_with_modules()` 与 committed-context wrapper 仍使用 cloned `World` 粗粒度边界；native tick 的 nested reducer 迁移已从 factory depreciation 开始，该处理器先基于 immutable world view 生成确定性排序的 event bodies，再进入 canonical `append_event`，并由 prepare 非变异回归约束，但尚未形成 root prepared batch。其他 publication/infrastructure failure 不追加第二事件。内部阶段、其他 lifecycle 入口、instance-state 对齐、恢复/迁移写入、durable external effect 与 receipt/outbox 尚未统一，因此不能据此宣称已经提供完整 ExecutionReceipt 级别的原子提交。

restricted starter-grant expiry 也已采用相同 nested precursor：基于 immutable world view 固定 eligibility、余额派生的 expired amount、grant metadata 与 BTreeMap account 顺序，再逐条进入 canonical publication；它仍依赖外层 step rollback，不代表独立 batch commit。

到期 material transit completion 也先在 immutable world view 上按 `(ready_at, priority, job_id)` 固定完成事件、loss/received 结果与 SLA metrics 投影；只有全部事件经 canonical `append_event` 成功后才安装 metrics 投影。该 seam 保留 urgent/standard 排序与 saturating counter 语义，但仍依赖外层 step rollback 来覆盖多事件 publication failure，不代表独立 root batch commit。

单条 raw logistics topology、direct material transfer 与 material-transit start/completion publication 使用按组拆分的 typed sparse transition。Transit preparation 只拥有 touched pending/settled IDs、route reservations、completed path/route authority、settlement receipt、world/source/destination ledgers、compatibility material cache、payer/owner/requester cells 与 industry progress；借用其余 state 计算 canonical root。Exact duplicate completion 保持业务状态与 activity 不变但仍投递一次既有 raw actor mailbox。验证或 post-prepare failure 不得留下 reserve/debit、destination credit、path authority、owner payout、progress、event allocator、journal 或 consensus 半写；外层 tick 的多事件 rollback 边界不因此扩大。

Factory build、completion、durability、maintenance 与 recycle 同样通过独立的 sparse factory-lifecycle transition 发布。它只暂存 touched build job、factory upsert/delete、settled/tombstone ID、相关 material ledger、compatibility cache、actor cell 与 progress；recycle 的 retired duplicate 保持业务状态和 activity 不变，但仍按 raw event 合同路由一次 actor mailbox。所有 owner、active-recipe、材料 preflight 与 completion identity/ready-time 校验均在 publication 前完成。

Recipe start、completion、blocked、resume 与 pause 使用独立的 sparse recipe-lifecycle transition。它只暂存 touched pending/settled job、factory、material/power ledger、logistics path、actor cell、progress 与 quote sink；重复 completion 或 terminal product-validation block 保持业务状态和 activity 不变，但仍提交一次 raw actor mailbox。材料、power、path allocation、timing、payload 与 output-capacity 校验全部先于 publication，失败不会留下部分 debit、reservation、output、factory slot 或 progress。

Core agent、body、observation、gameplay-policy 与 material-profile 的 raw publication 使用一个 full-event-bound sparse transition。该 transition 只暂存单个 touched agent、兼容 world material ledger、可选 policy/industry-progress 与单个 material profile；`ActionRejected` 保持 true no-state/no-route，缺失目标的 move/observation route 保持 no-op。Body interface consume 与 profile/policy 校验在 publication 前完成，所有成功 raw actor event 的 prospective root 恰好包含一次 mailbox route。

Canonical publication 对每个 `WorldEventBody` 都要求显式 typed classification；无法分类的 body 在 reducer、allocator、journal 与 consensus 之前 fail closed，不再允许进入 legacy mutate-first fallback。Intentional no-state body 同样绑定完整 payload，route-only 仅保留 full-domain-event-bound 形式。已由 core-policy projector 覆盖的 agent/body/policy/profile legacy reducer branches 已删除，replay 与 live publication 共用同一 transition。

native due-economy completion 先在 immutable world view 上固定全部 `FactoryBuilt` 与 `RecipeCompleted` event bodies：build phase 始终先于 recipe phase，两个 phase 内继续使用既有 production-priority、ready-time、job-id 排序和完整 payload。prepared bodies 仍逐条进入 canonical publication，因此跨事件失败原子性继续由外层 cloned-step rollback 提供，而不是由该 nested seam 独立提供。

agent-claim epoch 不能从同一个 base snapshot 整轮预生成，因为同一 owner 的前一个 upkeep debit、claim removal、grace/release/reclaim 与 refund provenance 会改变后一个 claim 的合法分支。当前 nested seam 因此每次只准备一个 event body，发布后重读最新 claim 与余额，再决定 follow-up；BTreeMap target-agent 顺序与 `agent_claim_last_processed_epoch` 仅在整轮成功后推进的语义保持不变。整轮 publication failure 仍依赖外层 cloned-step rollback。

gameplay cycle 采用 stage-scoped preparation，而不是从 tick 初始态生成一个跨阶段 vector。当前 economic-contract expiry stage 已从 immutable view 固定到期 atomic contracts、contract-id 顺序、status-derived reputation delta 与 event bodies；governance proposal finalization 随后从 immutable post-economic state 固定排序后的到期 proposal keys，以及由 votes、quorum 和 threshold 推导的 winner、weight、passed 与 event bodies。crisis lifecycle 再从 post-governance state 准备至多一个 deterministic auto-spawn event，发布后才从 resulting state 固定按 crisis-id 排序的 timeout event vector；不得让 timeout preparation 绕过 spawn 的顺序边界。war conclusion 使用更窄的 one-event decision：每次从当前 post-crisis state 准备 `war_id` 最小的到期 active war，发布后重读 resources/reputation，再准备下一场，以保证 contract expiry 与较早 war outcome 都进入后续 scoring/settlement。active gameplay-module lifecycle directive 在 envelope decode 后也逐条从当前 immutable state 转换为 zero-or-one domain event，发布后才处理下一条；`WarConclude` loser fallback 在该 preparation 时解析，zero-point `MetaGrant` 继续无事件，malformed envelope 继续走既有 module failure。这些 stage-local prepared decisions/vectors 的多事件失败仍由外层 cloned-step rollback 收口，不构成 whole-cycle 或 root transaction 原子性证明。

目标实现必须以一个显式的 staged transition boundary（可称 `ExecutionTransaction` / `TransitionBuffer`，具体类型由 runtime 实现决定）承载 parent state、logical time、资源 reservation、module state、pending effect、tick schedule、journal/event 与 sequence counters 的暂存值。module call、Kernel preflight 和 output/schema/capability 校验只能读写这个暂存视图；不得在 commit 前直接修改 canonical `WorldState`、canonical journal 或外部 effect 队列。所有成功 event/effect/state 变更与 execution commitment 在一个 commit 点原子发布；invariant、预算、artifact、receipt 或持久化失败只有在确认业务候选未提交时才丢弃暂存值，并只留下一个稳定的 rejected/fault disposition（若需要审计记录，也必须与该 disposition 同一原子提交，不能留下半个业务效果）。已提交与提交未知的结果遵守[普通 generation 合同](#runtime-generation-persistence-contract)，不得追加与持久结果矛盾的无效果终态。

这样可以让 module 成为一等执行负载，同时保留现有 `WorldState`、legacy event 和 snapshot 的兼容读取；本节不要求立即把当前 struct 改造成传统 ECS。原子边界落地前，Alliance/EconomicContract 试点只能作为 target migration work，不得把现有逐事件实现包装为已完成的原子 receipt 合同。

#### 能力状态口径与 Institution Migration Test

为避免把“接口存在”误报为“端到端能力已证明”，本 PRD 对架构能力统一使用以下状态词：

| 状态 | 含义 | 证据要求 |
| --- | --- | --- |
| `current` | 已存在于当前生产/兼容路径，并有对应代码或现行回归覆盖。 | 代码、现行测试或持久化格式可直接定位。 |
| `partial` | 仅部分入口、部分生命周期或部分数据范围满足目标边界；其余路径仍是 legacy/compatibility。 | 必须明确缺口所在的入口、身份范围或消费者。 |
| `target` | 本文要求的未来合同或迁移终点；不能作为当前可用能力或发布结论。 | 有明确不变量、迁移顺序与失败语义。 |
| `proven` | `target` 已由同一输入的执行、拒绝、快照、恢复和回放证据共同证明。 | 必须有可重跑 fixture/测试及可定位的 receipt、state-root 或 migration evidence。 |

当前边界按此口径归类如下：

| 能力面 | `current` / `partial` | `target` | `proven` 判据 |
| --- | --- | --- | --- |
| Kernel / Institution | L0/L1 约束和 L2 module bridge 已存在；当前互斥分类为 `Alliance=Compatibility keep`、`War=Defer`、`Governance=Kernel keep`、`EconomicContract=Compatibility keep`，尚未选定唯一 pilot。 | 治理批准的 activation boundary 在 Alliance/EconomicContract 中择一转为唯一 `Module candidate`，不新增 Kernel 专属 schema 或制度字段。 | Institution Migration Test 全部通过，且未选项、War 与 Governance 不被隐式改类。 |
| 统一 transaction | trusted capability command 与公开 `step()` / `step_with_modules()` / committed-context tick wrapper 已有 staged 发布边界和失败不发布回归；内部阶段、direct command、install/upgrade、恢复/迁移及外部 effect 仍未统一。 | 所有有 world effect 的 command/tick 入口共享 `ExecutionTransaction`（或等价 staged boundary），单点提交或稳定拒绝。 | native 与 module 入口在相同 parent/input 下产生等价 commitment、journal、snapshot 与 replay 结果，并覆盖 commit 前失败无半个效果。 |
| module instance | install、upgrade、tick、event 与持久化已保留稳定 instance identity；direct/trusted command、相关更新事件和 catalog 仍存在 `module_id` 全局寻址缺口。 | 授权、执行、state、事件、receipt 与 machine catalog 以稳定逻辑主键 `(world_id, module_id, instance_id)` 定位，并单独绑定当前 `artifact_hash/schema_version/activation_epoch`。 | 双实例不串写；同一 instance 升级后逻辑主键不变，历史 binding 可回放且不被当前 artifact 覆盖。 |

首个 Institution Migration Test 必须遵循本文档顶部所链接 product authority 的 SC-32：只有治理批准的 activation boundary 能在 Alliance/EconomicContract 中择一转为唯一 pilot。稳定 fixture contract ID 为 `institution-migration-v1`；每次证明产出 content-addressed manifest、input/root、execution receipts、snapshot/replay/recovery report 和 legacy-conformance report。runtime fixture 只负责生成 immutable artifacts 与 digest；负责人将 digest 记录到 PR 与实际验证记录，专业评审检查真实结果。最低验收合同为：

1. 模块具有受治理的 manifest、artifact hash、schema/version、稳定 `instance_id` 和 activation 状态；缺失或不匹配时不得执行。
2. command 经过同一权限、预算、quote/resolution、最终 affordability 和 Kernel apply 管线；module 不得直接写 canonical `WorldState`、journal 或外部 effect。
3. accepted outcome、事件、module state、receipt 与 checkpoint 使用同一 root/commitment；提交前的校验、预算、持久化或 artifact 失败，在确认未提交时产生稳定 reject/fault，且不留下部分业务效果。持久提交结果未知时先核对原候选；已提交结果不得因后续维护失败改判为无效果。
4. snapshot restore、canonical journal replay、restart/recovery 和跨 adapter conformance 对同一 accepted input 产生相同 state root/receipt；旧 compatibility shape 仍可显式读取或通过 adapter 迁移。
5. 至少建立两个同 artifact、不同 `instance_id` 的实例，证明 direct/trusted command、state update、event、receipt 和 catalog 不以全局 `module_id` 串写或覆盖。
6. preview、stale provider response、deny 与 no-effect 失败不 debit；accepted effect、debit 与 receipt 同一次且仅一次结算；retry/reconnect/restore/replay 不重复扣费、不发放 replay credit，并保留 owner/payer 可读性和旧玩家结果。
7. commit 后外部 effect 先写入 durable outbox/effect ledger；`effect_id` 在 receipt domain 下由 effect kind、canonical target/payload 与 effect ordinal 确定性派生，`(world_id, execution_receipt_id, effect_id)` 作为幂等键，绑定 descriptor、roots、intent hash、dispatch/ack 状态与 external receipt。同键同 payload 重投必须去重；同键不同 descriptor/intent/payload 必须以 `effect_id_conflict` fail closed 且不得覆盖或投递。crash/restart 可重投未确认记录，canonical replay 不得重执行外部业务效果。

Migration Test 未通过前，native vertical slice 只能标为 `current`/`compatibility keep`，不能标为已完成 open-institution extensibility；测试通过也不改变产品规则或玩家承诺的权威归属。

#### ExecutionTransaction 的优先级与范围

统一 `ExecutionTransaction` 是 runtime 架构 **P0**：这里的 P0 表示必须先确立统一状态转换边界，不等同于已经发生线上事故或要求一次性重写所有状态结构。它的范围包括 `step()`、`step_with_modules()`、native compatibility action、WASM/module command、tick directive、direct/trusted command、module install/upgrade 以及任何会改变 canonical world 的恢复/迁移写入。只读 quote/resolve 可以在 transaction 之前运行，但必须绑定 parent、manifest、input root 和 freshness，且不得产生 world effect。

Transaction 的最小职责是暂存 parent state、logical time、resource reservation/debit、module instance state、pending effect、tick schedule、journal/event 和 sequence counters；module call、Kernel preflight、schema/capability/output 校验只读写暂存视图。成功路径在一个 commit point 原子发布 state/event/receipt 与 durable outbox records；只有确认未跨相应提交点（持久路径为 `NotCommitted`）的失败路径才能丢弃暂存值并发布稳定的 rejected/fault disposition。已提交或提交未知时按[普通 generation 合同](#runtime-generation-persistence-contract)安装或核对原结果，不恢复旧 World 的写能力。外部不可回滚副作用只能在 commit 后由 outbox 按 receipt 驱动；必须有持久幂等键、dispatch/ack 状态、crash-safe 重投和 replay 不重执行合同。该边界不要求本轮引入 ECS、shard、动态 World Database 或替换现有 snapshot shape。

目标实现固定为显式 `ExecutionTransaction` + typed `TransitionBuffer`，而不是把 cloned `World` 固化为生产事务。每个 world-effecting public entrypoint 建立一个 root transaction；nested event、module routing 与 reducer 共享同一 buffer，只能用 savepoint 局部回滚，不能 inner commit。Buffer 至少覆盖 canonical state/runtime authority、所有 rolling sequence 与 era、journal batch、pending/inflight queue、receipt link、schedule、capability authorization/budget、commitment/consensus、replay-deterministic metrics、idempotency 与 durable outbox record。prepare 必须完成所有可能失败的校验、容量、commitment、serialization 与 persistence staging，并绑定当前 state/manifest/registry/journal/queue/consensus/authorization head；只有不可失败的 `PreparedCommit` install 可以越过 commit seam。提交前且确认未提交的失败不得产生 event-id 空洞、消费 pending action，或污染任何 deterministic projection；持久提交结果未知不能以保留旧内存作为未提交证明。

执行结果区分 `Accepted`、`Rejected` 与 `Faulted`。`Rejected` 只表示确定性输入、权限、预算、资源或 freshness 拒绝；`Faulted` 表示 trap、artifact/schema、invariant、serialization 或 persistence fault，不得伪装成可继续的业务拒绝。执行 disposition 与持久发布结果分别判定：commit uncertainty 必须保留为 `StatusUnknown`，不能仅凭 `Faulted` 或 infrastructure error 推断无效果。只有确认业务候选 `NotCommitted` 后，协议要求的拒绝或 fault 才能在丢弃业务 delta 后作为同一 root commit 的唯一无业务效果记录；若连该稳定 disposition 也无法形成，且确认其同样未提交，则返回 infrastructure error，并保证 canonical state、journal、sequence、queue/schedule、receipt、commitment 与 deterministic metrics 完全不变。任何已提交的业务或 disposition 均保留原结果；未知结果先核对，不追加矛盾终态。迁移不得改变现有入口在多错误同时存在时的对外错误优先级。

Phase 1 的首个 stateful queue 切片固定 `emit_effect` 语义：capability missing/expired/kind-not-allowed 在 intent/event 分配前拒绝且不写 audit；确定性 policy deny 只原子提交一个 `PolicyDecisionRecorded(Deny)`，intent 与 event allocator 各前进一步，不产生 `EffectQueued`；policy allow 后的 hard queue-full 或 post-prepare infrastructure failure 必须整体丢弃 allow audit、effect event、intent/event sequence 与 era、queue/eviction metrics、journal 和 consensus。满队列仍保留既有的“可确定性驱逐未绑定 intent”兼容规则，authorization-linked intent 不可驱逐。通用 raw `append_event` 的 `EffectQueued` 与 `ReceiptAppended` 也已进入 typed sidecar replacement：pending/inflight queue 与 eviction counter 在 owned projection 上先完成，unknown-intent 仍先于 post-prepare failpoint，随后才与 event allocator、journal retention 和 consensus 一次安装；失败不消费或驱逐 live intent。该切片仍不代表 durable outbox 或 replay allocator 已迁移完成。

后续 Phase 1 receipt 切片把 public `ingest_receipt` 从生产 `World` clone 迁移为 typed prepared delta：known-intent 校验、可选 `EffectReceiptCommitted`、authorization audit/link/root、签名锚点、pending/inflight 消费、`ReceiptAppended`、event sequence/era、journal/backpressure 与最终 consensus 必须一次 prepare、一次不可失败 install。任一签名、授权、consensus 或 post-prepare failure 均不得关闭 link、消费队列或写入 event。保持既有 receipt DTO、事件顺序、`CausedBy` 与 replay reducer 兼容。由于当前 DTO 尚无 `(world_id, execution_receipt_id, effect_id)` 及 descriptor-hash ledger，本切片不把 duplicate-same receipt 或 conflicting receipt 声称为已完成幂等；该 authority/schema 与 durable outbox ack 属 Phase 3。raw `EffectReceiptCommitted` 另以完整 event body 绑定两张 map 的 prepared delta，保持 blank/link/idempotent/auth/audit 顺序和任意非空 effect receipt id，并借用其余五张 authorization map 计算 full root；它不消费 queue、不签名或追加外部 receipt，不能替代 specialized `ingest_receipt`。

首批 CapabilityAuthorization 有状态切片覆盖 public grant registration、invocation-context、budget-account、agent-identity installation 与 proof-bearing authority/revocation administration：System subject 所需的可选 `SystemIdentityInstalled` 与必需 `InvocationContextInstalled` 作为一个 typed prepared batch；`BudgetAccountInstalled` 复用该 batch 并增加 typed budget-map projection；`GrantRegistered` 同步预演 canonical grant JSON map；`AgentIdentityInstalled` 在同一 batch 内预演 revocation-state identity projection；`AuthorityInstalledWithProof` 复用 canonical reducer transition validator，并同时预演 authority record/proof、revocation、supersession 与 finalized-receipt projection，revoke/supersede/trust-root rotation 委托到该 seam。raw `CommandCommitted` 另以完整 event body 绑定 replay-equivalent prepared delta，在 current state、manifest、journal head、context、authority 与 durable effect queue 上维持 reducer 错误顺序，只拥有并预演 grant、nonce、authorization receipt、budget 与 effect-link 五张会修改的 map；revocation/context 两张 map 以借用方式透传 full authorization root，所有 late link 校验完成后才随 event 一次安装。该 raw 路径不执行 command，也不替代 live command 的 `TrustedCommandStage`。以上路径都必须预演授权 map、event id/era、journal/backpressure、最终 authorization root 与单个 tick-consensus candidate，再以不可失败步骤安装。post-prepare failure 不得留下 grant/system-or-agent identity/context/budget/authority 半写、event-id 空洞或 root/consensus/metric 污染；相同对象的重复安装继续保持既有语义，已提交事件继续使用既有 replay reducer 与顺序。trusted capability command 也已从整 `World` clone 迁移为 borrowed-base typed stage：只暂存 command 可触及的 module state、agent/resource、pending effect、intent/event allocator、journal/backpressure、cache 与 consensus，并在同一 prepare 中校验 budget、grant、nonce、receipt、effect link、state/authorization root；post-prepare failure 必须保持 sandbox output、扣费、receipt、effect、event id、journal 与 consensus 全部不可见。该切片仍不是所有 nested command/event 共享的 root transaction，也未闭合 durable receipt/outbox/idempotency、generation persistence 和 replay/restore，因此 CapabilityAuthorization 及整体 ExecutionTransaction 能力仍为 `partial`。

每个可重试 root operation 必须用稳定 operation id 绑定 world、parent、canonical input/intent hash、manifest/activation 与 target。同 identity/同 binding 返回原 disposition，不再分配 event、扣费、入队或发出 outbox；同 identity/不同 binding 以 idempotency conflict fail closed。临时 event id 不是重试身份。外部 effect 仍以 `(world_id, execution_receipt_id, effect_id)` 去重，只承诺 durable outbox + at-least-once dispatch + stable idempotency，不承诺外部系统物理 exactly-once。

Replay 使用同一 reducer 和 typed buffer，但只能应用 canonical event，必须整体校验并一次发布 journal suffix；它不得生成 event 或调用 sandbox、LLM、adapter、dispatcher，只能重建 ledger/outbox 状态。持久化以完整 immutable generation 为单位，在 snapshot、journal、module store、sidecars、outbox、manifest 和 hashes 全部 stage/fsync/validate 后原子切换 latest-generation pointer；该 pointer switch 连同其承诺的持久化确认构成 durable commit point，替换可见但目录同步等持久条件未确认时仍是 `StatusUnknown`。确认未提交的失败保留旧 generation；确认已提交后以新 generation 恢复，后续镜像、GC 或 metrics 维护失败不撤销提交，禁止拼接 mixed generation。receipt ingestion 是独立 root transaction，必须原子完成 receipt 校验、authorization/intent link closure、queue/outbox 更新、业务 apply、receipt event 与 commitment/sequence 更新。

实施按 Phase 0 加五个可独立验收的迁移阶段推进：Phase 0 冻结 typed delta/savepoint/disposition、deterministic projection、failpoint 与 test-only clone oracle；Phase 1 迁移通用 `append_event` 及 reducer/sequence/journal/schedule/queue/consensus；Phase 2 迁移 step/action/direct/trusted/lifecycle/module/tick/observation/capability；Phase 3 迁移 receipt/outbox/idempotency；Phase 4 迁移 replay/restore；Phase 5 建立 generation persistence/crash recovery。任何 public mutation surface 尚未迁移时能力仍为 `partial`；全部入口实现但完整 crash/replay/ABI 证据未闭合时只能称 `target-implemented`；只有执行、拒绝、fault、幂等、回放、恢复、outbox、持久化、legacy serde 与 WASM compatibility gate 全部通过后才能称 `proven`。

<a id="runtime-generation-persistence-contract"></a>
#### 普通 generation 保存与恢复合同（目标）

本专业条款承接以下产品义务的适用部分，保留原文技术责任与未证明范围：

- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)：完整同代候选与历史恢复连续性；本地保存只是恢复链的子边界，不证明正式可服务或独立故障域。
- Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[REQ-DWE-001](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)：持久结果与调用方一致，已提交或未知时不伪造无效果终态；不代替全部验证者的确定性组合。

上述关系不表示产品全部义务已覆盖、实现或验收；本条工程前置、目标协议及外部依赖仍按原文区分。

普通 runtime 保存必须向调用方区分 `NotCommitted`、`Committed` 与 `StatusUnknown`，并独立表达提交后的维护降级。候选绑定稳定操作身份、原 base generation 与完整持久内容；确认已提交时安装或恢复该候选，未知时禁止旧 World 再发布，按同一身份核对后才解除写入隔离。此处描述本地 generation 的持久结果，不代替[合法 authority profile 的正式提交证明和独立故障域条件](#runtime-authority-profile-commit)。

专业要求和独立验收由存储专题的 [G1 提交判定与调用方一致性](runtime/runtime-storage-footprint-governance.prd.md#storage-generation-g1)、[G2 当前 index 引用闭包保护](runtime/runtime-storage-footprint-governance.prd.md#storage-generation-g2)、[G3 完整同代候选恢复](runtime/runtime-storage-footprint-governance.prd.md#storage-generation-g3)及[提交判定与恢复选择边界](runtime/runtime-storage-footprint-governance.prd.md#storage-generation-cross-boundary)承接，技术映射见[根设计](design.md#runtime-generation-persistence-design)。健康旧候选不证明新代未提交，也不自行恢复当前写权；涉及事件后缀时，完整候选还须满足 [J1/J2 恢复与续跑合同](#runtime-journal-suffix-recovery)，不能仅由同代材料可读推断控制状态已正确重建。重新服务仍受[恢复闸门](#runtime-recovery-acceptance)约束。文档采纳不证明普通保存入口或这些故障验收已实现。

#### Command-path module-instance completeness

实例的稳定逻辑主键是 `(world_id, module_id, instance_id)`；`artifact_hash/schema_version/activation_epoch` 是该主键在某一 execution height 的 active binding，不重定义逻辑实例。当前安装、升级、tick、event routing 与恢复已携带部分实例信息；仍需补齐 direct/trusted command 的 target、state lookup、更新事件、receipt linkage 与 catalog target。

目标路径必须验证逻辑主键与当前 artifact/schema/activation binding，再把 target 与 binding digest 固定进 staged command、event、receipt、snapshot 和 replay input。升级必须产生按 execution height 可回放的 version-lineage record；restore/replay 使用历史 binding，不以当前 artifact 覆盖历史。任何不一致在 first effect 前拒绝，禁止 fallback 到另一个同 `module_id` 实例。

具体入口解析、单实例兼容与双实例拒绝/隔离验证由 [实例命令设计](design.md#module-instance-command-design) 承接；实现与验收继续在 [#3555](https://github.com/eng-cc/oasis7/issues/3555) 独立跟进。文档补齐保留本能力的 `partial` 状态，不把原子发布、历史局部测试或汇总 Issue 关闭视为实例隔离已经完成。

### 单一内部 execution pipeline

native、WASM、tick module 和 compatibility fallback 必须最终进入一条内部确定性 pipeline。外部 `step()`、`step_with_modules()` 或 dev/committed wrapper 可以继续存在，但只能表达输入适配、模块集合或持久化模式差异，不能产生不同的状态转换语义。以下是目标 pipeline 的阶段顺序；它把需要模块计算的动态成本与最终 Kernel affordability check 分开，避免把尚未知道的 module cost 假装成 preflight 已知事实：

1. 解析并校验 world/runtime manifest、parent committed state 与 action envelope。
2. 以确定排序建立 input action root；拒绝重复、越界、未授权或无法解释的输入，并把 parent state、manifest 与 canonical action envelope 绑定到 quote 请求。
3. 执行不改变 canonical state 的 deterministic module quote/resolve preflight。该阶段可以调用 module 来解析动态 material/cost/duration，但只能返回受限 quote/resolved-action buffer，不得 debit、credit、持久化 module state、发出业务 event 或排入外部 effect。
4. 将 quote/resolved action 固定进 staged transition，执行最终 L0/L1 物理、权限、资源 affordability、预算与 quote freshness 校验，并在暂存视图中 reservation/debit；quote digest、resolved-action digest 与 input action root 一起进入 execution commitment。
5. 通过同一 dispatcher 路由 native compatibility action、已绑定 quote 的 module command 与 tick directive。
6. 在受限上下文中执行 module，收集结构化 event/effect/output；所有结果只写入 staged transition，不得直接持久化 canonical state。
7. 由统一 Kernel apply 依次应用 staged event/effect/state，形成下一状态；任一 invariant failure 原子丢弃整个 staged transition，并产生稳定的 rejected/fault disposition。
8. 按确定顺序运行已启用的系统/tick schedule，并把 schedule 变化纳入同一 staged transition。
9. 计算 event/state/执行 commitment，原子写 canonical journal、receipt 与必要 checkpoint。

`step_with_modules()` 不是绕过 native 规则的特殊真值；当前 gameplay module 激活时的整体 directive/fallback 语义继续兼容。wrapper 与 `step()` 现已共享“clone 后执行、成功才发布”的 coarse staged boundary，但内部执行顺序仍是 legacy/current compatibility behavior，不能宣称已经满足上述统一 pipeline。迁移完成后，quote/resolve、final affordability、atomic apply、receipt 与 replay conformance 必须证明两个入口的状态转换等价；在证据闭合前，ExecutionReceipt 只属于 target contract。后续拆分 `WorldExecutor`、`KernelSystem`、`PhysicsSystem`、`ResourceSystem`、`ModuleSystem`、`JobSystem`、`CommitSystem` 应按阶段逐步完成，保留固定顺序与既有 replay fixture；不进行一次性 ECS 或 WorldState 大迁移。

<a name="executionreceipt、tick-compatibility-与-finality-边界"></a>

### ExecutionReceipt、tick compatibility 与 finality 边界

目标状态下，world-runtime 的权威输出是 deterministic `ExecutionReceipt`/execution commitment，而不是共识证书；在迁移完成前，现有 execution record 与 tick compatibility record 仍是 current/legacy surface，不能被描述为已完成的 receipt contract。receipt 至少绑定 `world_id`、runtime/manifest version、parent state identity、确定排序的 `action_root`、执行/事件 commitment、next `state_root`、逻辑 tick/执行高度、journal/checkpoint references，以及 accepted/rejected/fault 的结构化结果。字段的具体编码和签名方式由对应接口/共识专题约束；runtime 不以本地 threshold 或 process-local 签名宣称 finality。

proposer 可以提交候选 receipt；active validator 必须从同一 committed parent state 重执行同一 action sequence，并比较 action root、execution commitment、event root 与 next state root。投票、锁定、round/view-change、stake threshold、partition recovery 与 `FinalityCertificate` 属于 p2p/consensus authority。runtime 只持久化可复现执行产物，并在 mismatch、缺失 artifact、超限或 fault 时阻断提交，不能以本地修补越过差异。

现有 `tick_consensus_records`、`TickBlock`、`TickCertificate` 是 legacy compatibility records：可用于旧 snapshot、replay 与诊断，但不再被解释为分布式 finality。迁移按以下顺序进行：

1. 冻结旧记录的语义边界，禁止新增消费者把本地 threshold-1 certificate 当成 p2p finality。
2. 为同一输入建立 receipt/legacy record conformance fixture，并在迁移窗口同时保留可回读的旧字段与新的 execution commitment。
3. 将新的 runtime/replication reader 切换到 receipt；旧字段变为只读兼容投影，直到 retention window 与所有消费者完成切换。
4. 只有在版本化 snapshot/replay migration、receipt equivalence 和下游读者证据齐全后，才允许停止生成 legacy 写入；删除旧读取路径另行走兼容周期。

迁移窗口内，`WorldTick`、consensus height 与 wall-clock slot 可以继续沿用当前实现的兼容耦合，但文档与 receipt 必须能区分三者。未来批量 tick、快进、停滞恢复或逻辑 region 不得借此隐式改变 single canonical timeline；解耦应通过版本化 execution protocol 与 replay fixture 另行验证。

<a name="snapshot、replay、version-与-canonical-timeline-兼容"></a>

### Snapshot、replay、version 与 canonical timeline 兼容

- 以下是目标 snapshot/replay contract；当前 `Snapshot` 的既有 `manifest`、`state`、`journal_len` 与 legacy tick records 仍是 current/compatibility surface，不等同于已经具备完整 ExecutionReceipt identity。
- 当前目标版本固定为 `world-runtime-snapshot-v2`。snapshot 必须保留 runtime/manifest version 与 hash、`world_id`、parent state identity、state root、height/tick identity、module instance identity/version/artifact hash/schema version/activation、module state、tick schedule、journal/checkpoint references，以及 `compatibility_marker=world-runtime-snapshot-v2`。新字段可以使用 serde-defaulted 读取以发现旧数据，但缺失/空 marker 只能进入显式 legacy adapter，不能默认满足 v2 合同；旧目录不得静默替换 artifact、parent state 或 canonical state。
- legacy adapter 版本固定为 `world-runtime-snapshot-legacy-v1`，迁移记录版本固定为 `world-runtime-snapshot-migration-v1`。adapter 必须记录来源 snapshot digest、可重建的 world/parent/tick/root 证据、module artifact/schema identity 与迁移 marker；无法重建或验证任一 identity 时返回结构化 `compatibility_fault`，不得把旧 snapshot 暴露为可继续执行的 current state。schema、runtime 或 module 升级必须先验证 artifact/interface/owner/namespace compatibility，再原子写入 registry、instance state、journal 与 migration marker；失败时不产生部分升级，历史 receipt 不重写。
- replay 以 checkpoint + canonical journal 重建，在每个 receipt 边界比较 action/event/next-state commitment；遇到首个差异、缺失 artifact 或旧 schema 不可解释时返回结构化 mismatch/fault，保留原始证据并阻止不一致状态重新对外暴露。
- retry、restart、restore、GC 后恢复与跨 adapter replay 必须对同一 accepted input 生成同一 receipt/state root；被拒绝 action 也必须有稳定 disposition，不能静默丢失。v2 restore 在重新对外暴露 state 前必须比较 snapshot root、replay root 与 next state root；legacy adapter 也必须产出同等比较证据，否则只允许诊断读取，不允许继续提交新效果。
- 当前产品与 runtime 仍只有一条 `world_id` 对应的 canonical world timeline。dev/local 或测试 world 必须使用独立 `world_id`，不得把离线历史合并回 global world。region/partition 先作为逻辑 scope 或存储/调度边界，独立 shard finality、跨 shard commit 与动态 World Database 均暂缓，不构成当前实现承诺。

<a id="runtime-journal-suffix-recovery"></a>
### 事件后缀恢复与续跑（目标）

本条承接产品 [恢复可服务验收](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005)。
合法 finalized checkpoint `C<T` 加完整、非空的 `WorldJournal` 事件后缀恢复到 T 后，必须保留决定后续执行的控制状态。
完整 T snapshot 往返、WorldJournal 事件归约、committed-input 重执行是三种独立验证路径；后两者的输入与执行边界见 [系统设计](design.md#runtime-journal-suffix-recovery-design)。

- **J1 — action allocator：**恢复 `next_action_id` 与 `action_id_era` 及相关预分配队列边界，接续分配与不中断对照一致；下一合法 recipe 不得复用后缀中已 settled 的 job identity。
  记录来源、单动作多事件、业务拒绝、显式/预分配 ID 与 rollover 由 [数值恢复合同](runtime/runtime-numeric-safety.prd.md#numeric-action-allocator-replay) 承接；HTTP producer 的独立命名空间不在本条范围。
- **J2 — module tick schedule：**每次成功 tick 的消费、重排或停止调度结果须可由记录重建；恢复后的 schedule、下次调用时点与不中断对照一致。
  WakeAfterTicks、Suspend、缺省、Pure/无 new_state 与双实例隔离由 [宿主 tick 重放设计](runtime/runtime-integration.md#module-tick-journal-replay) 承接。
- **正向续跑：**同候选、同 world、相同合法 authority/manifest/权限/资源及后续输入下，两分支比较 disposition、动作/job identity、唤醒次序、状态与费用；至少一个合法新 recipe 正常完成一次。
  单独的 WorldState/root 一致或“效果至多一次”不代替该结果；这不是无条件成功或完成时限承诺。
- **恢复边界：**普通加载、既有 signed rollback v2 的 C→T 路径及 repair 的 source-journal 重建采用同一控制状态语义；保留既有签名、artifact、finality、服务闸门及失败原子性。
  恢复结果需要持久保存时，提交结果按[普通 generation 合同](#runtime-generation-persistence-contract)安装或核对；只有确认未提交才允许丢弃该候选并继续使用旧 World，未知结果保持写入隔离。
  event replay 只应用已记录事实，不重新执行 action/WASM、不重新推导费用或派发外部效果；J1/J2 独立验收，不互相代签。
- **旧历史：**可反序列化不代表可继续执行；必要控制记录缺失时采用 [显式兼容矩阵](design.md#runtime-journal-suffix-compatibility)，不得把无记录解释为未分配或 suspend。
  本条和 [验证矩阵](design.md#runtime-journal-suffix-validation) 是目标合同，现有 snapshot/局部事务测试不构成非空 suffix 已通过的证据。

### Consumer compatibility: industrial profile and stage execution

- `MaterialProfileV1`、`ProductProfileV1`、`RecipeProfileV1` 与 `FactoryProfileV1` 是 ABI 类型；对应目录随 `WorldState` 持久化，治理变更通过可回放事件生效。字段结构以 `oasis7_wasm_abi` 为准，不在产品文档复制代码清单。
- 物流优先级按“动作显式值 > material profile 默认值 > 兼容关键词推断”解析；运输损耗、排产门槛、preferred factory 与瓶颈标签由当前 runtime 规则执行，并把决定结果冻结进事件或 job 状态以维持回放确定性。
- recipe `stage_gate` 与 product `unlock_stage` 可阻止高于当前 `IndustryStage` 的排产。阶段由当前设施能力重新计算，设施回收或损失后允许回退；不得把阶段描述为只升不降的永久等级。
- 非空 bottleneck profile 优先于推断值；缺失或空配置走兼容推断。产品战略角色与瓶颈压力共同影响排队优先级，准确映射和 fallback 以 runtime 代码与定向测试为执行真值。
- 上述执行能力不证明玩家侧 `product_validation_quote` / `validation_unlock_preview` 已实现；玩家用途、能力解锁、阶段后果、下一步与门槛恢复路径由 gameplay 权威定义并保持独立验收。

### Industrial execution status and authority matrix

本表是 current/legacy/target 的状态与权威索引，不是第二套工业协议。`world-runtime` 拥有 event、receipt、job、replay、ID/schema 的当前事实与目标执行合同；[`M4 industrial resource flow contract`](../world-simulator/m4/industrial-resource-flow-contract.prd.md) 拥有工业 domain 语义；[`gameplay top-level design`](../game/gameplay/gameplay-top-level-design.prd.md) 与产品 PRD 拥有玩家承诺。首次新增任一 root/join/window/terminal/bundle 字段或权威建设电力报价时，必须刷新本表并链接 replay/receipt evidence；在对应证据存在前，目标项不得写成当前实现。

| 术语 / surface | 历史兼容 | 当前 runtime 事实 | 目标 / 未决 | 权威与证据 |
| --- | --- | --- | --- | --- |
| Direct material transfer / immediate | 空 route/path binding 且 `auto_reroute=false` 时，zero-distance 保留 direct compatibility；同一空 binding 下 positive-distance remains existing unbound transit。显式 route/path binding 或 `auto_reroute=true` 请求 graph mode，只有 path validation/selection 成功才成为 path-bound transit（即使有效路径距离为零）；invalid explicit binding 或无可用 graph selection 必须 atomic reject；unavailable explicit binding 只有在 `auto_reroute` 选出另一条有效 graph path 时才能替换，绝不 fallback 到 direct。该 compatibility lane 不替代 gameplay authority 对玩家层 co-location/eligibility target 的定义。 | `MaterialTransferred` 直接 debit/credit 并记录以 `transfer_id` 为键的 receipt；同一 receipt 的 replay 是 idempotent，mismatch rejects。`MaterialTransitStarted` debit material、创建 pending job、reserve selected routes、escrow tariff；`Completed` 校验 identity/conservation/loss、credit received amount、release reservation，并 once-only settle tariff/receipt。当前只有通用 `WorldEvent.caused_by = Action(action_id)` trace，没有 operation/root 字段。 | 定义 direct-handoff operation identity 与 settlement/finality 映射，同时不伪造 path identity。 | runtime action/event/tests；gameplay direct-transfer compatibility；M4 handoff/settlement。 |
| Factory maintenance / depreciation | `FactoryMaintained` legacy event shape 没有 operation 字段；自动 `FactoryDurabilityChanged` 是 system effect。 | action 消耗 parts 并改变 durability；submitted action 可有 generic cause，自动 depreciation 可为 `caused_by=None`；尚无 maintenance status/quote/window/root。 | 分离 maintenance quote/status 与 sink；若维护影响工业 run，则把 effect 绑定 root/segment/window，并保持 system depreciation 的确定性 replay。 | runtime event/apply/replay；产品维护合同；gameplay 维护选择。 |
| Multi-stage job / root | 现有 `job_id: ActionId` 是 per-action/job key，不是 canonical root。 | pending maps 以 `ActionId` 为键；没有 root/revision/segment 或 parent/child links。 | accepted outcome 签发 immutable root；checkpoint/non-causal revision 保持 root，causal cutover 建立链接 child root；closed-root late effect 服从 pending/compensation。 | `PRD-WORLD_RUNTIME-043` 目标合同；M4 lineage/cutover；产品因果承诺。 |
| Input join | 没有 dedicated `join_id` / parent-edge receipt-set schema；多个 `consume` stack 不构成 join identity。 | 显式 `logistics_path_ids` 已具备 settled quantity 与 remaining recipe authority，按排序 `path_id` 原子分配/扣减；无 path binding 继续走 aggregate-ledger compatibility。当前 schema 仍不能表达 required parent set、batch/custody lineage 或完整 join remaining obligations。 | canonical join ID、稳定 parent set、atomic default、staged-intake opt-in 和单次 parent disposition。 | M4 join domain contract；`PRD-WORLD_RUNTIME-043` 执行目标；bounded path-quantity implementation evidence。 |
| Service window / terminal | recipe/transit 的 `ready_at` 或 output ledger 不是 service-window、lease、terminal 或 delivery identity。 | 当前事件没有独立 window/lease/terminal/delivery finality；production output 不能据此视为 delivery settlement。 | 独立 window identity、reservation mode/disposition，以及 production 与 delivery settlement 分离。 | M4 window/terminal domain contract；gameplay 玩家承诺；`PRD-WORLD_RUNTIME-043` 执行目标。 |
| Output bundle / branch | `produce`/`byproducts` vectors 与 output ledger 没有 bundle/branch identity。 | 当前不能表达 atomic-vs-split disposition、branch pending/residual 或 branch receipt/finality。 | 单一 bundle identity、branch identities、atomic default 或显式 split fan-out，以及逐 branch finality。 | M4 bundle domain contract；`PRD-WORLD_RUNTIME-043` 执行目标。 |
| Factory construction power quote | simulator legacy/current-local config 与 event 有 `factory_build_electricity_cost`；该事实不自动成为 runtime authority。 | runtime `BuildFactory` 校验 material `build_cost` 并进入 pending construction；modern `FactoryBuildStarted` (contract version 1) pins site/location/profile/receipt facts and retains the source ledger, while version 0 is replay-only compatibility. | 最小目标由 [Factory site admission and construction power contract](#factory-site-admission-and-construction-power-contract) 冻结：site/access 持久 authority、builder-held `start-only sink`、resolved obligation revision 与 legacy replay 边界；仍须 fresh runtime/QA evidence 才能写成 current。 | 产品 `BuildFactory` 报价合同；M4 construction profile；runtime event/state/replay；gameplay 选择。 |

### Factory site admission and construction power contract

本节是 `BuildFactory` 的最小 runtime execution contract，承接产品层的站点选择承诺与 M4 的 construction profile 语义；它冻结持久字段和原子边界，但不把本节的 target contract 误写成当前实现。`world-runtime` 拥有 event、state、apply order、persistence、replay 与 exactly-once；M4 拥有建设 profile 的成本/电力语义；产品/gameplay 拥有玩家选择、机会成本与可读性。

#### Persistent authority and backward-compatible fields

最小持久 authority 由以下 versioned records 组成。字段名是 runtime/M4 的实现契约，允许使用 serde default 读取旧 snapshot，但新增提交不得因缺字段而绕过校验。

| record | required fields | semantics |
| --- | --- | --- |
| `AgentLocationAuthorityV1`，存于 `WorldState.agent_location_authorities: BTreeMap<agent_id, ...>` | `agent_id`、`location_id`、`active`、`authority_revision`、`effective_at` | 这是 builder 的 canonical location assignment；`AgentState.pos: GeoPos` 只表示物理坐标，不能推导或替代 `location_id`。缺 assignment、`active=false`、非当前 revision 或 location 不存在时，co-location 为 `unknown/blocked`。assignment 只能由可回放的 movement/location authority event 更新，revision 单调递增。 |
| `FactorySiteAuthorityV1`，存于 `WorldState.factory_site_authorities: BTreeMap<site_id, ...>` | `site_id`、`location_id`、`owner_agent_id`、`authorized_agent_ids`、`chunk_ready`、`active`、`authority_revision`、`registered_at` | `site_id` 是稳定 key，`location_id` 必须引用同一 authoritative location anchor；owner 自动拥有 access，其他 builder 只能命中显式 allowlist；空 allowlist 不表示公开访问；`active=false` 或 `chunk_ready=false` 不可新建。所有 ID 非空，allowlist 排序去重，revision 单调递增。 |
| `FactoryConstructionPowerProfileV1`，存于 `WorldState.factory_construction_power_profiles: BTreeMap<factory_id, ...>` | `factory_id`、`factory_kind`、`source_module_id`（可为 `None` 的 direct profile source）、`electricity_amount`、`mode`、`authority_revision`、`active` | map key 必须与 `FactoryModuleSpec.factory_id` **逐字相等**，不得规范化、从 tag/display name 推导或由 caller 选择另一 key；`factory_kind`、`source_module_id` 与 amount/revision 来自 M4-governed profile authority event/state。module lane 还要求请求的 `module_id == source_module_id`；direct lane 只按 exact `factory_id` 查 profile，不接受 caller 自带 amount。`electricity_amount >= 0`，本批次 `mode` 只允许 `start_only_sink`，inactive/missing/stale profile fail closed。 |
| `FactoryBuildPowerObligationV1`，随 build job/event 持久化 | `payer_agent_id`、`profile_key`（exact `factory_id`）、`profile_revision`、`electricity_amount`、`mode` | obligation 只能由上表 profile 解析产生，不能由 Action、WASM module output、simulator config 或 Viewer 填写；`payer_agent_id` 必须是 builder，`profile_key` 必须等于提交的 `FactoryModuleSpec.factory_id`，`profile_revision` 指向该 profile 的权威版本。`base_power_draw`、maintenance、recipe power 和 battery runway 不能填充此 obligation。 |
| `FactoryBuildJobState` additions | `site_authority_revision`、`site_location_id`、`construction_power_obligation`（旧数据允许 `None`） | pending job 固定开始时的 site identity/location 与已解析的 power obligation；站点后续 metadata/access 变化不得静默改绑该 job。 |
| `FactoryState` additions | `site_authority_revision`、`site_location_id`、`construction_power_profile_key`、`construction_power_profile_revision`（旧数据允许 `None`） | 建成工厂保留其采用的 site authority/location anchor 与 exact construction-profile identity，供后续读取和 replay 对账；不把建设 receipt 解释为 recipe、物流或 terminal readiness。 |

Site registration、location anchor、Agent location assignment、ownership/access 变更和 readiness 变更必须通过 runtime 可回放的 authority event（或等价的已认证 authority input）发生；generic `BuildFactory` caller、Viewer、Agent、simulator config 不能直接写 registry。新 registration 从 revision `1` 开始；更新必须携带当前 revision 并原子产生下一 revision，stale/conflicting update 拒绝。runtime 不从 `site_id` 字符串、Agent 坐标、最近地点或客户端缓存推导 `location_id`、access 或 chunk readiness。co-location 只在同一 fresh snapshot 中同时满足：site active、site `location_id` 引用有效 anchor、builder 的 `AgentLocationAuthorityV1.location_id` 与 site `location_id` 精确相等且 assignment active；不能用 GeoPos 距离或“同一格”近似。

#### Admission and electricity settlement

`BuildFactory` admission 在同一 fresh authoritative snapshot 中按以下顺序裁决：builder 存在 → site registration/active/access/co-location/chunk readiness → factory identity 未 active/pending/retired → 用提交的 `FactoryModuleSpec.factory_id` exact lookup active `FactoryConstructionPowerProfileV1`（module lane 再校验 `source_module_id`）→ construction materials 与由该 profile 解析出的 `FactoryBuildPowerObligationV1` 均可支付。材料 obligation 使用 builder-owned material ledger；global/world material 不能作为新提交的隐式 fallback。Power obligation 使用 builder-held `Electricity`；Location electricity pool、global pool、`PowerStorage`、caller-provided amount 和 `base_power_draw` 都不是 payer。

成功只在一个 staged commit 中同时完成：

1. 持久化 site authority revision、location anchor 和 power obligation；
2. 扣除 construction materials 与 `electricity_amount` 各一次；
3. 写入 `FactoryBuildStarted`/pending job；
4. 后续按该 job identity 只产生一次 `FactoryBuilt`。

任一材料、电力、site/access/readiness、Agent location assignment、profile exact lookup 或 authority revision 缺失/冲突，都必须 atomic reject/`unknown`，不扣任何资源、不建立 pending、不产生设施。`start_only_sink` 不保留 completion hold，也不在 completion 再扣电或自动退款；站点变化不能自动迁移、取消、改路或重算已接受 job。PowerPlant 若未来由 factory profile 声明，必须在 profile 选定的 build-completion/activation boundary 后才生效。

同一 action/job identity 的重试、重连、snapshot restore、乱序和 replay 只能重读原 disposition；相同 payload 是 no-op，冲突 payload fail closed。任何 site/location authority、construction profile 或 power obligation 字段变化都必须进入 event/state root 与 replay 比较，不能只存在于 quote 或日志。

#### Legacy replay and non-goals

- 旧 snapshot 缺少 `factory_site_authorities`、`agent_location_authorities` 或 `factory_construction_power_profiles` 时按空 registry 读取；它只可进入显式 legacy replay/diagnostic adapter，不能凭空获得新建厂资格。旧 `GeoPos` 不得回填 Agent location assignment。
- 旧 `FactoryBuildStarted`/`FactoryBuilt` 事件缺少 site revision 或 power obligation 时，按原事件字段 replay，不能 retroactively 充电、补 site access 或从 simulator `factory_build_electricity_cost` 猜测金额；新事件必须写入上述字段。
- `BuildFactoryWithModule` 的 module output 只能提供候选材料/工期；profile 的 exact `factory_id` key、canonical kind、source module、power amount/revision 与 site/access/ledger 的最终裁决仍回到同一 runtime admission。不得保留 module-only 的 global electricity fallback。
- 本节不实现站点拓扑搜索、路径/容量/损耗算法、relocation/cutover、maintenance/runway、recipe lifecycle、terminal settlement、PowerStorage 或 UI 布局；这些能力仍服从各自 authority。

`test_tier_required` 至少证明：site registration/access/readiness、Agent location assignment/co-location 与 stale revision 的正负样例；材料或 builder electricity 不足时无 mutation；global/Location electricity 不能补贴；exact `FactoryModuleSpec.factory_id` profile lookup、module source mismatch、missing/inactive profile 均 fail closed；start-only sink 与 pending/completion/replay 各至多一次；snapshot/replay 保留 site/location/profile key+revision/power identity；旧事件可读但不能取得新提交资格；direct/module BuildFactory 对同一 authority snapshot 结果一致。`test_tier_full` 再覆盖并发 site update/profile update、重复/冲突 action identity、crash/recovery、PowerPlant boundary 与跨 adapter replay。

### Consumer compatibility: gameplay lifecycle protocol boundary

- Gameplay 生命周期沿用 `validated Action -> DomainEvent -> WorldState` 的 runtime 权威链；动作校验与路由以 `crates/oasis7_runtime/src/world/event_processing.rs` 为准，事件与持久状态以 `crates/oasis7_runtime/src/events.rs` 及 state apply 路径为准。
- 周期性治理、危机、战争与元进度推进由 `crates/oasis7_runtime/src/world/gameplay_loop.rs` 在 tick 中确定性执行；存在激活的 Gameplay tick module 时先校验并应用其 directive，只有没有激活 Gameplay tick module 时才整体使用 native fallback，激活模块后不会因某类 directive 缺失而逐项回退。具体动作名单、阈值、评分和玩家可读后果不在本 runtime 入口复制。
- 内建 Gameplay 模块的注册、`ModuleRole::Gameplay` 绑定与 readiness 由 `crates/oasis7_runtime/src/world/bootstrap_gameplay.rs` 和当前回归测试承接。模块存在或 readiness 通过不替代 gameplay 专业域的可玩性与后果可读性验收。
- Gameplay manifest 必须使用 `ModuleRole::Gameplay` 并携带合法 contract：mode 非空、去空白后唯一，`min_players >= 1`，设置 `max_players` 时必须满足 `max_players >= min_players`；非 Gameplay manifest 不得携带该 contract。register、activate、deactivate、upgrade 都必须对变更后的 active set 预检，同一 `(mode, GameplayModuleKind)` 最多只能有一个 active module；mode 判等统一按[派生 mode key 合同](#gameplay-mode-identity-contract)，不能只在输入校验时 trim。
- `gameplay_modules()` 与 `gameplay_mode_readiness(mode)` 返回确定排序的 active-set / coverage snapshot；readiness 只表示 runtime operational coverage，不是玩家体验、视觉或发布验收。
- Base module governance 负责通用 identity、version、artifact、interface/export、subscription、capability 与 limit 校验，只把 Gameplay contract 和变更后 active-set slot 冲突交给 gameplay layer。这是内部实现职责拆分，不创建新的 ABI 或规则语义边界；公开协议仍以本页和 `doc/world-runtime/wasm/wasm-interface.md` 为准。
- 发生 tick-consensus drift 时，runtime 恢复必须让受影响 action 得到显式处置（保留、replay，或带恢复路径的拒绝），不得静默丢失。可执行 governed rollback v2 的 custody、C/T reconciliation、nonce、journal/root 校验与 all-clear 流程仍由现行 rollback runbook 承载；历史 release `Go`、RTO/RPO 或一次性 soak 结果不得充当当前 release authority。
- 生命周期事件与状态仍受本 PRD 的确定性、receipt、replay、恢复与 ABI 兼容合同约束；持久化保留和恢复细则见 `doc/world-runtime/runtime/runtime-storage-footprint-governance.prd.md`。历史 closure 的实现过程从 Git history 与 PR 与实际验证记录 追溯，不再作为长期 runtime 权威入口。
- 对激活且订阅 `tick` 的模块，runtime 只在 logical time 到期时调用，并将 schedule 随 snapshot/replay 持久化；一次 tick 调用在暂存视图消费旧 schedule，成功时只有模块明确请求 wake 才建立下一次调度，`suspend` 或缺省指令不再调度。整条 route 失败丢弃暂存、保留原 schedule，并返回既有结构化错误/失败审计，不承诺自动重试。ABI wire shape、最小 wake clamp、tick context 与失败约定以 `doc/world-runtime/wasm/wasm-interface.md` 为准。
- 链 PoS 的时间锚、slot/tick 相位、严格滞后恢复、控制面参数、status 与 restart/replay 边界由 `doc/world-runtime/runtime/chain-pos-control-plane.prd.md` 唯一承接；它不定义 validator 经济、节点运维或发布结论。

## 接口 / 数据
- PRD 主入口: `doc/world-runtime/prd.md`
- 当前任务状态与阻断: 对应 GitHub task issue / Project
- 历史根入口: root world-runtime project legacy redirect shell 已删除，不作为当前执行入口。
- 文件级索引: `doc/world-runtime/prd.index.md`
- 追踪主键: `PRD-WORLD_RUNTIME-xxx`
- 测试与发布参考: `testing-manual.md`

### ModuleStore persistence and recovery contract

- Default `World::save_to_dir` / `load_from_dir` persist the module registry, manifest metadata, and content-addressed artifact bytes together. Compatibility `*_with_modules` APIs remain directed callers, not a second persistence truth.
- A legacy directory without a module store loads compatibly. Once registry/meta/artifact files exist, restore verifies their mutual consistency and artifact hash; missing or damaged data must return `ModuleStoreVersionMismatch`, `ModuleStoreArtifactMissing`, or `ModuleStoreManifestMismatch` (or governed recovery), never silently substitute bytes.
- Direct module-store hydration prepares the complete registry, artifact-key set, and artifact-byte map in owned replacements before changing a live `World`. Failure on any sorted persisted record leaves the prior registry, bytes, artifact set, and process-local cache unchanged; successful hydration retains the established cache policy rather than silently resetting it.
- Direct raw `ModuleEvent` and `ManifestUpdated` publication is also typed-prepared: register/upgrade/activate/deactivate project registry, artifact keys, tick schedule, and targeted subscription-cache invalidation into owned replacements, while manifest replacement computes consensus against the replacement manifest without trusting its informational hash field. Validation or post-prepare failure preserves registry/active state, artifacts and bytes, cache, schedule, manifest, allocator, journal/backpressure, and consensus; install is infallible and does not clone `World`.
- Raw controller-registry and validator-admission governance publication prepares the complete event-bound replacement for controller registry, admissions, node identity bindings, and derived main-token account bindings. Its consensus root uses a borrowed projection, so validation and publication failure cannot expose admission or identity state before journal and consensus acceptance.
- Direct raw governance proposal `Approved`, `Queued`, and `Applied` events prepare a full-event-bound proposal replacement before publication. They retain the raw reducer's rejection, queue-validation, and informational applied-hash behavior while keeping the specialized paired approval/queue and governed manifest-application batches unchanged.
- Direct raw capability admission events with replayable authority proof, agent/system identity, invocation context, budget account, or grant now use a full-event-bound prepared sidecar projection. The four admission maps and derived authorization root publish atomically; legacy unbound authority shapes remain fail-closed, while command commits and effect-receipt closure retain their larger specialized transaction boundaries.
- Persisted instances retain module identity/version/hash, owner, install target, activation state, and installation time so replay routes by stable instance identity rather than global `module_id` replacement. Governed proposal apply now stages ordered lifecycle events and every touched registry/artifact/schedule/cache/manifest/proposal/journal/consensus projection in a typed prepared batch and publishes only on full success; atomically aligning registry with persisted instance state, recovery/replay, receipt/outbox, and every non-proposal lifecycle entrypoint remains a target transaction contract.
- Evidence anchors: `crates/oasis7_runtime/src/module_store.rs`, `runtime/error.rs`, `runtime/tests/persistence.rs`, and `runtime/world/module_store_load_transaction_regressions.rs` cover default roundtrip, tamper rejection, legacy no-store load, and late-record atomicity.

## 里程碑
- M1 (2026-03-03): 完成模块设计 PRD 主体重写与任务改造。
- M2: 补齐模块设计验收清单与关键指标。
- M3: 建立 PRD-ID -> Task -> Test 的长期追踪闭环。

## 风险
- 模块边界演进快，文档同步可能滞后。
- 指标口径不稳定会降低验收一致性。
## 1. Executive Summary
- Problem Statement: world runtime 涉及确定性执行、事件溯源、WASM 扩展、治理、审计与运行态持久化等核心能力，若缺少统一设计入口，跨阶段改动容易引发一致性、安全与存储体积失控回归。
- Proposed Solution: 以 world-runtime PRD 统一定义内核能力边界、WASM 运行约束、治理流程、数值语义、存储治理与验证标准。
- Success Criteria:
  - SC-1: runtime 关键改动具备 PRD-WORLD_RUNTIME-ID 映射与测试证据。
  - SC-2: 确定性回放与事件审计链路保持可复现。
  - SC-3: WASM 沙箱与接口变更具备兼容性与安全校验记录。
  - SC-4: 数值语义硬化议题持续收敛并形成阶段性里程碑。
  - SC-5: 运行态持久化具备可观测的存储预算、保留策略与恢复验证，不再在默认链路中无界增长。
  - SC-6: retention policy 保留范围内的目标高度必须具备可验证的 replay contract，可由 checkpoint + canonical log 重建并校验 `execution_state_root`。
  - SC-7: 面向 Viewer / QA 的运行时测试钩子必须显式 env-gated，并输出可回放的标准世界事件，避免测试态捷径泄漏到默认产品路径。
  - SC-8: provider `player_parity` / `headless_agent` 共用同一 runtime 权威动作校验，且 mode/schema/environment/fixture/replay 元数据可稳定写入 request、summary 与 benchmark 产物。
  - SC-9: WASM 构建与发布链路必须通过 pinned Docker canonical builder 收敛为单一 publish hash，保证 `builder_image_digest/source_hash/build_manifest_hash/wasm_hash` 可追溯，且执行层默认只认 canonical binary。
  - SC-10: 生产运行入口必须默认启用 release security policy，关闭 builtin manifest fallback、本地 identity hash 签名、本地 finality signing 与 runtime source compile，保证“只认 canonical binary”不是测试态约定而是产品默认路径。
  - SC-11: `TASK-WORLD_RUNTIME-043` 收口时必须归档真实跨宿主 Docker canonical evidence，至少覆盖 `linux-x86_64` 与一条 Docker-capable `darwin-arm64` 证据输入，不能以 Linux-only gate 宣称跨宿主 closure。
  - SC-12: `doc/world-runtime/**` 仍可读运行时专题标题统一使用 `oasis7 Runtime` 品牌，不再在模块入口与历史专题中混用 `oasis7 Runtime` 标题。
  - SC-13: WASM 构建、同步、CI summary 与 builder image 的 operator env key 必须统一使用 `OASIS7_WASM_*`；repo-owned 脚本、容器镜像与 build suite 不得再接受任何旧品牌前缀作为有效运行入口。
  - SC-14: builtin wasm materializer、release manifest fallback 与 DistFS root override 的 runtime env key 必须统一使用 `OASIS7_BUILTIN_WASM_*`；运行时取件/抓取/回退链路不得再接受任何旧品牌前缀作为有效运行入口。
  - SC-15: `compile_module_artifact_from_source` 及其 source package 限额/超时控制必须统一使用 `OASIS7_MODULE_SOURCE_*`；dev/test source compile 路径、simulator/runtime 回归与沙箱环境隔离断言不得再接受任何旧品牌前缀作为有效运行入口。
- SC-16: runtime 文档中的当前 cargo 回归命令、crate 路径与产物文件清单必须统一使用 `oasis7*` / `crates/oasis7*`；旧品牌包名与源码路径仅允许保留在历史证据、兼容说明或负向测试语义中。
  - SC-17: `oasis7_chain_runtime` 的 `/v1/chain/status` 必须显式暴露节点网络流量观测快照，至少区分 `udp_gossip` 与 `libp2p_replication` 两条链路，并标明统计范围是否包含 transport/control-plane 开销。
  - SC-18: `fetch-commit` gap-sync 请求必须对最近刚返回“缺少 handler / 不支持该协议”签名的 `ErrUnsupported`、`ErrNotFound`、`Timeout` 或连接缺口的 peer 做短时协议级退避；业务语义层的 `ErrUnsupported` 不计入该退避条件，以避免在真实 triad 中对同一无效目标反复发起 libp2p 请求。
  - SC-19: `libp2p_net` peer discovery 路径中的 `get_local_peer_record`、`get_cached_peer_record` 与 `get_cached_discovery_peers` 请求必须对同一 peer 保持短时协议级冷却，压制 DHT/routing/rendezvous/connection-established 事件造成的高频重复取件，同时保留单次 cached-peer-record 请求链里的多 proxy fallback。
  - SC-20: chain-linked `oasis7_viewer_live` 必须默认被动跟随 `/v1/chain/status.consensus.committed_height` 对应的 committed execution state，不需要显式 `Play` 才能消费 committed action；若轮询没有新的 committed action 或没有新增 world event，则不得把该轮询记成逻辑 world progression。
  - SC-21: 节点运营者与 launcher 用户必须能从同一份 `/v1/chain/status.observability` 真值回答“当前连了多少 peer、是否落后、是否有存储/复制/奖励子系统告警”，并可由 repo-owned 脚本与 launcher UI 直接消费，无需各自重复推导。
- SC-22: WASM build / executor / router 必须形成统一的本地 observability snapshot，并通过 `/v1/chain/status.wasm` 暴露 bounded timing / cache / failure 指标；release candidate 与节点 incident 不得再只依赖 ignored perf probe 或临时日志回答热点归因。
  - SC-23: 每个 WASM 模块必须支持通过标准 module-local observe spec 接入共享 contract/perf runner，让新模块在新增时即可产出统一的功能与性能证据，而不是再写 bespoke 脚本。
- SC-24: `/v1/chain/status` 必须显式暴露 commit freshness、pending proposal/queue pressure 摘要、recent finality latency summary、transfer lifecycle/confirmation latency summary 与入站时序拒绝计数，让节点运营者无需翻原始日志即可判断“卡在没提案、卡在未提交、卡在 submit buffer/队列积压、还是卡在 transfer 长时间未确认”。
- SC-25: 首个 agent `slot-1` claim 必须补齐“canonical quote -> 若专用池足够则自动补足 restricted starter amount -> 直接 `ClaimAgent`”的链上闭环；`/v1/chain/agent-claim/**` 需要让玩家在不经过运营审批的前提下直连 claim，并由 runtime 真值暴露自动资助额度与 claim 结果。
- SC-26: builtin wasm 模块边界中的 `GeoPos` 与一切 `*_cm` 坐标字段必须维持整数厘米合同；持久化状态允许兼容读取旧的“整值浮点”厘米表示，但动作/事件/观测入口不得接受 fractional cm，也不得再输出 `0.0` 这类浮点厘米表象。
- SC-27: 正式发布默认入口不得再把 `llm_bootstrap` 或其他调试场景当作初始世界；`oasis7_viewer_live`、`oasis7_game_launcher`、`oasis7_web_launcher`、`oasis7_client_launcher` 与 `oasis7_chain_runtime` 首次启动必须共享同一份固定 genesis / 固定 unified-world bootstrap `world_id`，并避免默认注入 5 个预置 agent。
- SC-28: 只有配置的权威来源可提交最终执行状态；传播或非权威来源的冲突写入必须原子拒绝并留下审计原因，不得进入 committed history 或玩家可见结果。
- SC-29: replay 漂移必须能定位首个不一致边界，并按最小影响范围执行 rollback、canonical replay 与 reconciliation；只有最终 state root 和已确认结果重新一致后，恢复状态才能重新对外暴露。

## 2. User Experience & Functionality
- User Personas:
  - 运行时架构师：需要控制可信边界与模块化演进。
  - 模块开发者：需要稳定 ABI/执行语义与治理流程。
  - 审计与安全评审者：需要完整可追溯的事件与收据链路。
  - QA / 发布运维：需要可预测的运行态磁盘预算、恢复能力与状态指标。
  - 发布节点运营者 / 构建审计者：需要区分哪些校验属于 Docker canonical build evidence，哪些属于线上发布合法性。
- User Scenarios & Frequency:
  - 运行时语义评审：每次核心行为改动前执行，确认确定性与兼容边界。
  - WASM 接口变更：每个接口变更至少进行一次兼容核验与回放验证。
  - 治理事件审计：发布前执行，检查关键治理事件链路完整性。
  - 安全回归复核：按周执行，验证沙箱、签名、权限约束无回退。
  - 运行态存储复核：每次持久化 / 启动器 / 链路改动后执行，确认 footprint、GC 与恢复能力符合预算。
  - WASM 构建/发布复核：每次涉及 builder image、canonicalizer、manifest/identity、source compile 或 release materializer 时执行，确认 Docker canonical hash 与社会层审计字段保持一致。
- User Stories:
  - PRD-WORLD_RUNTIME-001: As a 架构师, I want deterministic world execution semantics, so that replay and audit remain trustworthy.
  - PRD-WORLD_RUNTIME-002: As a 模块开发者, I want stable WASM interfaces and lifecycle governance, so that upgrades are safe.
  - PRD-WORLD_RUNTIME-003: As a 安全评审者, I want explicit security and receipt guarantees, so that critical risks are controlled.
  - PRD-WORLD_RUNTIME-013: As a Runtime 维护者, I want bounded execution-state retention and sidecar GC, so that默认运行链路的磁盘占用受控。
  - PRD-WORLD_RUNTIME-014: As a QA / 审计维护者, I want GC 后仍可 latest-state 恢复并保留检查点取证能力, so that体积优化不破坏恢复与审计。
  - PRD-WORLD_RUNTIME-015: As a 发布工程师, I want profile-based storage policies and metrics, so that dev/release/soak 能执行不同磁盘预算。
  - PRD-WORLD_RUNTIME-019: As a Runtime 维护者 / Viewer / QA, I want factory production blocked/resumed/completed state to be explicit and replayable, so that early industrial onboarding can explain why production advanced or stalled.
  - PRD-WORLD_RUNTIME-020: As a `wasm_platform_engineer`, I want publishable WASM to be built only inside a pinned Docker builder image, so that host platform differences stop influencing release hashes.
  - PRD-WORLD_RUNTIME-021: As a 模块发布者 / 发布节点运营者, I want build receipt and release evidence to bind `builder_image_digest + source_hash + build_manifest_hash + wasm_hash`, so that binary trust can be socially verified without relying on host-native builds.
  - PRD-WORLD_RUNTIME-022: As a `runtime_engineer` / `qa_engineer`, I want runtime to consume only Docker-canonical binaries and production source compile to leave the runtime hot path, so that build drift is blocked before execution.
  - PRD-WORLD_RUNTIME-023: As a `runtime_engineer`, I want production-facing runtime entrypoints to default to hardened `ReleaseSecurityPolicy`, so that `no-fallback / no-local-signing / no-runtime-source-compile` is enforced by construction instead of call-site convention.
  - PRD-WORLD_RUNTIME-024: As a `runtime_engineer` / `qa_engineer`, I want `apply_domain_event*` replay/apply paths to return structured `WorldError` for invariant breaks, so that corrupted journal / migration drift is diagnosable without panic-killing recovery or preflight.
  - PRD-WORLD_RUNTIME-025: As a `runtime_engineer`, I want oversized runtime hotpath files split below the 1200-line governance ceiling, so that determinism, replay, and rule changes stop depending on multi-kiloline match blocks.
  - PRD-WORLD_RUNTIME-026: As a `runtime_engineer` / `qa_engineer` / 节点运营者, I want `/v1/chain/status` to expose network traffic counters for `udp_gossip` and `libp2p_replication`, so that real traffic spikes can be attributed to a concrete lane instead of only host-level bandwidth totals.
  - PRD-WORLD_RUNTIME-027: As a `runtime_engineer` / 节点运营者, I want a persistent triad traffic sampler to convert cumulative `/v1/chain/status.traffic` counters into recent-window deltas, so that questions like "last 10 minutes per node" can be answered without waiting for a process restart.
  - PRD-WORLD_RUNTIME-028: As a 节点运营者, I want traffic monitoring to be an env-gated node startup capability, so that local recent-window history can be enabled per node without hand-running an external triad script.
  - PRD-WORLD_RUNTIME-029: As a `runtime_engineer` / 节点运营者, I want `fetch-commit` retries to short-circuit peers that just returned protocol-unavailable, not-found, or timeout signatures, so that gap-sync traffic waste drops without relaxing replication correctness.
  - PRD-WORLD_RUNTIME-030: As a `runtime_engineer` / 节点运营者, I want peer-record/discovery requests plus periodic discovery DHT refreshes to respect short cooldown and republish budgets, so that repeated DHT/routing/rendezvous triggers stop reissuing `get_local_peer_record` / `get_cached_peer_record` / `get_cached_discovery_peers`, and `RefreshPeerDiscovery` stops relaunching `get_providers` / local peer-record republish on every short interval tick.
  - PRD-WORLD_RUNTIME-031: As a `runtime_engineer` / `viewer_engineer`, I want chain-linked viewer runtime to passively follow committed execution-state progress from `oasis7_chain_runtime`, so that the player no longer needs explicit `Play` just to consume committed chain actions and empty polls do not masquerade as world advancement.
  - PRD-WORLD_RUNTIME-032: As a `runtime_engineer` / 节点运营者, I want storage challenge `fetch-blob` probes to reuse a short-lived success cache for recently verified content hashes, so that triad nodes stop re-fetching the same reachable blob every commit while still rechecking new blobs and expiring old proofs quickly.
  - PRD-WORLD_RUNTIME-033: As a `runtime_engineer` / `viewer_engineer`, I want chain-linked viewer gameplay actions to submit through `oasis7_chain_runtime` and only become visible after committed execution-state sync, so that player controls follow the same consensus-backed path as chain observation instead of mutating the local viewer state optimistically.
  - PRD-WORLD_RUNTIME-034: As a `runtime_engineer` / 节点运营者, I want repeated successful `fetch-commit` requests with the same deterministic payload to reuse a short-lived local success cache, so that followers stop re-asking the network for the same already-found commit during tight gap-sync loops without hiding not-found or timeout recovery.
  - PRD-WORLD_RUNTIME-035: As a `runtime_engineer` / `viewer_engineer` / 节点运营者, I want node observability to be published as a stable summary plus alerts contract and surfaced through launcher/UI and repo-owned reports, so that current peer count, lag, degraded subsystems, and active alerts are directly visible without reading raw payload internals.
  - PRD-WORLD_RUNTIME-036: As a `wasm_platform_engineer` / `runtime_engineer` / `qa_engineer`, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes.
  - PRD-WORLD_RUNTIME-037: As a `wasm_platform_engineer` / `qa_engineer`, I want each wasm module to expose a standard local observe spec consumable by a shared runner, so that every new module can inherit consistent contract and perf evidence without bespoke glue code.
  - PRD-WORLD_RUNTIME-038: As a `runtime_engineer` / 节点运营者, I want `/v1/chain/status.traffic.libp2p_replication` to expose real substream wire-byte totals plus derived control-plane wire bytes, so that control-plane overhead can be estimated from runtime truth instead of only host NIC totals or event-count heuristics.
  - PRD-WORLD_RUNTIME-039: As a `runtime_engineer` / 节点运营者, I want `/v1/chain/status` to expose commit freshness, pending proposal progress, pending consensus action queue pressure, recent finality latency, transfer lifecycle and confirmation latency summaries, plus inbound timing reject counters, so that common chain stalls can be classified directly from status truth instead of ad hoc log grep.
  - PRD-WORLD_RUNTIME-040: As a 新账号玩家 / `liveops_community`, I want the first `slot-1` claim to auto-draw startup restricted funding from the dedicated pool when runtime says the pool can cover it, so that onboarding no longer depends on operator review queues or undocumented manual grants.
  - <a id="prd-world-runtime-041"></a>PRD-WORLD_RUNTIME-041: 作为 Runtime、consensus、node 与网络维护者，我需要算术越界、整数窄化及批量状态转移遵循明确的 checked failure、协议 rollover 或 observable clamp 合同；可失败路径在写入前拒绝，显式 clamp 保留确定边界和可重跑测试，避免部分状态与不可重放结果。专业语义与实现映射由 [数值安全 PRD](runtime/runtime-numeric-safety.prd.md) 和 [数值安全设计](runtime/runtime-numeric-safety.design.md) 维护。
  - PRD-WORLD_RUNTIME-043: As a Runtime 维护者 / QA / 审计者, I want every authoritative accepted industrial outcome to receive one immutable root operation identity with persisted revision/segment, direct parent/child and terminal-disposition links, so that stage/join/bundle/transit/terminal effects remain attributable and retry, recovery or replay cannot duplicate sink, credit, progress, receipt or reward.
- Critical User Flows:
  1. Flow-WR-001: `提交 runtime 变更 -> 执行回放一致性验证 -> 对比事件链 -> 输出兼容结论`
  2. Flow-WR-002: `WASM 模块注册/升级 -> 生命周期治理校验 -> 沙箱执行 -> 审计事件归档`
  3. Flow-WR-003: `安全异常发现 -> 回溯 receipt -> 定位策略缺口 -> 补回归与发布阻断`
  4. Flow-WR-004: `运行一段时间 -> 采集 storage metrics -> 执行 retention / GC -> 重启恢复 -> 对比 latest state 与审计链`
  5. Flow-WR-005: `选择 retention policy 保留的目标高度 -> 定位 checkpoint -> 回放 canonical log -> 校验 execution_state_root -> 输出 replay 结论`
  6. Flow-WR-006: `源码/manifest 变更 -> pinned Docker builder image 构建 canonical packaged wasm -> canonical hash/identity/release evidence -> DistFS/release manifest -> runtime 仅按 binary hash 装载执行`
  7. Flow-WR-007: `viewer 连接 chain-linked runtime -> 轮询 /v1/chain/status -> committed_height 增加时重载 committed execution state -> 仅在出现新 event / logical time 变化时向 viewer 发出 snapshot/events`
  8. Flow-WR-008: `viewer 触发 gameplay_action -> POST /v1/chain/gameplay/submit -> chain runtime 校验 auth/nonce 并入 consensus queue -> committed execution state 落盘 -> viewer 仅在下一次 chain sync 后观察到结果`
  9. Flow-WR-009: `build suite / executor / router 更新本地累计 timing metrics -> oasis7_chain_runtime 通过 /v1/chain/status.wasm 暴露 bounded snapshot -> 外部脚本做窗口 delta / p50 / p95 / hotspot 汇总`
  10. Flow-WR-010: `玩家读取 slot-1 canonical quote -> runtime 判断是否可从 restricted_starter_claim_liveops_pool 自动补足首笔启动额度 -> 玩家 POST /v1/chain/agent-claim/submit -> runtime 原子完成自动资助与 ClaimAgent -> viewer / API 读取同一条 claim 与 auto-funding 真值`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 确定性执行与回放 | action/event 序列、snapshot、replay 差异 | 执行回放并比较关键状态 | `pending -> replaying -> matched/mismatched` | 按 tick 与 event id 有序比较 | 仅运行时维护者可调整基线 |
| WASM 生命周期治理 | 模块ID、版本、哈希、策略 | 注册/升级/停用流程带审计输出 | `register -> activate -> deactivate -> upgrade`（治理动作） | 版本与策略双约束 | 未授权模块不得激活 |
| 审计与收据链路 | effect、receipt、签名、cause | 导出审计记录并验证签名 | `emitted -> signed -> verified/rejected` | 按事件时间与重要级别检索 | 安全评审者可查看完整链路 |
| 运行态存储治理 | `storage_profile`、snapshot/journal refs、GC 结果、目录字节数 | 启动时加载策略，运行时发布指标并执行 retention / GC | `configured -> active -> degraded/failed` | latest head 永远 pin；checkpoint 按高度排序；metrics 按目录聚合 | 仅运行时维护者 / 发布配置可放宽预算 |
| 回放契约治理 | `canonical_log`、`checkpoint_anchor`、`retained_heights`、`execution_state_root` | 对保留范围内目标高度执行 replay 验证 | `requested -> replaying -> matched/mismatched` | 以 checkpoint + canonical log 为重建基准 | QA / 审计维护者可读取结果 |
| WASM Docker 确定性构建与工件治理 | `builder_image_digest`、`source_hash`、`build_manifest_hash`、`wasm_hash`、`canonicalizer_version`、`canonical_token` | 统一 Docker canonical build、manifest/identity、DistFS 与 runtime binary-only 消费 | `source -> container-built -> canonicalized -> manifested -> verified/executed` | 发布级只允许一个 canonical hash；宿主差异不得进入发布 hash 空间 | `wasm_platform_engineer` 定义 builder image；CI 仅校验；生产发布不由 CI 写入 |
| WASM 可观测性与耗时指标 | `/v1/chain/status.wasm`、`build timing`、`executor cache/call buckets`、`router prepare/match timings`、`degraded_reason` | build suite / executor / router 更新本地 snapshot，节点按 status 请求返回 bounded machine-readable 指标 | `metrics_unavailable -> available -> degraded` | 默认只保留累计 counters、sum、fixed buckets 与 bounded top-N；重启/reset 需缩窗 | `wasm_platform_engineer` 设计 schema，`runtime_engineer` 暴露 status，`qa_engineer` 复核窗口汇总 |
| WASM 模块标准化观测与契约测试 | `module_observe.json`、`cases`、`router_probes`、`summary.json/md`、`executor/router delta` | 通用 runner 根据 module-local spec 构建模块、执行 contract cases、采样 perf、验证 router probe | `spec_authored -> built -> observed -> summarized` | 模块差异通过 spec 表达；默认只输出 bounded summary，不允许 bespoke runner 分叉 | `wasm_platform_engineer` 维护 schema/runner，模块作者维护 module-local spec |
| 历史标题治理 | 标题前缀、专题路径、历史命名说明 | 将仍可读 runtime/module/governance/wasm/testing 专题标题切到 `oasis7 Runtime` | `legacy_title -> current_title -> audited` | 先改主入口/治理/运行时专题，再改更深历史专题 | `producer_system_designer` 定口径，`runtime_engineer` 承接 |
| chain-linked gameplay 提交 | `GameplayActionRequest`、auth proof、nonce、consensus `action_id`、`committed_height` | viewer action 通过 `/v1/chain/gameplay/submit` 入链，提交成功只返回 consensus action id，不直接改本地 viewer state | `requested -> submitted -> committed_visible/rejected` | viewer 只在 committed execution state 产生新 logical time/event 后更新快照 | 需要 viewer auth proof 且 chain runtime 必须拒绝 nonce replay |
- Acceptance Criteria:
  - AC-1: world-runtime PRD 覆盖内核、WASM、治理、安全四条主线。
  - AC-2: GitHub task 将 PRD-ID、任务状态与验收证据关联；专业文档只维护现行技术合同。
  - AC-3: 与 `doc/world-runtime/runtime/runtime-integration.md`、`doc/world-runtime/wasm/wasm-interface.md` 等分册一致。
  - AC-4: 关键行为变更同步更新测试方案与执行记录。
  - AC-5: 内置 WASM 工件 `sha256` 清单与 identity manifest 保持一致，CI 不得出现 hash token 漂移。
  - AC-6: 运行态存储治理具备专题 PRD / project、预算口径、恢复验证与测试映射，默认链路不得出现无界磁盘增长。
  - AC-7: 运行态持久化专题必须明确 replay contract、canonical log 与 checkpoint 语义，并通过 retained-height replay 测试验证。
  - AC-8: WASM deterministic pipeline 专题必须明确 Docker builder image、canonicalizer version、single canonical publish hash、identity/release evidence、source compile 外移或 gated 与 runtime binary-only 消费边界，并具备独立任务与验证映射。
  - AC-9: 生产 runtime / node 入口必须有可验证的 release policy 绑定证据，证明 fallback / 本地签名 / runtime source compile 默认关闭；若仅测试调用 `enable_production_release_policy()`，不得视为收口。
  - AC-10: 发布候选的 wasm determinism evidence 必须显式区分“当前稳定 gate”和“最终跨宿主 gate”；当 GitHub-hosted runner 缺 Docker daemon 时，必须补外部 Docker-capable macOS evidence，不能把 `linux-x86_64` 单宿主结果等同于跨宿主 closure。
  - AC-11: `doc/world-runtime/**` 仍可读专题标题统一使用 `oasis7 Runtime` 品牌；旧 `oasis7 Runtime` 仅允许保留在正文历史上下文、实现兼容说明与证据原文中。
  - AC-12: `scripts/build-wasm-module.sh`、`scripts/sync-m1-builtin-wasm-artifacts.sh`、`scripts/ci-m1-wasm-summary.sh`、`tools/wasm_build_suite` 与 `docker/wasm-builder/Dockerfile` 必须只读取或写入 `OASIS7_WASM_*`；错误提示、usage、容器注入 env 与 build receipt 元数据采集不得再接受任何旧品牌前缀作为有效运行入口。
  - AC-13: `runtime/builtin_wasm_materializer`、`runtime/m{1,4,5}_builtin_wasm_artifact`、`runtime/world/release_manifest` 及对应测试必须只读取 `OASIS7_BUILTIN_WASM_DISTFS_ROOT`、`OASIS7_BUILTIN_WASM_FETCHER`、`OASIS7_BUILTIN_WASM_FETCH_URLS`、`OASIS7_BUILTIN_WASM_COMPILER`、`OASIS7_BUILTIN_WASM_FETCH_TIMEOUT_MS`；builtin wasm 取件、抓取、编译 fallback 与 release manifest 生产策略故障签名必须证明旧品牌前缀已失效。
  - AC-14: `runtime/module_source_compiler` 与 `runtime/simulator` 对应回归必须只读取 `OASIS7_MODULE_SOURCE_COMPILER`、`OASIS7_MODULE_SOURCE_MAX_FILES`、`OASIS7_MODULE_SOURCE_MAX_FILE_BYTES`、`OASIS7_MODULE_SOURCE_MAX_TOTAL_BYTES`、`OASIS7_MODULE_SOURCE_COMPILE_TIMEOUT_MS`；source compile 成功、旧 alias 已移除与 sandbox env 隔离断言必须覆盖当前前缀。
  - AC-15: `/v1/chain/status` 必须输出 `traffic.udp_gossip` 与 `traffic.libp2p_replication` 两组快照；其中 UDP gossip 至少提供按消息种类聚合的入/出站 datagram 与 payload bytes，libp2p replication 至少提供 gossip/request/response 的入/出站 payload counters、按 topic/protocol 聚合明细，以及 `scope`/排除项说明。
  - AC-16: runtime 文档中的当前 `cargo test -p` 命令、crate 路径与产物清单必须写为 `oasis7` / `crates/oasis7*`；旧品牌包名与源码路径仅允许保留在历史证据、兼容说明或负向测试输入中。
  - AC-17: `World::new()` / `RuntimeWorld::new()` 所服务的生产或默认运行入口不得依赖额外 `enable_production_release_policy()` 调用才能满足 hardened policy；若某入口必须放宽，必须以显式 dev/test 配置进入并留下验证证据。
  - AC-18: `state.apply_domain_event*`、preflight preview 与恢复链路中不得因“prechecked / must be handled”类假设触发 panic；同类异常必须落为可断言的 `WorldError`，并有损坏事件回归样本覆盖。
  - AC-19: `action_to_event_*`、`apply_domain_event_*`、`state.rs` 与其他 runtime 热路径 Rust 文件不得超过 1200 行；拆分后需保留现有 determinism / replay / persistence 回归覆盖，不得以降低测试强度换取拆分完成。
  - AC-20: `oasis7_chain_runtime` 默认注入的 loopback replication fallback 仅可为 replication / feedback 提供本地兜底，不得在已配置 UDP gossip 的多机部署中抢占 PoS consensus 广播；显式共享 replication network 的 network-consensus 路径必须继续可用。
  - AC-21: repo 内必须提供可复用的 triad traffic monitor 脚本，默认面向本机 observer + 两台 ECS 节点，能抓取 `/v1/chain/status` 并写入持久化 history。
  - AC-22: triad traffic monitor 必须支持最近 N 分钟窗口汇总，至少输出每节点的 UDP gossip / libp2p replication totals delta、top `by_kind` / `by_topic` / `by_protocol` 贡献项，以及 committed/network heights 与 recent error counters 的窗口变化。
  - AC-23: repo 内必须提供标准化 wasm module observe 入口，至少覆盖 module-local spec、共享 runner、wrapper script、模板与一个真实模块样例；新模块接入默认不得要求 bespoke runner 代码改动。
  - AC-24: triad traffic monitor 的窗口基线选择必须识别 `observed_since_unix_ms` 变化并在进程重启或 counter reset 时自动缩短覆盖窗口，而不是跨 reset 计算负值或伪增量。
  - AC-25: triad traffic monitor 的持久化 history 不得无限增长；脚本必须只保留“最近窗口 + buffer”范围内的样本，并以 NDJSON 流式读取/裁剪 history，而不是整文件 `read_text` 后一次性解码。
  - AC-26: repo 内必须提供节点启动壳的正式源码来源，至少覆盖当前 `/opt/oasis7/p2p-triad{,-local}/bin/start-node.sh` 所使用的 `node.env -> runtime CLI` 装配逻辑，并允许新增 env-gated sidecar 能力而不再依赖 `/opt` 上手改。
  - AC-27: 节点本地 traffic monitor 必须能通过 `node.env` 功能开关启停，默认对单节点本地 `/v1/chain/status` 进行周期采样，输出持久化 history 与最近 N 分钟 summary；节点与 triad monitor 的窗口汇总必须复用共享 helper，且本地 history 也要做 bounded retention。
  - AC-28: 节点启动壳在开启 traffic monitor 时必须与 runtime 共享生命周期，runtime 退出或 service 停止时不能留下长期孤儿 monitor 进程；若开关开启但 monitor 脚本缺失，启动必须显式失败而不是静默跳过。
  - AC-29: `libp2p_replication_network` 必须仅对 `fetch-commit` 请求中的 missing-handler/unsupported-protocol `ErrUnsupported` 签名、`ErrNotFound`、`request failed: Timeout` 与连接缺口类错误触发短时 peer cooldown，并通过定向回归证明立即重试会被抑制、窗口过后可恢复请求、非 `fetch-commit` 协议与通用业务错误（包括泛化业务态 `ErrUnsupported`）不受影响。
  - AC-30: `libp2p_net` 必须同时对 peer-record/discovery 路径中的 `get_local_peer_record`、`get_cached_peer_record`、`get_cached_discovery_peers` 触发 peer-scoped、protocol-scoped 短时冷却，并对 `RefreshPeerDiscovery` 内的 `get_providers` 查询与本地 peer-record/provider republish 触发短窗 budget gate；定向回归需证明同一 peer 在窗口内不会被立刻重复请求、窗口过后可恢复请求、断连会清理对应 peer 冷却、cached-peer-record 在单次请求链中的 fallback proxy 仍可继续尝试、且 discovery query 在已有同类查询在飞或 cooldown 未到期时不会重复启动。
  - AC-31: storage challenge gate 对近期已验证成功的 `content_hash` 必须提供短窗口 success cache，只允许在缓存过期、命中新 hash、或本地 blob 缺失时重新发起 `fetch-blob` 网络探测；定向回归需证明连续两次 gate 调用不会对同一已验证 blob 重复发网请求，同时缓存过期后仍会恢复真实探测。
  - AC-31: chain-linked `gameplay_action` 必须通过 `oasis7_chain_runtime` 的 `/v1/chain/gameplay/submit` 进入 consensus queue；提交路径必须复用 viewer auth proof、拒绝 nonce replay、返回 consensus `action_id` 作为提交回执，并证明 viewer 本地 state/cache 在提交时不会立即变更，只会在 committed execution-state sync 后观察到新工厂/配方结果。
  - AC-32: gap-sync `fetch-commit` 路径必须仅在高层校验接受 commit 后建立短时、本地、payload-scoped success cache；定向回归需证明相同 payload 的立即重复成功请求不会再次发网、窗口过后会恢复真实请求、且校验失败或 `found=false` 响应不会进入 success cache。
  - AC-33: `/v1/chain/status` 必须新增稳定的 `observability` 摘要契约，至少暴露 `status/summary`、`connected_peer_count`、`active/candidate/suspect/blocked` peer 计数、`known_peer_heads`、`network_height_lag`、`recent_replication_error_count`、`storage_degraded`、`reward_runtime_degraded` 与结构化 `alerts[{severity,code,summary}]`；repo-owned 报告脚本与 launcher/control-plane 必须直接消费该契约而不是各自重算，并能补充最近 traffic window 摘要。
  - AC-34: `tools/wasm_build_suite`、`oasis7_wasm_executor` 与 `oasis7_wasm_router` 必须形成统一的 bounded timing 指标面，至少覆盖 canonical build、cache hit/miss、executor call 与 router prepare/match 四段热点；正式候选归因不得只依赖 ignored perf probe 的 `eprintln!`。
  - AC-35: `oasis7_chain_runtime` 的 `/v1/chain/status` 必须新增 `wasm` section，显式输出 `metrics_available`、`observed_since_unix_ms`、`degraded_reason` 与 build/executor/router 的 machine-readable snapshot；这些字段不得进入 world state、event log 或 replay contract。
  - AC-36: 默认 WASM status payload 不得暴露 `trace_id`、原始 payload bytes 或无界 `module_id -> timing` map；若提供模块级热点明细，必须限制为 bounded top-N 或显式 allowlist，并在裁剪时输出可观测标记。
  - AC-37: `/v1/chain/status.traffic.libp2p_replication` 必须同时暴露 `totals`（应用 payload）、`wire_totals`（libp2p substream wire bytes）与 `control_plane.wire_bytes`（`wire_totals - totals.payload_bytes`）；`control_plane.wire_scope` 必须显式声明其只覆盖 substream 级非 payload bytes，且继续排除 transport handshake/framing 开销。
  - AC-38: `crates/oasis7_builtin_wasm_modules/m1_*` 中消费 `GeoPos`/`*_cm` 坐标的 builtin wasm 模块必须把模块内部主表示收口到整数厘米，并对动作/事件 JSON 边界拒绝 fractional cm；升级后仍需兼容读取旧的整值浮点 module state，并把新的 state / observability sample 统一序列化为整数厘米。
  - AC-41: runtime schema 与 apply paths 必须区分 legacy `ActionId/job_id/caused_by` trace 和 canonical operation protocol。每个 authoritative accepted industrial outcome 的测试必须证明：acceptance 只签发一个 root，atomic reject 且无 accepted intent 时不签发；两个 payload 相同的 accepted intents 获得不同 roots；stage/join/bundle/branch/transit/buffer/terminal/window/checkpoint/validation/settlement child 持久化同一 root、owning revision/segment 与直接 parent/child role；missing/conflicting/wrong-root/terminal-closed linkage 在 first sink、credit、progress 或 reward 前 fail closed；retry、reconnect、乱序、snapshot restore 与 canonical replay 重建相同 child identity 和 terminal disposition，且不复制效果；snapshot + journal replay 得到同一 operation graph/finality 与 world state root。
  - AC-42: runtime 文档必须对关键能力明确标注 `current`、`partial`、`target` 或 `proven`；不得以已有接口、局部 staged path 或 compatibility record 代替端到端证明。
  - AC-43: 治理批准的 activation boundary 必须按产品 SC-32 在 Alliance/EconomicContract 中择一转为唯一 pilot；`institution-migration-v1` 必须证明 module-owned command/event/state 不新增 Kernel 制度 schema/顶层字段，并覆盖权限、owner/payer 预算可读性、preview/stale/deny/no-effect 不扣费、accepted effect+debit+receipt exactly-once、retry/reconnect/restore/replay 不重复扣费或发放 replay credit、quote、Kernel apply、durable outbox、snapshot、replay 与 recovery；content-addressed report bundle digest 必须回写 实际验证记录。
  - AC-44: 所有有 world effect 的 command/tick/恢复写入入口必须收敛到统一 `ExecutionTransaction`（或等价 staged boundary）；commit 前不得修改 canonical state/journal/外部 effect，失败必须无半个业务效果并留下稳定 rejected/fault disposition。该项是架构 P0，不能解读为一次性 ECS/WorldState 重写承诺。
  - AC-45: module instance 完整性必须覆盖 direct/trusted command 的授权、寻址、state lookup、更新事件、receipt、snapshot/replay 与 machine descriptor；同 module artifact 的不同 `instance_id` 必须可并行且不串写，任何 `module_id` 全局 fallback 都必须显式标为 partial。
  - AC-46: subsystem state encapsulation 必须渐进落地并保留现有 replay/persistence identity；本 PRD 不承诺 big-bang ECS、独立 shard finality、跨 shard commit 或动态 World Database。
- Non-Goals:
  - 不在本 PRD 中展开每个阶段的实现代码细节。
  - 不替代 p2p 网络拓扑或 site 发布策略设计。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: WASM 执行与治理测试、审计导出工具、数值语义回归套件。
- Evaluation Strategy: 以回放一致性、治理事件完整度、沙箱安全回归数、数值语义缺陷收敛率评估。

## 4. Technical Specifications
- Architecture Overview: world-runtime 模块是系统可信执行基座，负责世界状态演化、模块扩展执行与治理审计，向上游 simulator/game 与下游 p2p 提供稳定语义。
- Integration Points:
  - `doc/world-runtime/runtime/runtime-integration.md`
  - `doc/world-runtime/wasm/wasm-interface.md`
  - `doc/world-runtime/wasm/wasm-executor.prd.md`
  - `doc/world-runtime/wasm/wasm-sdk.prd.md`
  - `doc/world-runtime/wasm/wasm-deterministic-build-pipeline.prd.md`
  - `doc/world-runtime/wasm/wasm-observability-timing-metrics.prd.md`
  - `doc/world-runtime/governance/governance-events.md`
  - `doc/world-runtime/module/player-published-entities.prd.md`
  - `doc/world-runtime/runtime/runtime-storage-footprint-governance.prd.md`
  - `doc/world-runtime/runtime/runtime-storage-footprint-governance.design.md`
  - `doc/world-runtime/runtime/chain-pos-control-plane.prd.md`
  - `doc/world-runtime/testing/testing.md`
- Edge Cases & Error Handling:
  - 回放不一致：立即标记高风险阻断并输出差异快照。
  - 接口超时/失败：WASM 执行异常需返回结构化错误而非 panic。
  - 空事件流：空输入需稳定返回，无副作用写入。
  - chain-linked 空轮询：`/v1/chain/status` 轮询若未观察到新的 `committed_height`，或新的 execution world 未产生新增 event / logical time，则 viewer 只能保持当前快照，不得伪造“自动推进中”的 world 进度。
  - 权限不足：未授权模块请求直接拒绝并记录审计事件。
  - 并发冲突：治理操作并发时按版本序列化处理，拒绝乱序变更。
  - 数据异常：receipt 校验失败时不得推进状态并触发安全告警。
  - 存储异常：GC/保存中断时必须保留 latest recoverable head，禁止“先删后写”导致不可恢复状态。
  - 构建漂移：若同一 Docker builder 在不同宿主上产出不同 canonical hash，或 receipt/identity 不一致，必须在进入 runtime 执行前被 gate 阻断。
  - 生产入口未启用 release security policy：必须在发布验证中被标记为 `no-go`，因为此时 builtin manifest fallback / 本地签名 / runtime source compile 仍可能留在热路径。
  - 默认 loopback replication fallback 抢占 consensus 广播：若节点已配置 UDP gossip peers，则该 fallback 不得阻止 proposal / attestation / commit 继续经 gossip 交换，否则多机 PoS 会表现为各自推进高度但 `known_peer_heads=0`。
  - GitHub-hosted macOS runner 无 Docker daemon：允许将 CI 临时收敛为 Linux-only stable gate，但必须把跨宿主 canonical evidence 标记为未完成，并要求外部 Docker-capable macOS summary/import 继续补证。
  - 执行器初始化失败：WASM executor / SDK wire 初始化或解码失败必须向宿主或调用者返回结构化错误，不得在平台层静默吞错。
  - WASM metrics snapshot 不可用：必须通过 `degraded_reason` 暴露降级原因，但不得让指标锁失败反向阻断正常 module 执行。
- Non-Functional Requirements:
  - NFR-WR-1: 同一输入回放结果一致率 100%。
  - NFR-WR-2: 关键治理事件审计链路完整率 100%。
  - NFR-WR-3: WASM 接口变更需保持向后兼容或明确破坏性声明。
  - NFR-WR-4: 安全相关回归在 full 层级覆盖率达到目标阈值并持续跟踪。
  - NFR-WR-5: 核心运行时异常可在 30 分钟内完成初步定位。
  - NFR-WR-6: 默认开发/启动器 profile 必须定义明确磁盘预算、保留窗口与 metrics 输出，不得依赖手工清目录维持可用性。
  - NFR-WR-7: retention policy 保留范围内的 replay success rate 必须为 `100%`，且重建 `execution_state_root` 与原记录一致。
  - NFR-WR-8: 同一 commit、同一 Docker builder image digest 下的 canonical packaged wasm hash 可复现率必须为 `100%`，且 drift 失败输出必须定位到 `module_id/builder_image_digest/expected/actual`。
  - NFR-WR-9: production runtime / node 默认策略必须保证 `allow_builtin_manifest_fallback=false`、`allow_identity_hash_signature=false`、`allow_local_finality_signing=false`、`allow_runtime_source_compile=false`；任何放宽都必须通过显式 dev/test 配置进入。
  - NFR-WR-10: `TASK-WORLD_RUNTIME-043` 完成前，发布文档与 gate 摘要必须明确标示“Linux-only stable gate”与“cross-host evidence pending”的区别，避免误报 closure。
  - NFR-WR-11: 网络流量观测快照必须在线程间安全可读、随节点生命周期累积，并在 payload 中明确标注统计范围，避免将逻辑 payload 计数误报成完整 wire-bandwidth 真值。
  - NFR-WR-12: `fetch-commit` 失败退避必须保持 peer-scoped、protocol-scoped 且自动过期，默认只压制短时高频重试，不得把业务层永久不兼容或其他协议的错误扩大成全局节点隔离。
  - NFR-WR-13: peer-record/discovery 降噪必须局限于同 peer 的单协议短窗抑制，以及同节点本地 discovery query/republish 的 bounded budget gate；默认只减少重复取件与重复 DHT 控制面噪音，不得把一次 cached-peer-record cache miss 扩大成长期 peer 隔离，也不得阻断同一请求链中的备选 proxy fallback 或 reachability 变化驱动的必要 republish。
  - NFR-WR-14: storage challenge `fetch-blob` success cache 必须只复用短窗口内的正向验证结果，默认只减少最近重复取件噪音，不得把过期网络可达性证明长期当真，也不得掩盖本地 blob 缺失等硬失败。
  - NFR-WR-15: chain-linked gameplay submit 必须保持“提交成功不等于本地 viewer state 立即可见”的 committed-only 可见性边界，viewer 只能在 committed execution-state sync 之后反映新状态，不得因本地 optimistic mutation 伪造链上结果。
  - NFR-WR-16: `fetch-commit` success cache 必须只复用近期、相同 deterministic request payload 的正向响应，不得缓存 `found=false` 或协议错误，也不得把 peer-scoped 失败冷却扩大成全局成功假象。
  - NFR-WR-17: 节点观测摘要必须来自 runtime 已发布的单一真值，launcher、web control plane 与 repo-owned 报告脚本只做透传与格式化，不得各自维护独立健康判定逻辑，避免不同入口对同一节点给出互相冲突的状态。
  - NFR-WR-18: `/v1/chain/status.wasm` 默认输出必须保持 bounded cardinality，未启用模块级 top-N 时不得随模块总数线性增长。
  - NFR-WR-19: control-plane 字节计数必须建立在 runtime 可证明的 wire 观测层上；若当前实现只能覆盖 libp2p substream 而不能覆盖 transport handshake/framing，则 payload、报告脚本与文档必须显式声明该边界，禁止把它表述成完整 NIC/wire 真值。
  - NFR-WR-20: WASM timing 指标的本地采集不得改变 deterministic execution 输出；所有 wall-clock 数据只允许存在于本地观测层。
  - NFR-WR-21: 默认配置下，WASM metrics instrumentation 对现有 release perf probe 的额外 wall-clock 开销目标不高于 `10%`。
  - NFR-WR-22: 默认 `/v1/chain/status.wasm` payload 预算建议 `<=64 KiB`，开启 bounded top-N 明细后的预算建议 `<=128 KiB`。
  - NFR-WR-23: `/v1/chain/status` 新增的链健康字段必须保持 bounded cardinality 与常数级开销；默认路径只允许输出聚合计数、单个 pending proposal 摘要、payload byte totals 与 bounded latency summary，不得把未提交 action 明细、逐笔 transfer 历史或高基数 peer 级失败列表直接塞进 status payload。
  - NFR-WR-24: 标准化 wasm module observe runner 不得因模块特例膨胀成分支集合；默认新增模块只允许通过 module-local spec/fixture 接入，不得要求 runner 增加模块专属逻辑。
- Security & Privacy: 强制最小权限、签名校验、审计留痕；禁止未授权模块绕过规则层直接修改世界状态。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03-03): 固化 runtime 模块设计基线与边界约束。
  - v1.1: 补齐 WASM 生命周期与治理流程的跨模块验收清单。
  - v2.0: 建立运行时质量趋势报表（确定性、安全、性能、治理事件完整度）。
- Technical Risks:
  - 风险-1: 运行时复杂度提升导致验证成本增加。
  - 风险-2: ABI/治理策略变更引发兼容性断裂。
  - 风险-3（2026-03-18 记录，2026-06-28 已收口）: `TASK-WORLD_RUNTIME-043` 原先只有 GitHub-hosted `linux-x86_64` stable gate，缺少真实 Docker-capable `darwin-arm64` full-tier evidence；现已通过 self-hosted runner `oasis7-Mac-darwin-arm64-docker` 和 `Wasm Darwin Docker Evidence` main run `28297899706` 补齐 Darwin live summary bundle，并完成 Linux cross-host verify，因此跨宿主 closure 不再是 P0 阻塞。
  - 风险-4（2026-03-18 记录，2026-03-31 已收口）: `ReleaseSecurityPolicy` 的生产禁 fallback / 禁本地签名 / 禁 runtime source compile 不再依赖额外 `enable_production_release_policy()` 约定；production-facing `chain runtime execution world` 装载、`reward runtime worker`、`viewer runtime_live` bootstrap 与 `governance_registry_import` 新建/加载路径均已默认绑定 hardened policy。
  - 风险-5（2026-03-31 已收口）: `apply_domain_event_gameplay*` 中 replay / preflight / 恢复链路残留的 panic-style `expect(...)` 已改为结构化 `WorldError`，损坏事件与缺失 actor 回归已补齐，不再把状态漂移故障降级为 panic。
  - 风险-6（2026-03-31 已收口）: runtime 热路径超限文件已按语义边界拆分，`action_to_event_core.rs` 与 `apply_domain_event_main_token.rs` 均已回到治理线内，并通过编译/定向回归验证“拆文件不改语义”。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-WORLD_RUNTIME-001 | TASK-WORLD_RUNTIME-001/002/005/049 | `test_tier_required` + `test_tier_full` | 回放一致性、核心边界验收清单校验 | 世界状态演化与确定性语义 |
| PRD-WORLD_RUNTIME-002 | TASK-WORLD_RUNTIME-002/003/005/006/054 | `test_tier_required` | WASM 接口兼容性检查、治理流程测试、builtin wasm `sha256` 与 identity 清单一致性校验、初始化失败结构化错误回归 | 模块升级、工件治理与生命周期稳定性 |
| PRD-WORLD_RUNTIME-003 | TASK-WORLD_RUNTIME-003/004/005 | `test_tier_full` | 收据签名校验、安全回归抽样 | 审计可信性与安全边界 |
| PRD-WORLD_RUNTIME-013 | TASK-WORLD_RUNTIME-030/031/032/034 | `test_tier_required` | retention / GC / footprint budget 回归 | execution bridge、execution world、CAS 持久化 |
| PRD-WORLD_RUNTIME-014 | TASK-WORLD_RUNTIME-030/031/032/033/034 | `test_tier_required` + `test_tier_full` | latest-state restart、checkpoint replay、GC fail-safe、checkpoint 保留验证 | 恢复能力与审计链路 |
| PRD-WORLD_RUNTIME-015 | TASK-WORLD_RUNTIME-030/032/033/034 | `test_tier_required` | storage profile / metrics / archive read / launcher 脚本透传验证 | 发布链路可观测性与环境预算治理 |
| PRD-WORLD_RUNTIME-019 | TASK-WORLD_RUNTIME-038 | `test_tier_required` | 工厂生产阻塞/恢复/完成状态回归、事件历史可解释性断言 | 前期工业引导、Viewer 工业反馈、QA playability 解释链 |
| PRD-WORLD_RUNTIME-020 | TASK-WORLD_RUNTIME-041/042 | `test_tier_required` | Docker builder image、containerized canonical packaging、single canonical token 验证 | wasm publish build entry、容器环境收敛 |
| PRD-WORLD_RUNTIME-021 | TASK-WORLD_RUNTIME-041/042/043 | `test_tier_required` | build receipt / identity / release evidence 绑定验证 | 工件治理、发布证据与社会层可验证性 |
| PRD-WORLD_RUNTIME-022 | TASK-WORLD_RUNTIME-041/043/044 | `test_tier_required` + `test_tier_full` | multi-runner Docker compare、source compile 外移或 gated、runtime binary-only policy 验证；production entry 必须补 release policy 绑定证据 | build drift 阻断、执行前合法性与源码包发布边界 |
| PRD-WORLD_RUNTIME-023 | TASK-WORLD_RUNTIME-054 | `test_tier_required` | 生产入口 policy 绑定审计、默认构造路径巡检、release profile 回归 | runtime 默认安全边界、发布入口一致性 |
| PRD-WORLD_RUNTIME-024 | TASK-WORLD_RUNTIME-055 | `test_tier_required` | 损坏事件 / 缺失 actor 回归、preflight/replay 错误语义断言 | 恢复链路、事件预演、结构化故障定位 |
| PRD-WORLD_RUNTIME-025 | TASK-WORLD_RUNTIME-056 | `test_tier_required` + `test_tier_full` | 热路径文件长度治理、拆分后 determinism / replay / persistence 回归 | 规则演进可维护性、运行时热路径复杂度 |
| PRD-WORLD_RUNTIME-026 | TASK-WORLD_RUNTIME-061 | `test_tier_required` | `/v1/chain/status` 流量快照、UDP gossip / libp2p replication counters、范围说明与定向回归 | 节点带宽归因、运行态网络可观测性 |
| PRD-WORLD_RUNTIME-027 | task_370ce55ed73a490797055403164e8f41 | `test_tier_required` | triad traffic history 持久化、最近 N 分钟窗口汇总、`observed_since_unix_ms` reset-aware baseline 选择、live triad 短窗口采样验证 | 节点最近窗口流量归因、triad 运营监控可回答性 |
| PRD-WORLD_RUNTIME-028 | task_7863b156fce3484481310b33a263cc7c | `test_tier_required` | repo-owned node start wrapper、env-gated local traffic monitor、dry-run CLI 装配验证、单节点 live status 采样验证 | 节点级监控开关、真实环境启动入口可维护性 |
| PRD-WORLD_RUNTIME-029 | task_df0a42e3efea4806bb3f41245c1ef4d5 | `test_tier_required` | `fetch-commit` peer cooldown 定向回归、协议级范围约束断言、`doc-governance-check` 与 `git diff --check` | libp2p 复制流量浪费收口、真实 triad gap-sync 降噪 |
| PRD-WORLD_RUNTIME-030 | task_a28db8372d864bde9a9c5ea508bd7824 / task_fb91c7de413747b9b14712211f6666ea | `test_tier_required` | `oasis7_net` peer-record/discovery cooldown 与 discovery-query budget 定向回归、fallback proxy 连续性断言、`cargo check --tests -p oasis7_net -p oasis7_node -p oasis7`、`doc-governance-check` 与 `git diff --check` | peer-record/discovery 请求与 discovery DHT 控制面流量降噪、真实 triad 连续触发抑制 |
| PRD-WORLD_RUNTIME-031 | task_c1149e15fef14f12925182a03f37e546 | `test_tier_required` | `oasis7_viewer_live` chain-linked 被动跟随回归、不开 `Play` 的 committed world 同步、空轮询不推进断言 | viewer live 与 chain runtime 的逻辑 world progress 一致性 |
| PRD-WORLD_RUNTIME-032 | task_53b1918a361445f5bf678bcf525abc5c | `test_tier_required` | storage challenge `fetch-blob` success cache 定向回归、缓存过期恢复探测断言、`doc-governance-check` 与 `git diff --check` | triad `fetch-blob` 重复成功拉取降噪、sequencer↔storage 热点流量收口 |
| PRD-WORLD_RUNTIME-033 | task_dd49ad3480d14922993ceb3acf2555c6 | `test_tier_required` | `/v1/chain/gameplay/submit` handler 回归、viewer chain-linked gameplay submit 回归、`cargo check`、`git diff --check` | viewer gameplay action 与 chain runtime committed world 的一致性闭环 |
| PRD-WORLD_RUNTIME-034 | task_5b736236fdf5404099ef1d1aec37beb1 | `test_tier_required` | `fetch-commit` success cache 定向回归、缓存过期恢复请求断言、校验失败不缓存断言、`doc-governance-check` 与 `git diff --check` | triad `fetch-commit` 重复成功拉取降噪、gap-sync 紧环路请求收口 |
| PRD-WORLD_RUNTIME-035 | task_0c817eead8024055b841eb1be55adac3 | `test_tier_required` | status payload `observability` 合同回归、web launcher probe/snapshot 透传回归、client launcher snapshot 消费回归、repo-owned node observability report 脚本验证 | 节点健康摘要统一真值、launcher/operator 可读观测面 |
| PRD-WORLD_RUNTIME-036 | task_f0830d708c3b4f7abeea8cecf73053e4 | `test_tier_required` | WASM observability 设计文档、根 PRD/project/README/prd.index 回写、`doc-governance-check` 与 `git diff --check` | WASM build/executor/router 热点归因、节点 status 可观测性专题入口 |
| PRD-WORLD_RUNTIME-037 | task_20b6ee42182247ccbebe6a6a2c2db469 | `test_tier_required` | `cargo test --manifest-path tools/wasm_module_observe/Cargo.toml --offline`、`cargo run --manifest-path tools/wasm_module_observe/Cargo.toml -- observe --spec crates/oasis7_builtin_wasm_modules/m1_rule_move/observability/module_observe.json`、`bash -n scripts/oasis7-wasm-module-observe.sh`、`doc-governance-check`、`git diff --check` | 模块级 contract/perf 证据标准化、未来 wasm 模块接入路径 |
| PRD-WORLD_RUNTIME-038 | task_c79092f4b50d4d52a36b11fe0fe5eb5e | `test_tier_required` | `libp2p` substream wire-byte 计数定向回归、chain status schema 回归、repo-owned traffic summary/observability 脚本口径更新、`doc-governance-check`、`git diff --check` | control-plane byte counters 真值补齐、payload 与非 payload 的可解释边界 |
| PRD-WORLD_RUNTIME-039 | task_f15a1d9ea3194c39aa35158bf93d8ff1 | `test_tier_required` | `/v1/chain/status.consensus` commit freshness / pending proposal / queue pressure / inbound timing reject 回归、`cargo check -p oasis7_chain_runtime --bin oasis7_chain_runtime`、`doc-governance-check`、`git diff --check` | 节点共识堵塞分类、提交停滞与队列压力真值补齐 |
| PRD-WORLD_RUNTIME-039 | task_191e827b3ba84711bd0572504aea8251 | `test_tier_required` | transfer lifecycle/latency summary 回归、recent finality latency / payload-byte status payload 回归、`cargo check -p oasis7_chain_runtime --bin oasis7_chain_runtime`、`doc-governance-check`、`git diff --check` | transfer 提交确认时延、finality 最近窗口与 submit buffer/queue payload pressure 真值补齐 |
| PRD-WORLD_RUNTIME-040 | task_313368c409c54cc2bcf8ef4f47919b65 | `test_tier_required` | 首个 claim auto-funding / claim runtime 回归、`oasis7_chain_runtime` `/v1/chain/agent-claim/**` claim submit API 回归、viewer claim snapshot 自动资助字段回归、`software_safe` 正式摘要去审批化回归、`doc-governance-check`、`git diff --check` | 新账号首个 agent onboarding、专用池自动资助真值、slot-1 restricted funding 直连 claim 闭环 |
| [PRD-WORLD_RUNTIME-041](#prd-world-runtime-041) | [数值安全专业合同](runtime/runtime-numeric-safety.prd.md) | `test_tier_required` + 受影响边界回归 | [数值设计验证映射](runtime/runtime-numeric-safety.design.md#numeric-safety-validation)：需求与实现回链、checked failure/rollover/clamp；`cargo test -p oasis7_net --lib duration_millis_to_i64_saturating` 定位现有 clock 边界测试；文档修补不代表本轮执行或全域数值审计完成 | world、consensus、node、replication、membership 与 net clock 的既有数值合同 |
| PRD-WORLD_RUNTIME-043 | task_bb59399112634bdb994530662adfdc4b | `test_tier_required` + `test_tier_full` | schema round-trip、相同 intent 不同 root、fan-in/fan-out parent linkage、wrong/missing-root fail-closed、terminal-late、retry/recovery/replay idempotency、snapshot+journal state-root equivalence | 工业 action/event schema、apply paths、persistence/replay 与 Viewer/API projection |
| PRD-WORLD_RUNTIME-044 | task_35b6789c87be4eaca9d39182663728c9 | `test_tier_required`（current，AC-42；AC-44 partial evidence） | `cargo test -p oasis7 --lib execution_transaction_regressions -- --nocapture`；覆盖普通 tick、真实 module state write 后的后续失败、committed-context tick 的失败不发布，以及成功 snapshot/journal replay 等价；只证明公开 tick staged publication boundary，不代表 AC-44 的所有 world-effect 入口已统一 | 当前 Kernel/Institution 分类与公开 tick staged publication boundary |
| PRD-WORLD_RUNTIME-045 | task_35b6789c87be4eaca9d39182663728c9 | `test_tier_full`（target，AC-42/43/44/45/46） | 计划中的 canonical fixture `tests/fixtures/world_runtime/institution-migration-v1/` 与目标入口 `cargo test -p oasis7 institution_migration_v1 -- --nocapture`；该 fixture、runner 与 `report.schema.json` 在目标实现落地前不存在，不得作为 current/proven 证据；完成后须产出 manifest/input-root、execution receipt、snapshot/replay/recovery、legacy-conformance 与 outbox crash/conflict reports，并验证同 root/receipt、双实例隔离、失败零扣费、accepted exactly-once debit/effect/receipt、same-key 去重、different-payload `effect_id_conflict`、restart/replay 不重复外部效果 | 目标 Kernel/Institution migration、统一 transaction、module instance、economy 与 external-effect recovery |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-WR-001 | 以确定性回放作为运行时验收核心 | 仅执行结果正确即可 | 可追溯和审计需求要求强确定性。 |
| DEC-WR-002 | WASM 生命周期走治理流程 | 模块直接热替换 | 无治理热替换难以保证安全与一致性。 |
| DEC-WR-003 | 安全事件必须输出可验证 receipt | 仅日志文本记录 | 签名收据可支撑事后审计与取证。 |
| DEC-WR-004 | 每个 wasm 模块通过 module-local observe spec 接入共享 runner | 每个模块自写 perf/contract harness | 标准入口更适合长期扩展，也能让新模块自然继承观测与契约测试能力。 |

## 7. 模块执行、市场与历史 gap 合并边界

- `ModuleInstallTarget` 仅包含 `SelfAgent` 与
  `LocationInfrastructure { location_id }`。`InstallModuleToTargetFromArtifact`
  及其携带 finality 的动作显式携带目标；legacy
  `InstallModuleFromArtifact` 动作统一归一化为 `SelfAgent`。
- location-infrastructure 目标要求 `location_id` 非空且已经存在，并且
  installer 当前位于该 location；任一校验失败都不得写入 installed-module
  状态。`ModuleInstalled` / `DomainEvent::ModuleInstalled`、pending/release
  状态与 installed instance 持久化 `install_target`；历史字段缺失时按
  `SelfAgent` 反序列化，使 legacy action、event、snapshot 与 replay 保持原
  行为。
- target selection 必须沿 validated action -> domain event -> state apply
  进入权威状态并可重放；基础设施实例使用其记录的 infrastructure tick
  origin。实例按稳定 `instance_id` 顺序执行，安装、升级、重放和恢复不得
  退回 `module_id` 全局单实例覆盖。详细生命周期合同见
  `doc/world-runtime/module/module-lifecycle.md#稳定实例交易与升级合同`。
- 默认 world 保存/加载包含 module registry、metadata 与 artifact；旧目录缺少 module store 时仍可兼容加载。升级必须保持 module identity，并通过 owner、interface/export、subscription、capability 与 limit 兼容检查；失败不得产生部分状态。
- 生产 runtime 只消费 external Docker canonical builder 产出的 binary + receipt，并拒绝 runtime source compile。源码包文件数、大小、路径、超时和最小环境限制只属于显式 dev/test opt-in guardrail，不构成“世界内生产编译”承诺。
- 模块调用计费由确定性输入/输出/effect/emit 计量产生审计事件；余额不足时在输出、状态与 emit 应用前结构化拒绝。费用只在成功路径生效，replay 应用已提交的计费事件，不重新推导价格。
- 模块 artifact 的 listing、bid、purchase、delist、destroy 与 fee 是 runtime 权威动作/事件/状态：只允许当前 owner 操作，结算需资源充足，成交转移 ownership 并清理相关挂单，仍被 active instance 使用的 artifact 不得销毁。价格、玩家价值与恢复提示由 game 专业权威拥有。
- live/viewer 只回放共识已提交的有序动作；空轮询不构成逻辑世界推进。共识 action root、payload 完整性与提交验证由 P2P 专业权威拥有，LLM/provider 只提交意图，不成为第二执行权威。
- 五类 raw agent-intent lifecycle event 必须以完整 event body 绑定 sparse prepared delta：只拥有受影响 AgentCell 与至多一个 ledger replacement，completed transition 在 state projection 前绑定 prospective event id 与已提交 receipt witness；canonical root 必须包含 candidate intent/ledger 和恰好一次 mailbox route。失败不得消费 event id 或修改 ledger、slot、mailbox、journal、retention、consensus；provider advisory、historical no-op 与 public 多事件 chat workflow 的既有边界不变。
- Raw resource transfer、data collection、authenticated collection 与 data-access grant/revoke 必须先生成完整 event-bound sparse delta，再进入 publication。Delta 只拥有受影响 AgentCell、单个 permission/nonce 子项和 compatibility world-material replacement；prospective root 包含恰好一次既有 mailbox routing。校验或 publication 失败不得修改资源、nonce、permission、activity、mailbox、journal、allocator 或 consensus，且不得把 action-only 限制引入 raw reducer。
- Raw power redemption outcome 也必须先生成完整 event-bound sparse delta。成功 redemption 原子拥有 node credit balance、protocol reserve、redeem nonce、target electricity 与相关 activity；rejection 保持无校验且只触碰存在的 node/target activity。两者只向存在的 target mailbox 投递一次，不得把 action signature/policy 校验下沉到 raw reducer。
- Raw node-points settlement 必须在 publication 前完成 full-event-bound preparation：node credit、mint records、epoch budget、node-service treasury、账户余额、token supply、bridge epoch record 与 compatibility material ledger 作为一个 compound delta 原子安装。Prospective root 只借用未触碰状态并序列化 sparse replacements；该事件不路由 mailbox，action preview 复用同一 pure preparation，replay 复用同一 reducer/projector。
- Raw main-token genesis、vesting claim、transfer、epoch issuance 与 fee settlement 必须以完整事件绑定的 monetary delta 发布。该 delta 原子覆盖 supply、账户、genesis bucket、claim/transfer nonce、epoch issuance 与 treasury 的实际触碰项，并在 borrowed prospective root 中加入 vesting beneficiary 或 transfer 原始 source 的既有单路 mailbox；action preview 与 replay 复用 pure preparation，失败不得留下 supply 或 treasury 半写入。
- Raw main-token policy scheduling 与 treasury distribution 必须以完整事件绑定的 governance-monetary delta 发布，原子覆盖 scheduled policy 或 treasury、账户、supply、distribution record 及 compatibility materials；两者均不路由 mailbox，action preview 与 replay 复用 pure preparation。
- Restricted starter-claim 的 pool top-up、grant issue、expiry 与 revoke 必须以完整事件绑定的 sparse delta 发布，原子覆盖 treasury、beneficiary balance、supply、grant/top-up record、compatibility materials 与既有 controller/issuer mailbox；expiry 不路由，缺失路由 Agent 时保持 no-op。
- Raw `StarterOcClaimed` 必须以完整事件绑定的独立 sparse delta 发布，原子覆盖 starter claim、目标账户与 activity、token supply、可选 treasury bucket 和 compatibility materials；prospective root 包含 raw agent 的单次既有路由。Raw 路径保留 treasury 与 mint 两种 funding 语义，不引入 action-only 的玩家、公钥或全局唯一性限制。
- Raw claim release request、grace entry 与 idle warning 必须以完整事件绑定的轻量 lifecycle delta 发布，只替换目标 claim 与 compatibility materials；仅 release request 更新 claimer activity，三者均只向 raw claimer 路由一次，且不得修改 target activity、货币字段或全局 claim epoch cursor。
- Raw `AgentClaimed` 与 `AgentClaimUpkeepSettled` 必须以完整事件绑定的 economic delta 发布，原子覆盖 claim、全局 processed-epoch cursor、claimer account、supply、实际触碰的 treasury entries、claimer activity 与 compatibility materials；prospective root 只加入一次 claimer 既有路由，target 保持不变，action preview 与 replay 复用 pure preparation。
- Raw `AgentClaimReleased` 与 `AgentClaimReclaimed` 必须在删除 claim 前完成完整事件绑定的 terminal preparation，原子验证并安装 refund provenance/sink、upkeep/slash treasury、claimer account、circulating supply、processed-epoch cursor、claimer activity 与 compatibility materials；只路由 claimer 一次，target 不变，raw reclaim reason 保持 informational，tick 继续逐事件重读状态。
- Raw economic-contract open、accept、settle 与 expire 必须以完整事件绑定的 sparse delta 发布，原子覆盖 contract、重叠参与者 AgentCell、world resource treasury、reputation、pair cooldown、reward windows 与 compatibility materials；只按既有 raw `agent_id` 路由一次，保留 open overwrite、expired payload parties、raw notes/deltas 等宽松语义，不下沉 action policy，tick expiry 继续按 contract ID 排序逐事件发布。
- Raw alliance form/join/leave/dissolve 与 war declare/conclude 必须先形成完整事件绑定的 sparse delta。该 delta 原子覆盖 touched alliances、wars、participants、reputation 与 compatibility materials；prospective root 只加入既有 raw actor 的单次路由，conclusion 保持无路由。所有成员、资源和 outcome 的既有校验顺序必须在 publication 前完成，失败不得留下 alliance/war/resource/activity 半写入。
- Raw governance proposal、crisis、meta-progress 与 product-validation publication 分别使用窄的 full-event projector：proposal 保留 duplicate-open votes 与 recast 语义，crisis 保留 legacy resolve/spawn/timeout payload authority，meta self-target 合并为一个 AgentCell，product requester 缺失仍提交 validation 且不路由。所有 prospective roots 仅覆盖对应 map 与既有单次 mailbox route。
- 本节吸收已完成 README gap 专题中的仍有效 runtime 合同；历史里程碑、旧路径与完成状态从 Git history 和 PR 与实际验证记录 追溯。

<a id="module-store-persistence-and-recovery-contract"></a>
本锚点是旧 ModuleStore persistence and recovery contract 的兼容定位，原有 load/save/late-record/error/cache/instance 条款保持原节全文，接受关系消费该节而非通用工业矩阵。

### 合法退休、实例升级与共享内容的可恢复性

以下补齐 [#4417](https://github.com/eng-cc/oasis7/issues/4417) 的独立验收边界，是应满足的合同；既有局部原子发布、普通 round-trip 或文档通过不等于这些实现已完成。存储、兼容与测试设计见[模块生命周期分册](module/module-lifecycle.md#module-retirement-and-shared-content)。

- **合法退休仍可读**：通过真实安装形成 registry 的 inactive artifact，在没有任何 active instance 或 legacy active 引用时合法销毁，必须保留历史 registry、实例身份和原有审计，并使销毁后的 `snapshot/from_snapshot`、`save/load` 与 canonical CAS restart 得到一致可读结果。退休模块不能恢复执行；其他模块正常。不能删历史 registry、把所有缺 bytes 都视为退休，或等重启失败后才承认该已提交状态不完整。
- **退休与损坏分开**：当前可执行集合、历史 registry 与可恢复的工件闭包是三种信息。只有受支持持久格式中可验证的退休事实，才允许对应历史记录缺少当前执行 bytes；active 引用或未退休记录缺 bytes 仍拒绝。保留中的历史 checkpoint/replay 所需 bytes 继续由既有保留合同管理，不能随当前 artifact 销毁一起误删。
- **退休可由 journal 重建**：成功销毁的退休事实必须与销毁事件在同一提交中持久记录，绑定具体 artifact 生命周期；从销毁前 checkpoint 重放成功动作 tail，与直接恢复销毁后 snapshot/目录/CAS 得到同一结果。旧的单条 raw destroyed event 保持 state-only 兼容，不能作为动作退休的充分证据；新格式缺失、截断或错配的退休记录不能恢复出合法退休。具体记录及原子边界见[动作退休 journal 设计](module/module-lifecycle.md#module-artifact-retirement-journal)。
- **实例升级可复用版本**：A/B 两实例从 v1 依次正常升级到同一 v2 时，目标已注册且完整 manifest 一致即可复用，不能重复注册或覆盖它；源版本依据被升级实例的历史绑定，不依据另一个实例已改变的全局 active 指针。每次保留该实例 ID、owner、target 与独立 state，且不改变另一实例。错误 owner/from/manifest、冻结、不兼容或无效 finality 继续拒绝且不扣升级费；不能用 rollback 事件或冻结正常 v1 代替 upgrade。
- **共享内容不丢 manifest**：`wasm_hash` 只标识内容，同模块不同 version、不同模块可以共享 bytes，但各自完整 manifest identity 必须保留。标准保存后的 Observer 与普通恢复均能读取全部合法记录，包括 inactive 记录；不以最后写入的 `<hash>.meta.json` 覆盖结果当作唯一 manifest。坏 bytes、错误 manifest identity 仍拒绝，Observer 不写源目录。

| 独立验收 | 最小可判定场景 | 不足以代签的样例 |
| --- | --- | --- |
| 退休闭包（P1） | 真实安装 inactive A，保留另一合法模块 B，销毁 A 后分别做 snapshot、目录、canonical CAS restart；核对历史记录、A 不可执行和 B 正常，再验证 active 销毁与未退休 missing bytes 负例。 | registry 为空的 artifact fixture、只有销毁前 baseline+tail、单独缓存读取。 |
| 动作退休 tail 恢复（P1） | 由销毁前 checkpoint 重放已成功提交的销毁事件及绑定退休记录，再保存/恢复并与直接销毁后状态比较；A 不可执行、B 正常，重放不重复收费。 | 单条 raw destroyed event、手填退休字段、缺失/错配/截断的新格式单元、只测试销毁后 snapshot。 |
| 多实例升级 | A 升级 v2 后保存/恢复，再升级仍在 v1 的 B；比较两者身份/state、收费与 upgrade 事件，并覆盖上述拒绝路径。 | 单实例升级成功、用 rollback 或重装生成新实例。 |
| 共享内容 | 同模块两版本及不同模块共用 H，标准 writer 保存后通过真实 Observer 加载；单 active/全 inactive 与受支持 inline/non-inline 各测一次，覆盖错误 hash/identity。 | 仅普通 inline round-trip、新 service 旁路不读该格式。 |

<a id="gameplay-mode-identity-contract"></a>
### Gameplay mode 的统一判等

补齐 [#4497](https://github.com/eng-cc/oasis7/issues/4497) 所承接的 mode 边界：manifest 原文及其 hash 保持不变，所有 mode 校验、active-slot 冲突检查、readiness 查询与恢复后投影使用同一派生 key `mode.trim()`；不额外做大小写或其他字符归一化。manifest 声明的空 key 拒绝，单 manifest 中派生 key 重复拒绝；查询保留现有空输入行为，不由此新增错误返回。同 kind 的 `sandbox` 与 ` sandbox ` 占用同一槽，不得双重激活，也不得出现激活成功却查询不到的覆盖。

这是现行合同的一致性目标，不声称当前所有消费者已经统一。register/activate/deactivate/upgrade 及 snapshot/replay 后以相同输入判定；旧合法原文字节可读，恢复不得静默改写 manifest、覆盖冲突或任选一个 active record。测试分别覆盖单条带空白的合法声明、单 manifest 重复、跨 manifest 同槽冲突、不同 kind 合法覆盖和恢复后的相同 readiness；设计接线见[派生 mode key](module/module-lifecycle.md#gameplay-mode-key-design)。

## 基础设施专业接受与事实边界

本文保留所有既有 PRD-ID、SC、AC、专题/实现历史及 current/legacy/target 条款。本节承接产品基础设施结果，属于目标专业合同；文档采纳不改变 §工业执行矩阵的当前字段、不使 §ExecutionTransaction 的整体 partial 能力成为 proven，也不替代已执行证据。产品根 SC-8/9 与本文旧 SC-8/9 使用各自 path#fragment 身份，不能混用编号。批准阅读基线为 eng-cc/oasis7 的 f9d5a552d9af04c1b1398262808198a58e560230；后续相关输入须独立重审。

<a id="runtime-deterministic-acceptance"></a>
### 确定性与原子拒绝的专业接受
承接 [REQ/AC-DWE-001](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) 与产品根 SC-1/4：同 world、governing version、ordered input、parent committed state 的 action root/execution hash/state root/receipt-journal binding 必须一致。`bft` 下活动验证者在 attestation 前各自重执行；缺证、冲突、越权、artifact/版本不一致不能产生部分 receipt、资源或状态效果。runtime 提供执行绑定，P2P 提供合法 profile/epoch、提交证明与持久条件；`bft` 另有活动集合、round/threshold 检查，适用证书负例各自阻断。见 [确定性设计](design.md#runtime-deterministic-design)。本地 root 一致性不代签 SC-1/4 的 full 同候选组合证据。

<a id="runtime-pending-acceptance"></a>
### 无效果待决与当前条件重审
承接 [REQ/AC-DWE-002](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002)、DE-2 与产品根 SC-5。签名送出、transport 接收、排队、重连或 UI 成功都不产生资源、控制权、资格、声誉、阶段或依赖结果。runtime/P2P 必须持久保留可读的原请求身份及真实待决处置；恢复可服务后从 fresh authoritative snapshot 读取当前权限、资源、期限与前置条件，在 canonical 顺序重新裁决为执行、拒绝、过期或须重规划。只有对应 committed receipt 更新世界结论。等待不授予完成时间、返还或旧优先级；消费者可查看/等待/重规划，只有专业域支持时才可明确撤回/替代。见 [待决设计](design.md#runtime-pending-design)。现有 effect/cognition pending 路径不证明所有 signed finality-pending 请求持久化。

<a id="runtime-lineage-acceptance"></a>
### 互斥 lineage 的唯一有效胜者
承接 [REQ/AC-DWE-003](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-003)。专业域明确标记互斥的同 lineage 原请求、withdrawal、replacement，由首个产生有效世界效果的 committed receipt 唯一获胜，在同一个原子边界终止其余成员为无效果、不可执行、可追溯。拒绝/过期仅终止自己，未标记互斥的独立 intent 可并发。撤回/替代在自身 committed receipt 前不能取消、隐藏、覆盖或宣称优先于原请求；关联和各自真实状态可读。retry/reconnect/restore/replay 只能读取同一处置，不产生第二 sink/receipt/effect。见 [lineage 设计](design.md#runtime-lineage-design)。普通 operation idempotency 或签名真实性不是 lineage 原子仲裁；标记、排序和 schema 的批准仍是外部依赖。

<a id="runtime-version-acceptance"></a>
### Canonical execution block 的版本接受
承接 [REQ/AC-DWE-004](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)、DE-4、产品根 SC-4。首次进入已 committed 且 finality-verified canonical execution block 时，相对治理 activation boundary 确定 governing manifest：boundary 前旧版本，at/after 新版本。candidate/proposed block、提交时间、client compatibility declaration 或节点软件不能选版。激活前待决但激活后首次执行的请求必须按新 manifest 当前权限/资源/前置条件重裁决；不兼容只能原子拒绝/过期或主体明确提交新兼容请求，禁止静默翻译、旧报价/资格或混合规则部分效果。明确跨版本 replacement 进入原互斥 lineage，未关联新请求只有经专业域确认独立才可并发。历史 receipt 按原 block manifest/artifact replay；缺失/冲突 activation proof、artifact 或 compatibility 阻断执行与恢复，保留已确认历史和真实未确认处置。见 [版本设计](design.md#runtime-version-design)。四条 release lane 保持独立；普通 rolling node release 不激活世界语义。

<a id="runtime-recovery-acceptance"></a>
### 同世界恢复与重新服务的专业接受
承接 [REQ/AC-DCS-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)、[REQ/AC-DCS-005](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005)、DC-2/5、DE-3 和产品根 SC-2/3/6/7。bootstrap/snapshot/replay/state-sync/pruning/disaster 各自证明同 world identity → finalized checkpoint/registry → hash-bound snapshot → canonical replay → state root。缺失/冲突/其他 world 候选保持原世界不可用/隔离，不接受新权威效果、不静默确认/取消/重放/迁移未 final 请求；旧 receipt 保持原历史。替代世界与受治理迁移是独立产品决策。
历史验证只允许作恢复只读的必要依据；追加、最终性、当前版本化执行及单调 head 连续性共同重新成立才能开放新 intent。失败闸门后每个受测新 intent 的 committed receipt 为 0；可服务后 pending 当前条件重审，无期限/优先级继承，每个新 intent 至多一 committed receipt、无第二效果。闸门在提交/恢复中回退到只读/隔离，不撤销/改写已确认历史。正式消费者读到等级、主要 blocker、下一步。见 [恢复服务设计](design.md#runtime-recovery-design)。本地 bridge v3/CAS root 与 R0/R1 不是 checkpoint-finality 或开放写入证明；pruning 的冗余 archive/角色权限由 P2P/ops 承担，SC-1/2 的完整网络职责不移入 runtime。

<a id="runtime-industrial-acceptance"></a>
### 工业跨阶段因果与有界容量接受
承接产品根 [工业跨域边界](../product/world-infrastructure/prd.md#工业流水线的跨域执行边界) SC-8/9，既有 PRD-WORLD_RUNTIME-043 不变。代表性 factory → recipe/input join → handoff → transit → output/terminal 在同 world/version/immutable root 产生 accepted/blocked/terminal。child 的 root/revision/parent/目的状态在首个不可逆 sink 前校验，缺失/冲突 fail closed；accepted 不是原料/容量 reservation。hold 必须有界、独占、绑定 root/阶段/边/批次；中间 edge 与 destination buffer 满载只可保留未消费投入、接收仍有容量的已结算产出，或原子拒绝/延期新承诺，不能丢弃/瞬移/无限积压/隐式改道/伪造完成。
同一 authoritative release/arrival 对同 hold 的释放及 fresh-snapshot 重评各最多一次；后续不同事件只对仍有效 intent 的未满足剩余量从新权威快照重评。因果变化生成 parent-linked revision/child root；非因果 checkpoint 不改 root。retry/reconnect/restore/replay 重读同 disposition，不重复 sink/credit/progress/receipt/reward。消费者能读最早 blocker、held/consumed/unmet/residual 与 next recheck；局部完成/在途/缓存不等于生产或交付。M4/gameplay 拥有容量、资源、window/W 规则，runtime 拥有事件/持久化/原子执行。见 [工业设计](design.md#runtime-industrial-design)。现有 ActionId、ready_at、consume/produce vector 不重命名成尚缺的 root/join/window/bundle 字段。

<a id="runtime-world-scope-acceptance"></a>
### 世界身份与消费者作用域
承接产品根 SC-10 [基础不变量](../product/world-infrastructure/prd.md#基础不变量)。提交前消费者区分 global-authoritative 与隔离 local/development 目标；intent/committed receipt/result 绑定同 world。缺失/错 world 或 local receipt reuse/replay 不接受、不产生、不呈现 global effect；local 历史永不合入 global。见 [身份设计](design.md#runtime-world-scope-design)。scope 呈现由 Agent/Viewer/API 专业 authority 定义，runtime 不新增 UX/schema。

### 专业验收接收边界
上面每一专业接受都由 root design 的 §2.1 与 §11.1 分配到真实条款和验证场景。既有其他 PRD/AC/历史 Test Plan 不删除、不因新增产品关系被覆盖。pending、lineage、version、service gate 及完整工业 root/hold 仍是 target；现有分散的 typed publication/current bridge 属 current/partial。定义测试、文档 active、PR merged 与实际执行证据是独立事实；完整 SC/DC/DE 组合结果需要同一 candidate/config/world/entry/environment/evidence window 的 runtime/P2P/ops/consumer/QA 证据。
