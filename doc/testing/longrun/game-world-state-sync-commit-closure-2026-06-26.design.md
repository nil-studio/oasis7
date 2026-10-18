# oasis7: Game World State Sync and Commit Closure Design

- 对应需求文档: `doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

审计轮次: 1

## 1. 设计定位
本设计补足 S9A 与 S10 之间的可读桥梁：单节点 runtime 合同只能证明本地执行正确，多节点状态同步/提交闭环必须证明 committed world state 能在 sequencer、storage、validator、observer 之间持续传播、恢复并投影。

## 2. Claim Boundary
| Claim | 最低证据 | 可以声明 | 不得声明 |
| --- | --- | --- | --- |
| `module_required` | runtime required + node/net/libp2p/consensus/distfs + mixed-topology required | 本地合同可集成 | 真实多节点持续稳定 |
| `module_full` | mixed-topology full + triad longrun + state-sync closure | proxy/triad 下可推进和恢复 | real-env/public-testnet ready |
| `integration_required` | 真实游戏 seed/world state + S10 或等价多节点 + API/viewer projection | 游戏世界状态链路无明显漂移 | 公开网络 readiness |
| `release_full` | real-env/public_testnet readiness lane + 同窗口证据 | live candidate 候选信心 | mainnet 或外部市场信心 |

## 3. Test Matrix
| 层级 | 节点/拓扑 | 覆盖对象 | 关键检查 |
| --- | --- | --- | --- |
| Phase 1 | single node | action、execution record、receipt、state hash、replay | same input same result; checkpoint/rollback recovers |
| Phase 2 | deterministic crates | node/net/consensus/distfs 合同 | libp2p path、consensus/finality、blob/store 基础合同 |
| Phase 3 | triad/proxy | commit propagation、peer heads、gap sync | committed height 单调、consensus hash 一致、peer heads 新鲜 |
| Phase 4 | observer/state sync | checkpoint、bundle、blob closure、observer catch-up | missing blob = 0; observer 自动追高 |
| Phase 5 | five-node game soak | real gameplay data、settlement、storage/observer roles | lag/stall/settlement/distfs gate 通过 |
| Phase 6 | real-env/release | same-window public_testnet/live candidate | readiness lanes pass; manifest 非 placeholder |

## 4. Evidence Shape
每次执行至少记录：
- command 和环境：git sha、world id、node ids、ports、duration、Bash 版本。
- commit 证据：submitted action、execution record、receipt、state hash、committed height。
- sync 证据：network height、peer heads、gap sync/state sync 结果、observer catch-up。
- closure 证据：state-sync bundle、blob closure report、missing blob count。
- projection 证据：`/v1/chain/status` 与 API/viewer projection 的同窗口样本。
- evidence packet：`doc/testing/templates/state-sync-closure-evidence-packet-template.md` 作为 `module_full` state-sync closure 汇总模板；不得把该模板文件本身作为 pass evidence。
- S10 summary contract：`scripts/s10-five-node-game-soak.sh` 在 `summary.json` 输出 `api_viewer_projection`，在 `summary.md` 输出 `API / Viewer Projection Contract`；默认 `status=not_collected`，真实 pass 需同窗口 API/viewer evidence refs。
- readiness lane binding：`scripts/network-tier-public-testnet-readiness.sh` 要求 `api_viewer_projection_ready` active lane；pass 证据不能是 template/placeholder。

## 5. Failure Routing
- `consensus_hash_divergence`: runtime/node/consensus 联合排查。
- `committed_height_not_monotonic`: runtime commit path 与 sequencer 状态排查。
- `known_peer_heads_zero_samples`: net/node peer-head publication 排查。
- `http_failure_samples`: node service health、status endpoint、port allocation 和 local process lifecycle 排查。
- `missing_blob_count > 0`: distfs/store closure 排查。
- `observer_catch_up_failed`: state-sync/checkpoint/observer bootstrap 排查。
- readiness lane `partial` / `block`: blockchain ops + QA 收口，不允许 release claim。

## 6. Integration Points
- `testing-manual.md#s9a链上大世界状态底座自闭环`
- `doc/testing/longrun/p2p-longrun-soak-and-chaos.prd.md`
- `doc/testing/longrun/s10-five-node-real-game-soak.prd.md`
- `scripts/game-world-state-sync-commit-module-required.sh`
- `doc/testing/templates/state-sync-closure-evidence-packet-template.md`
- `scripts/p2p-mixed-topology-matrix.sh`
- `scripts/p2p-longrun-soak.sh`
- `doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md`（runtime signed V2 checkpoint recovery contract）
- `crates/oasis7_node/src/replication_checkpoint.rs`（checkpoint descriptor、签名 replication message 与 blob closure authority）
- `crates/oasis7_node/src/node_engine_replication_checkpoint.rs`（network head/checkpoint identity binding 与 provider publication）
- `scripts/p2p-verify-state-sync-closure.sh`
- `scripts/s10-five-node-game-soak.sh`
- `scripts/s10-five-node-game-soak-summary.test.sh`
- `scripts/network-tier-public-testnet-readiness.sh`
- `doc/testing/templates/public-testnet-readiness-lanes.example.tsv`

## 7. Design Risks
- Proxy triad 是可执行近似，不是 dedicated physical network lab。
- 单次 short soak 可发现明显阻断，但不能替代 release endurance。
- API/viewer projection 只证明状态可见性，不证明玩法好玩或真实玩家意愿。
- state-sync closure report 只能证明 blob 引用闭包；observer 自动追高仍需单独证据。
- 手工复制数据目录、checkpoint 或 seed 被禁止作为 recovery 或 live-candidate readiness 输入；它们只能作为隔离的故障现场证据，不能继续接入正式 world state。

---

以下完整验证视图承接目标执行/恢复合同。以上原始 S9A/S10 基线条款与证据边界保留；新增 schema 是目标验收形状，不是已部署协议、自动 validator、运行通过或发行结论。Owner 是 `qa_engineer`；runtime/WASM/P2P/Agent/Viewer 各自保有执行、协议和消费者语义 authority。审读输入为 canonical repository `eng-cc/oasis7` 的专业合同与产品条款；候选实际 source/integration/tree、运行产物与判定由对应 PR 与实际验证记录 绑定，本文不维护动态任务台账。

## 1. 问题、目标与非目标

单个 receipt、可达 HTTP endpoint、完整历史链或 blob closure 不能证明恢复后的世界可以接受新写入。目标是将相同候选、世界/历史分支和 canonical 窗口的 execution、state-sync、intent 结果与消费者样本连接为可回读的结构化附件，逐一判定 DC-5 的服务状态、重审、零/单次 receipt 与真实下一步。成功条件是全部 schema 与语义检查以及相应 required/full 环境证据同时成立。

本文实际定义 QA target attachment schema 和验证算法、场景及 evidence receiver；它不实现 runtime、最终性、迁移、消费者、schema validator 工具或新 workflow。文档结构、fixture 验证与实现/full proof 分别成立。以下例子与 schema 的 shape pass 永不表示 DC-5 或 SC-9 已通过。

## 2. 上游约束与相关角色

产品定义单次效果、版本选择和可读状态；runtime 定义原子提交、lineage、恢复与 replay；WASM 定义目标 execution core/receipt/migration 和计量身份；P2P 定义证明和历史分支；QA 验证组合；Agent/Viewer 负责真实消费者。QA 不签发最终性、替代 gameplay 的 W/window 判定或把本地签名当作外部证明。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [AC-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | 合法 checkpoint 加非空事件后缀恢复后，在固定有效条件下保持与不中断对照相同的后续处理、行动时点和结算；至少一条合法新配方完成一次 | [正向续跑判定](#sr2-journal-suffix-recovery) | runtime 拥有恢复与续跑语义；P2P 拥有合法提交证明，QA 验证实际材料 | 至多一次上限不能代替正向完成；局部结果不证明 full 服务闸门 |
| [runtime 日志恢复合同](../../world-runtime/prd.md#runtime-journal-suffix-recovery) | 合法 finalized C<T 加真实非空 WorldJournal suffix，J1 action 身份与 J2 成功 module tick 分别恢复并按相同后续输入续跑，不重跑历史 WASM 或重复计费 | [正向续跑判定](#sr2-journal-suffix-recovery) | runtime 拥有 allocator、schedule/control、日志与续跑语义；QA 验证同候选对照材料 | 完整快照往返、committed input 重执行不能代签事件后缀恢复；J1/J2 不互代 |
| [doc/product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005) | DC-5 同候选 execution/state-sync 附件、服务转换、manifest/head 负例、每 intent 的 receipt 0/1 与消费者 blocker/next-step；可服务结论还须承接恢复后的正向续跑结果 | [doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-semantic-validator](game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-semantic-validator) | runtime/P2P 证明与消费者 readback，QA 组合验证 | schema pass 或零次结算不代表实际 DC-5 pass |
| [doc/product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-003](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-003) | linked replacement/withdrawal 竞态、幂等、互斥唯一有效赢家；拒绝/过期仅自身，独立请求可并发 | [doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix](game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix) | runtime 的持久 lineage 与 staged publication，Agent/Viewer 投影 | 现有 Agent chat supersession 不证明通用 lineage |
| [doc/product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-004) | 首次已 committed/finality-verified block 选版、激活后重审、原历史 manifest/schedule replay | [doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios](game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios) | runtime/WASM 激活、artifact 与迁移合同；P2P canonical inclusion | 本地提交/客户端版本不能选版，缺历史 bytes 不通过 |
| [SC-9](../../product/world-infrastructure/prd.md#sc-infra-009) | SC-9 两类 outage 在四边界成对注入，保持 baseline 与真实 provider/API/Agent/browser 环境义务 | [doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-sc9-eight-cells](game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-sc9-eight-cells) | gameplay W/window authority、runtime 恢复、Agent/Viewer 消费与 QA | generic WASM/schema/fixture 不关闭八个 cell |
| [REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | successful receipt core 与 detached inclusion、decimal U64/UTF8；仅承接确定性已提交执行的证据身份，专业约束仍由 [WASM execution contract](../../world-runtime/wasm/wasm-interface.md#sr2-execution-contract) 拥有；schedule/migration canonical identity 由下行独立接收 | [doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-attachment](game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-attachment) | WASM 字段与 hash domain、runtime publication；QA shape receiver | 新 schema 不改 current wasm-1 或 legacy JSON receipt hash |
| [REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) | schedule/migration canonical identity；仅承接版本激活与历史重放所需证据关联，字段与 hash 仍按 [WASM execution contract](../../world-runtime/wasm/wasm-interface.md#sr2-execution-contract)、[计量](../../world-runtime/wasm/wasm-interface.md#sr2-metering-contract)和[迁移](../../world-runtime/wasm/wasm-interface.md#sr2-migration-contract) | [目标附件](#sr2-dc5-attachment) | WASM/runtime 拥有版本、计量及迁移语义，P2P 拥有 canonical inclusion，QA 只接收证据 | 不改变 current wasm-1、legacy receipt hash 或产品版本规则；shape 不代签激活、历史 replay 或迁移成功 |

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线 | 目标 | 差距与证据限制 |
| --- | --- | --- | --- |
| state-sync lane | 原 §3–6 的 scripts、S10 summary、state-sync envelope 与 checkpoint locators | 同候选 execution/state-sync/consumer 实际 readback | 现有 template 和 not_collected 字段不是 pass |
| execution/receipt | 当前 EffectReceipt/ModuleRuntimeChargeEvent 与部分 prepared publication | 成功执行 receipt core、完整 lineage/activation/migration 及证明 | 代码局部回归不证明 target 全原子/外部 finality |
| attachment | 本 schema 实际约束字段、enum、required、附加字段与条件形状 | §6/§11 语义算法和全部适用环境完成 | schema 自身不是语义验证工具或 full 证据 |
| observability | local bounded status/timing 可支持采样 | 真实状态等级与 proof readback 的关联观察 | wall clock/health 不能选择费用、manifest 或写权限 |

本篇基线不把历史 Docker run、旧 CI 或 local fixture 重新标为本候选证据。运行 candidate 对象与 immutable evidence refs 必须在执行时冻结并回读。实现缺失是 explicit target/unproved，不由文档采纳推导消除。

## 4. 边界与结构

数据流为 `runtime execution evidence + P2P state-sync evidence + consumer samples -> QA attachment -> schema validation -> semantic verification -> tier-specific verdict`。箭头表示读取与验证，不表示 QA 修改世界。runtime/P2P 各自产生独立可验证的 scope/candidate/window/证明；消费者只呈现真实 committed/pending/replan 等状态；QA 保存原始 bytes、digest、length、locator、环境与判定。

新文件 [实际 target schema](../schemas/world-execution-state-sync-attachment.schema.json) 属于验收 evidence shape authority；现有 [state-sync envelope](../templates/state-sync-closure-evidence-packet-template.md) 仍是汇总 envelope，本篇仍是 execution lane。它们不能互相替代。schema `$id` 是标识，不要求线上下载，也不是 trust root。专业 field contract 链接 [WASM types](../../world-runtime/wasm/wasm-interface.md#sr2-types)、[execution contract](../../world-runtime/wasm/wasm-interface.md#sr2-execution-contract)、[pending](../../world-runtime/module/module-lifecycle.md#sr2-pending-contract)、[activation](../../world-runtime/module/module-lifecycle.md#sr2-activation-contract)、[external](../../world-runtime/module/module-lifecycle.md#sr2-external-contract) 和 [migration publication](../../world-runtime/module/module-lifecycle.md#sr2-migration-publication)。

## 5. 关键运行流程

1. 冻结 source commit、integration base commit、实际 tested tree、world/branch 与 canonical start/end 窗口；schema 版本、protocol/execution/test-contract version 明确。候选 tree 不由 source OID 或文件名猜测。
2. 在适用拓扑收集 serviceable→stale/catching-up→verified read-only→serviceable 和 proof conflict→isolated，另各覆盖恢复只读/可服务/受阻以及提交或恢复中的 gate rollback。每一 transition 收集 proof、last trusted boundary、manifest/head、compatibility 与真实消费者样本。
3. 对受测 intent 收集 authenticated request/lineage、当前恢复重审、decision、成功 receipt core 与 detached finality proof。transport ACK 和 pending 无成功 execution receipt；不支持替换的领域不得伪装撤回或隐式连线。
4. 将已归档 bytes 的 locator+digest+byte_length 放入附件，先验证 schema，之后执行 §6 语义步骤。缺失/冲突/不可回读时 blocked，不向更旧 green、缓存、另一个 world 或 source 候选回退。
5. 重试必须使用同 identity/request_hash，读回原 decision/receipt；同 ID 换 payload 或 orphaned proof 拒绝。gate 失效后保留已确认历史；恢复 pending 重新检查当前权限/资源/前置/expiry，不延期限、不继承旧价格/优先级。
6. 最后由 QA 对实际 layer/env 证据评估本次边界，保留失败签名和 raw artifacts；schema-only 结果只能报 schema shape verified。未知外部证明是 blocked/unverified，不能造 finalized rejection 或成功。

## 6. 接口与数据合同

<a id="sr2-dc5-attachment"></a>
### Target attachment 与共享类型

实际 schema 使用 Draft 2020-12 和 `WorldExecutionStateSyncAttachmentV1` literal；所有 object required 明确且 additionalProperties=false，未知版本/字段/enum/null 拒绝。`scope={world_id,branch_id}` 仅在每个 independently verifiable record 的 core 中定义一次；world branch 是 canonical 历史身份，不是 Git branch 名。

`CandidateIdentityV1` 精确字段为 `source_oid/integration_base_oid/tested_tree_oid/execution_version/protocol_version/test_contract_version`；前三者是 full lowercase 40-hex Git SHA-1 OID，前二解析为 commit、后一为 tree；`Hash32` 是 SHA-256 lowercase 64-hex JSON projection，对应 32 CBOR bytes，不能替代 Git OID。`Id` 是 1..256 UTF-8 bytes，不 normalize/trim；JSON minLength/maxLength 的字符计数只是 lexical guard，必须再检 UTF-8 字节长度。

所有 U64/PositiveU64、资源量与 balance 的 JSON projection 均为无符号十进制字符串，无前导零、浮点、指数；U64 最大值 18446744073709551615，PositiveU64>=1；schema pattern 精确约束 U64 最大值，语义步骤还须检查运算溢出和当前资源整数转换范围。不得将大于 2^53 的值通过 JS Number 往返。`JournalPositionV1={event_id_era,event_id}` 两个 U64 按 tuple 比较，不能把 era rollover 压成单索引。receipt/effective-world-effect count 是单独 bounded JSON integer 0|1。

`ContentRefV1={digest,byte_length,locator}`；读取并复算 bytes SHA-256 与长度。locator 不参与内容 hash，也不建立 authority。`ExecutionResultCoreV1` 与 `ModuleExecutionReceiptCoreV1` 按专业 field contract 明确定义；core 没有自己的 ID、未来 containing-block hash、signature 或 finality wrapper。`ExecutionIdentityV1` 与 `ModuleExecutionReceiptV1` 是 detached canonical inclusion wrapper，scope 继承 core，proof 必须将 exact execution_id/receipt_id 绑定同一个 committed block 与 activation。`ModuleExecutionReceiptV1` 仅 committed_success；canonical no-effect reject/expire/replan/loser 是 `IntentDecisionV1`，不计 execution receipt、不赢 lineage。

`PendingIntentV1` 使用 `member_generation`，不同于 subject/capability generation/provider key epoch；runtime 赋予 checked predecessor successor，非优先级。linked_replacement/withdrawal 必须携带 predecessor intent/hash 和领域已授权 exclusive_set_id；original/independent 不携带 predecessor。`ExternalEffectReceiptV1` status succeeded/failed 是 provider 结果；unverified/verified_canonical/reorged/suspended 是独立 verification disposition；provider proof 不签发 canonical world effect。新记录不改 current EffectReceipt.status String、legacy JSON hash、现有 Agent chat ledger 或 wasm-1 defaults。

附件 required top-level 为 `schema_version,scope,candidate,evidence_window,execution_attachment,state_sync_attachment,transitions,intent_results,consumer_samples,cases`。transition 绑定 sequence、from/to grade、trigger、last trusted/head、governing manifest/execution version、compatibility/head/finality verdict、blocker/affected operations/next step 与 refs；intent_results 绑定 intent/request/lineage/member generation、submitted grade、pending/decision、receipt/effect 0|1、actual refs、state/resource 前后 hash 和 revalidation；consumer 样本关联 transition 和可选 intent、实际等级、last trusted、blocker 与 next_step。每个列表至少一条，但列表非空不能证明场景覆盖。

Target hash 为 `H(domain,body)=SHA256(UTF8(domain)||0x00||CanonicalCBOR(body))`。domain 精确为 `oasis7/execution-result/v1`、`oasis7/module-execution-receipt/v1`、`oasis7/intent-request/v1`、`oasis7/intent-decision/v1`、`oasis7/external-effect-receipt/v1`、`oasis7/version-activation/v1`、`oasis7/module-state-migrated/v1`、`oasis7/world-execution-state-sync-attachment/v1`。literal schema_version 在 body 中；U64 JSON decimal 转 canonical unsigned integer，Hash32 转 bytes；明确的 complete-key set 排序去重，语义数组保留顺序。最短整数、definite length、canonical map order、唯一 keys；floats/非canonical bytes 拒绝。ContentRef 只 commit digest+byte_length，locator 是 metadata；block/proof/signature/self hash 留在不递归的独立 envelope。现有 receipt JSON 与 CBOR helper 不被假定同此新 domain 等价。

Pending request commitment core 排除自己的 request_hash、accepted_position 和 disposition；runtime-admitted member_generation 在 hash 前赋值且保留于核心，不用未来 journal/block identity 形成循环。IntentDecision 的 execution_receipt_ref/winner_receipt_ref 指向 canonical serialized **receipt_core bytes**：digest 是 SHA256(CanonicalCBOR(core))，byte_length 是实际 bytes 长度，另复算 domain-separated receipt_id=H(module-execution-receipt,core)。ContentRef.digest 不是 receipt_id，也不是包含未来 block 的 finalized wrapper hash；detached finalized wrapper单独读取并验证 exactcore的canonical inclusion。这是 runtime/WASM frozen target semantics，未改变当前 wire或此JSON字段形状。

<a id="sr2-dc5-semantic-validator"></a>
### Target semantic receiver（实现与运行证明仍未提供）

以下是本篇真实验证设计算法，不是声称存在的新工具。schema-only cannot pass DC-5：

1. Parse/version/unknown-field validation 后，验证所有 Id UTF-8 bytes、U64 最大值/转换、Git object kind、窗口 start<=end 与 last trusted/head 连续性。严格检测原始 JSON duplicate keys，schema 解析后的 map 无法检测被 parser 覆盖的 key；拒绝不合 canonical profile 的编码。
2. fetch 每个 ContentRef 原 bytes，复算 digest/length，保留只读原件；执行与 sync packet 的 scope/candidate/window 必须 exact equality，execution_version/protocol/test contract 对应实测版本。缺少任何 linked attachment、template/not_collected/partial 或读回不确定阻断。
3. 验证 state-sync genesis/manifest、checkpoint certificate、snapshot/bundle、replay 范围、state root、peer heads、blob closure、observer catch-up 与拓扑采样；blob closure 不代签自动追高。P2P authority 定义 finality/inclusion/branch/signers/epoch verification；签名/CI/provider/local node 结果本身不能签发正确 canonical identity。
4. 检查 transition sequence strictly increasing，from/to continuity、trigger/evidence、last trusted boundary 与真实当前 manifest、compatibility/head/finality verdict。history valid 仅能只读；serviceable 需当前 append/finality、version execution、compatibility 与 monotonic head 同时验证。proof conflict 进入 unavailable_isolated；HTTP/health green 不升级。
5. 覆盖必需状态路径、恢复只读/可服务/受阻各一例、gate rollback、manifest missing/conflicting 与 head regression/continuity missing 负例。任一场景缺失不得判 DC-5 通过；仅 enum 存在不算覆盖。
6. 逐 intent 验证 subject/world/request/target/lineage/member generation 与 exact decision/receipt/ref join，分别验证 core byte digest与domain-separated receipt_id及detached finality wrapper。以**实际执行/提交时**的 gate 判定新 execution receipts：不满足 serviceable 或当前 manifest/head/finality 证据时 count=0/effect=0，无 charge/state/effect publication；不能仅按 submitted_grade 推断，因为旧 pending 可在恢复后合法提交。count=1 需成功 receipt 与唯一可验证 inclusion，所有 identity/core/schedule/state/resources 一致；audit decision 不计 receipt。receipt_refs length=count，重复 receipt identity 不重复计费/效果。
   恢复可服务还须关联 [J1/J2 正向续跑结果](#sr2-journal-suffix-recovery)：固定有效条件下，合法新配方须完成一次，后续 disposition、模块唤醒时点和业务结算与不中断对照一致。全部 count=0、误拒绝或只比较恢复时 root 不满足该正向要求；既有 receipt/effect 上限与失败闸门继续独立校验。
7. 领域标记 exclusive set 的 lineage 在 canonical order 首个有效 receipt 唯一胜出，原子提交 state/resources/events/receipt 与全部 losing-member no-effect decisions；effective sum<=1。reject/expire/replan 只自身终止，独立请求继续。replacement/withdrawal 在 committed decision 前不覆盖原请求；generation 不能创建权限或排序优先。读回 crash/restart/reservation/after-publication artifacts 验证已确认效果不再运行。
8. 恢复 pending 在 first committed/finality-verified canonical block 选择 governing manifest；inclusive activation boundary 后按新权限/resources/preconditions/expiry 重审，不静默 payload translation、旧报价资格、lease 或优先级刷新。old receipt/state/event bytes 保留，replay 原 manifest/artifact/schedule；缺版本/activation/migration proof fail closed。
9. 验证 core hash/usage/checked charges、actual deterministic access count（支持但零收费可非零）、fuel_used<=limit、journal tuple monotonic、migration exact from/to/schema/artifact/namespace/output bound；`from_schema_version` 与 `to_schema_version` 各自必须是 exact declared `PositiveU64` 且不等（`from_schema_version != to_schema_version`）；numeric ordering 不授予迁移权；equal-version migration 必须在 authoritative mutation 前拒绝。迁移第二对象失败或 post-prepare 注入后无部分 registry/state/charge/event/artifact。clock timing/status reset 不改变任何 execution commitment。
10. 关联 actual consumer_samples 到 transition/intent，核对 grade、last trusted、affected operation、blocker、real next step 与 pending/rejected/expired/replan/committed 真实状态，原/替代各自状态可读；消费者 root/receipt 与 same-window authoritative evidence 一致。非serviceable 没有新成功结果，回退不改旧 confirmed receipt。缺消费者截图/API/provider parity 的适用 tier 保持 blocked，不由 transport 代签。

## 7. 状态、事务与持久化

Attachment lifecycle 是 collected→shape_valid→semantic_verified→tier_complete 或 blocked；这些是 evidence 状态，不是 world/workflow 状态。唯一执行成功在实际 canonical receipt 出现后成立；pending、delivery ACK、no-effect decision 不构成世界效果。目标 runtime transaction seam 见 [host publication](../../world-runtime/wasm/wasm-executor.design.md#sr2-host-publication) 与 [runtime DC-5 semantic receiver](../../world-runtime/wasm/wasm-executor.design.md#sr2-dc5-semantic-validator)，QA 不补 runtime 实现。

原 artifacts、snapshot、journal、旧 manifest/artifact/schedule/activation/migration bytes 按各 authority 保留；GC 不能丢历史 replay 所需材料。QA evidence 只追加归档，不原地修改原 receipt/proof/history；新 source/tree/schema version 产生独立新证据包。重试读取已持久 disposition，不能因 attachment 重建再次应用输出。Crash 覆盖 reservation 前后、prepared 未发表、durably published 三个 seam；receipt/loser/charge/state publication 必须一起读回成立或全无效果。

## 8. 部署、安全与运行约束

required fixtures 可在隔离环境检 schema/编码与 deterministic failure contracts；full combination 必须同候选多节点、state-sync/restore、actual formal consumer，并按真实 provider/headed 条款执行。dev/local world 不作为 global-authoritative 证明；world/branch错配拒绝。证据打包路径只读；不加载未验证 artifact、不执行 evidence 中的命令、不把 URL 当权限。

私钥/auth proof/raw module input/output 不进入默认 public status；证明可在受控 archive readback，以 digest 对外引用。未知 signer、epoch/root、branch或 proof scheme blocked；禁止 unsigned/local self-sign downgrade。按 scoped environment 固定节点/拓扑/资源与超时，读回失败保留 blocker；无法收集实际材料不得增加 mock success、手工复制正式 data 或替代 endpoint。

## 9. 质量与容量

| 场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| Attachment shape | Draft2020-12 validator、每 target record 正负 fixtures | required/enum/decimal/unknown/null 与 conditional shape 拒绝 | 本 schema 与 §6 exact aliases | schema self-check + schema fixture validation | 仅结构，不证明 proof/global joins |
| DC-5 恢复 | 同候选多节点，固定 canonical window，service与conflict路径 | 非service新 receipt0，合法恢复每intent至多1，真实 blocker/nextstep | DCS AC005/DC-5、根 SC-7 | §11 DC5-01..05 + existing state-sync lane | 实际 topology/runtime/consumer 未证明 |
| 原子/重放 | 确定输入、限额、failure seam、same tree snapshots | 失败零 charge/state/effect，已提交一次、原版本 replay root一致 | runtime/WASM publication与migration合同 | §11 RCP/LIN/MIG/MET/ACT 与 SR2-RT groups | located baseline非target完整证明 |
| SC-9 | 同baseline八cell，required API/provider与full headed/Agent | 数量/root/W/window/receipt语义守恒且一次性 | 根 SC-9 与 gameplay authority | §11八cell matrix、S6/S9/S10环境 | 模板/schema/plumbing不能替代 |

单次 short soak 不代表 endurance；具体 duration、规模、硬件/节点预算取对应 longrun/manual authority 并写入实际 packet。此设计不发明新性能数值。timing local bounded/top-N/reset/degraded 不改变 world state 或费用；状态样本不替代证明。

## 10. 兼容、迁移与回滚

目标验收附件是 additive、versioned，旧 envelope 与本篇原 evidence shape 保留；旧 tooling 不懂新附件时必须明确 unsupported/blocked，不静默 partial pass。它不成为 wasm-1 的强制新字段或重写 SDK defaults。target schema UNKNOWN、future versions、bad projection 在边界拒绝；旧 snapshot/golden module bytes 保持现有 decode/replay 行为。

回滚只停止消费新附件版本并恢复明确已知 verifier baseline；不能删除或重新解释 confirmed receipt/state、降级到不验证 proof 的 success，或因当前价格/manifest变化重算历史。authority 升级及 runtime schema migration 仍需受治理实现与独立验证；本文合入不自动激活。candidate/source/tree 变化重新收集证据，旧 pass不可移植。无历史 artifact/activation proof时恢复 blocked并保留已确认历史。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [AC-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) | [正向续跑判定](#sr2-journal-suffix-recovery) | 恢复后的处理、行动时点和业务结算与不中断对照一致，合法新配方完成一次；新请求和模块行动两项分别通过 | SR2-RT-03 的 J1/J2 子义务；扩展 [现有恢复测试](../../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs)，required deterministic fixture，固定 source/tree/world/manifest、合法 authority/权限/资源、C/T、真实非空 suffix 与相同后续输入；full 服务证明另验 | C snapshot、真实 suffix、两分支在 T 与续跑后的处理/时点/结算比较、两项各自结果 | planned；未提供执行结果；至多一次或零次结果不能代签正向完成，也不代签 full finality/consumer |
| [runtime 日志恢复合同](../../world-runtime/prd.md#runtime-journal-suffix-recovery) | [正向续跑判定](#sr2-journal-suffix-recovery) | J1 action 身份与 J2 成功 module tick 的非空 suffix 恢复分别成立，不重跑历史 WASM 或重复计费 | SR2-RT-03 的 J1/J2 子义务；扩展 [现有恢复测试](../../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs)，required deterministic fixture，固定 source/tree/world/manifest、finalized C<T、原始 WorldJournal suffix 与后续输入 | C snapshot、真实 suffix、不中断/恢复分支在 T 与续跑后的控制状态比较、J1/J2 各自结果、历史调用与计费计数 | planned；未提供执行结果；snapshot roundtrip、committed-input replay 或其中一项通过不能代签另一项 |
| [doc/product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005) | [doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-semantic-validator](game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-semantic-validator) | actual same-candidate linked附件与全部service/manifest/head/receipt/consumer边界；可服务结论同时具备正向续跑结果 | DC5-01..05、SR2-RT-03 的 J1/J2 子义务、[scripts/game-world-state-sync-commit-module-required.test.sh](../../../scripts/game-world-state-sync-commit-module-required.test.sh) 与 `scripts/game-world-state-sync-commit-module-required.sh`、`scripts/p2p-verify-state-sync-closure.sh`；full same source/base/tree/window topology+execution+consumer，不以模板替代 | archived attachment+raw proof bytes+readback+same-window consumer、J1/J2 各自续跑结果，实际 candidate 绑定 Issue evidence | schema-only、单节点、blob closure、零次结算无自动catch-up或可服务结论 |
| [doc/product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-003](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-003) | [doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix](game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix) | exclusive唯一有效winner、重试/restart不重复、独立intent不取消 | LIN-01..04/RCP-01..02；[crates/oasis7_runtime/src/tests/capability_effect_receipt_raw_publication_transaction_regressions.rs](../../../crates/oasis7_runtime/src/tests/capability_effect_receipt_raw_publication_transaction_regressions.rs) 与 `crates/oasis7_runtime/src/world/effect_publication_transaction_regressions.rs` 是 partial baseline；required fixture + full canonical crash/recovery same tree | request/lineage/decision/receipt/charge/state/journal前后artifact，失败注入位置与实际结果 | current chat ledger/locally signed receipts不证明通用全合同 |
| [doc/product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-004) | [doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios](game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios) | canonical activation选版、旧历史原schedule replay、schema迁移失败原子 | ACT-01/MIG-01..03/MET-01..02、SR2-RT-04..08；[crates/oasis7/tests/module_lifecycle.rs](../../../crates/oasis7/tests/module_lifecycle.rs) partial baseline；full同候选activation/replay+consumer | immutable旧新bytes、activation/inclusion、core/wrapper、state/charge/receipt roots和replan观察 | legacy replay测试不能代签targetmigration/finality |
| [SC-9](../../product/world-infrastructure/prd.md#sc-infra-009) | [doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-sc9-eight-cells](game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-sc9-eight-cells) | SC-9八cell、fixedbaseline、两类outage、required/full各自真实环境 | SC9-A/R-SF/TR/BA/TS；[testing-manual.md](../../../testing-manual.md) S6、`scripts/s10-five-node-game-soak.sh`、active-provider pureAPI+Agent parity，同候选固定baseline | 八组paired before/outage/recovery/retry/replay artifacts、desktop+narrow screenshot/console、provider identity fields | provider_local_mock/schema/模板不是activeLLM/realprovider/headed证明 |
| [REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | [doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-attachment](game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-attachment) | exact successful core/wrapper shape、canonical/hash/U64/UTF8、unknownfailclosed；专业约束仍由 [WASM execution contract](../../world-runtime/wasm/wasm-interface.md#sr2-execution-contract) 拥有；仅验证该产品子义务的证据形状，不以字段完整代签确定性执行 | Draft2020-12 schema self-check/positive/adversarialfixtures；[crates/oasis7_wasm_abi/tests/open_module_commands.rs](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs) current ABI baseline；required structural，full proof独立 | validator/version/input/schema SHA256/results，真实corebytes/proof另归档 | shape pass不验证内容hash、trust、运行或所有targetfees |
| [REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) | [目标附件](#sr2-dc5-attachment) | schedule/migration canonical identity 的版本及历史证据关联；不接管字段 authority | 沿本设计已有 ACT-01、MIG-01..03、MET-01..02 与 semantic 步骤8/9；[module_lifecycle.rs](../../../crates/oasis7/tests/module_lifecycle.rs) 仅为 located partial baseline；required 结构与语义负例分别记录，full 仍需同候选实际 activation、migration、历史 replay/proof 与消费者 | 原 manifest/artifact/schedule 与迁移前后 bytes、activation/inclusion、core/wrapper、state/charge/receipt roots；源/树/world/window 与原始 artifact 回读一致 | 未执行；结构通过不能代替语义或 full 证明，已有原子性、equal-version 拒绝及 J1/J2 正向续跑要求不减 |

<a id="sr2-qa-negative-matrix"></a>
### 17 个 target negative cases

下列 IDs 是稳定验证设计 cases，不是假造任务 IDs；输入、断言、tier 与 receiver 全部明确。Current locators 只作为可扩展的 partial baseline，未声称这些 target cases 已接入或执行。结构 fixtures 在 required 验 schema；涉及 canonical proof/atomic/recovery 的行为断言需实现 required fixtures 与 full同候选 actual运行。

| Case | 输入/注入 | 必须断言及 exact receiver | Layer/environment |
| --- | --- | --- | --- |
| DC5-01 | 单一 execution/sync ref 换candidate/world/branch/source/tree/window | §6 semantic步骤1–2 invalid join，DC-5 blocked，无新效果 | required identity-negative；full same candidate readback |
| DC5-02 | 缺 attachment/proof/transition/consumer，template/not_collected | schema required或semantic步骤2/5/10 blocked，不推断pass | required shape；full archive/consumer |
| DC5-03 | head regression、manifest conflict/missing、compatibility mismatch、orphan proof | 步骤3–6拒绝service，新增receipt/effect0，真实blocker | required deterministic gate；full topology |
| DC5-04 | history完整但append/version证明缺失 | 步骤4只读，不产生新receipt1 | required recovery gate；full restore |
| DC5-05 | reconnect green后pending等待期间gate失效 | 步骤6/8 current重审，无charge/effect/expiry或priority刷新 | required outage fixture；full消费者 |
| RCP-01 | receipt request/target/block/hash错配或proof不可达 | 步骤2/3/6 unverified/blocked，zero counted effects | required proof-negative；full provider/inclusion |
| RCP-02 | local signer/provider answer/CI artifact伪装canonical proof | 步骤3拒绝trust，不造finality或success | required trust-negative；full externalproof |
| LIN-01 | 原请求与linked replacement激活前后两序竞态/sameblock | 步骤7/8有效winner1，losers0，历史winner不变 | required orderfixtures；full canonicalrun |
| LIN-02 | duplicate withdrawal/replacement同hash再换hash | 步骤6/7同结果idempotent、改hash拒绝零mutation | required durable replay；full crossentry |
| LIN-03 | predecessor dangling/cycle/world-subject错配/generation回退 | 步骤7在commit前拒绝，独立intent不取消 | required lineagefixture；full API/Agent |
| LIN-04 | reservation/prepare/durablepublish各seam crash | 步骤7读回一个winner或零效果，不rerun committed WASM | required failureinjection；full restart/snapshot |
| MIG-01 | 原有缺migration declaration/bad fromto/schema/artifact/manifest/state hash；新增 shape-valid equal-version 输入：`from_schema_version=to_schema_version="1"`（其余 migration identity/`PositiveU64` 字段合法） | 原有负例继续按步骤8/9拒绝；新增输入通过 schema shape，但 semantic 步骤9须在 authoritative mutation 前拒绝；旧version/state/bytes保持，无charge/event部分发表 | required migrationnegative；full历史replay |
| MIG-02 | oversize/noncanonical/namespace越权/伪造event context | 步骤9全preparedbatch中止，包括第二对象与postprepare | required bounds/seam；full migrationrun |
| MIG-03 | 原event/state bytes跨migration后restart | 步骤8/9exact root/receipt/charges，旧bytes不重写 | required golden；full snapshot-tail |
| MET-01 | 历史commit后更换schedule/current price | 步骤8/9原cost不变，新execution才可用新admitted schedule | required deterministic replay；full activation |
| MET-02 | count/charge overflow、fuel/balance不足、unordered set | 步骤1/9commit前拒绝零charge/state/effect | required arithmetic/bounds；full atomicreadback |
| ACT-01 | candidate/client/submission time与final committed activation不一致 | 步骤8按canonical block选版，缺/冲突proof无效果 | required boundaryfixture；full finalizedhistory |

<a id="sr2-runtime-scenarios"></a>
### 10 个 runtime target scenario groups

| Group | 输入组合 | 判定与 QA cases | 证据/环境与 actual receiver |
| --- | --- | --- | --- |
| SR2-RT-01 | finality outage在submit/recovery前，expiry已过 | pending无charge/output/receipt，重连不续期；DC5-04/05 | §6步骤4/8，required outage/full恢复 |
| SR2-RT-02 | original/replacement/withdrawal双序、sameblock、duplicate、restart、independent | effectivewinner1且原子losers0，拒绝仅self；LIN-01..04 | §6步骤7，required order/crash/fullcanonical |
| SR2-RT-03 | recovery时permission/resource/quote已变；另在有效条件固定时，由 C<T 的 finalized checkpoint 加非空事件 suffix 恢复并续跑 | 变化条件仍 current-condition reject/replan零partial；DC5-05。固定有效条件另分别判定 [J1/J2](#sr2-journal-suffix-recovery)，同后续输入与不中断对照一致，合法新配方完成一次 | §6步骤6/8 与正向续跑判定，required precondition/continuation；full consumer/service 仍独立 |
| SR2-RT-04 | activation前/边界/后、oldsubmit-newfinalize、linkedcrossversion | canonical version选版，一次效果；ACT-01/LIN-01 | §6步骤8，requiredboundary/fullactivation |
| SR2-RT-05 | activation proof conflict/missing或旧artifact缺失 | restorefailclosed，原历史/schedule不改；ACT-01/MET-01 | §6步骤8，requiredmissing/fullhistory |
| SR2-RT-06 | external wrongworld/request/lineage/provider/proof/root/branch，duplicate | 拒绝或原decision，无localsignfallback；RCP-01/02/LIN-02 | §6步骤2/3/6，requiredtrust/fullprovider |
| SR2-RT-07 | migration第二对象失败/outputlimit/hash与postpreparefault | 全registry/state/artifact/schedule/charge/event保留；MIG-01..03 | §6步骤9，requiredseam/fullsnapshot |
| SR2-RT-08 | oldmanifest/SDKgoldenbytes/newunknownversion | oldbyte/semantic兼容，新unknownclosed；MIG-03/ACT-01 | §6步骤1/8，requiredABI/fullreplay |
| SR2-RT-09 | DC5attachmentswap/mixedcandidate/missingnegative/consumer | blocked，field/coverage不得缺失；DC5-01..05 | §6全步骤，requiredshape/fulljoinedpacket |
| SR2-RT-10 | metricsclock/reset/degraded而canonical inputs不变 | receipt/hash/root/fee相同，metrics不进入commit；MET-01/02 | §6步骤9，requiredvariation/fullcomparison |

<a id="sr2-journal-suffix-recovery"></a>
### SR2-RT-03 的正向日志恢复与续跑

本节承接 [AC-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) 的两项独立结果，以及 [AC-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005) 的恢复可服务正向证明；执行语义由 [runtime 恢复设计](../../world-runtime/design.md#runtime-journal-suffix-recovery-design) 与 [runtime 验证设计](../../world-runtime/design.md#runtime-journal-suffix-validation) 拥有。J1/J2 是现有 SR2-RT-03 内的独立子义务，不替代原条件变化负例，不改变 17 个 negative cases、10 个 runtime groups 或 8 个 SC-9 cells。

固定同一 source/tested tree、world、合法 authority、manifest/artifact、权限、资源、checkpoint C、目标 T 和有序输入。C 必须是已完成 tick 的合法 checkpoint 且 C<T；恢复输入必须包含实际归档的、完整非空的 canonical `WorldJournal` 后缀。由同一 C 运行出不中断 control 至 T，再只用 C 与该 suffix 恢复至 T，分别比较 T 边界的执行状态和影响后续运行的状态，随后向两条分支提供相同的有效 action/tick 输入。目标 T 的完整 snapshot 只能作对照，不能成为被测恢复输入；重新执行 ordered committed inputs 是另一条验证路径，不能作为事件日志恢复的替代。

| 子义务 | 必须包含的实际历史 | T 边界及后续正向判定 |
| --- | --- | --- |
| J1 | C 之后有 action 分配，且至少一个 recipe job 至 T 已完成结算 | allocator/era 与不中断 control 一致；恢复后相同的新合法 recipe 取得不与历史 settled job 冲突的身份，接受/完成结果与对照一致并正常结算一次，原 job、扣费和产物不被覆盖或重复结算 |
| J2 | C 之后至 T 有真正成功的 WASM module tick，分别覆盖 due 请求消费、`WakeAfterTicks`、`Suspend` 与缺省指令 | 恢复后的 schedule/control 与不中断分支一致；相同后续 tick 的触发时点、暂停状态、结果及业务结算一致，不提前/漏掉调用。恢复期间不重新调用历史 WASM，不重复计费；恢复后的新调用仍按原执行合同计费 |

J1/J2 各自记录实际断言与 verdict，任一未运行或失败不能由另一项通过覆盖。证据保留 C snapshot、原始 suffix、C/T 边界及目标对照、后续输入、结果/资源/receipt 比较和 replay 期间历史 WASM 调用与费用计数；字段与 oracle 以 runtime 验证合同为准。完整 snapshot 往返、事件后缀 replay 和 committed-input 重执行分别标明实际路径，不互相代签。只比较 T 的 root、只证明 receipt≤1，或以人工补写 scheduler/allocator 后的状态作恢复输入，均不满足本节。

本节是 `planned` 的 required 确定性 runtime 验证，不新增 provider 或模型试玩要求。局部 J1/J2 结果不替代既有 full 服务闸门、提交证明、拓扑及适用消费者证据；文档采纳、结构检查或局部实现也不代表运行通过或发行就绪。

Located baselines（未据此宣称执行）：`crates/oasis7_runtime/src/world/effect_publication_transaction_regressions.rs` 的 raw_pending_receipt_post_prepare_failure_preserves_publication_state/raw_inflight_receipt_post_prepare_failure_preserves_publication_state/unknown_receipt_error_precedes_post_prepare_fault；`module_output_publication_transaction_regressions.rs` 的 module_state_append_inserts_replaces_and_replays_to_same_root/charge_append_retry_publishes_once_and_replays_to_same_root；`governed_module_lifecycle_transaction_regressions.rs` 的 governed_upgrade_tail_failure_preserves_only_approved_prelude/governed_success_keeps_event_order_roots_replay_and_rollback_schedule；`module_instance_publication_transaction_regressions.rs` 的 upgrade_post_prepare_failure_publishes_no_instance_fee_or_event/instance_install_upgrade_success_preserves_published_and_replayed_roots；`crates/oasis7/tests/module_lifecycle.rs` 的 shadow_failure_blocks_apply/replay_preserves_module_events。Executor/ABI/SDK安全和release-controls partial locators仍按各专业design保留；模板test只检模板，不证明DC5。

<a id="sr2-sc9-eight-cells"></a>
### SC-9 八个独立完整 proof cells

每 cell 在同一 baseline 固定 `world_id/root/revision/child/stage/edge/batch`、requested/committed/executed/held/consumed/unmet/residual quantities、canonical state bucket、window identity/lease lineage、receipts、W、progression_effect、next_action、next_recheck，保留 before/outage/fresh-snapshot/recovery/retry/replay 的实测材料。不得只固定名称不固定实际值。A=权威 finality/append/execution/industrial-service；R=非权威 Viewer/API/hosting/read surface。hosting若是权威依赖归A。

| Cell | Boundary + outage | 独立断言 | 必需证据 |
| --- | --- | --- | --- |
| SC9-A-SF | stage_finish + authority | 未finality无root/hold/sink/WIP/credit/output/eligibility/W；postroot保留真实投入/产出，不第二child | 固定baseline、freshsnapshot、一次canonical disposition、receipt/idempotency、W authority |
| SC9-R-SF | stage_finish + read surface | outage仅stale/unknown，world/root/quantities/window/W不改，reconcile实际stage finish | authoritative journal+Agent/Viewer/pureAPI同义readback，不补发reward |
| SC9-A-TR | transit + authority | 保留在途/到达数量、root/lease/receipt，恢复最多一次continuation/hold/defer/reject/expire/compensation | freshsnapshot路径/容量/owner/power/expiry重审、deliverycredit至多一次 |
| SC9-R-TR | transit + read surface | 读面不可达不制造arrival/sink/credit，真实authority完成则只补读 | 原/新authoritative记录、三类消费者reconcile，无重复credit |
| SC9-A-BA | buffer_admission + authority | committed/held/consumed/unmet/residual有界守恒，拒绝不丢/瞬移/复制holdrelease | freshcapacity/path/recipe/permission重审、一次release/一次effect |
| SC9-R-BA | buffer_admission + read surface | 观察stale/unknown不重设容量/hold/window/W，不自动改道 | sameworldsnapshot/journal+实际buffer disposition，同义消费者 |
| SC9-A-TS | terminal_settlement + authority | 未final无sink/reward，已有root/receipt保留，终结至多一次且不续lease/priority | freshterminal/power/batch/expiry重审、settlement receipt/charge/quantity |
| SC9-R-TS | terminal_settlement + read surface | 无world/W/receipt/progression mutation；已真实settled只reconcile，无第二sink/reward | authoritative settlement+三类readback，retry/replay zero duplicate |

所有A cell只有 gameplay authority 的 canonical interruption disposition 可将**当前未完成 candidate** W reset一次；历史里程碑/receipts不变，产出因果变化才linked revision/childroot。R cell永不重置W、回滚世界、续租或补发。一个有序输入/freshsnapshot revalidation/canonical disposition/effect各一次，retry/reconnect/restore/replay/duplicatearrival-submit不能复制sink/credit/receipt/holdrelease/W/progression/reward。

`test_tier_required`：八cell deterministic protocol matrix **和** active-LLM/provider-backed pure API 与同一权威结果 parity。`test_tier_full`：在required之上，真实本地栈/真实provider external headed S6/Playwright **desktop+narrow** screenshots/console，以及 provider-backed Agent parity；记录 `agent_decision_source`、`agent_provider_backend`、`agent_provider_contract`、`agent_provider_transport`。`provider_local_mock` 只做plumbing/fixture预检，不能替代activeLLM/realprovider/headed/Agent parity。具体环境入口由 `testing-manual.md` S6及现有S9/S10 lane承接；任何cell、环境或专业owner证据缺失仍未通过，不从genericWASM或schema结果推导SC-9关闭。

## 12. 决策、长期风险与未决问题

选择一个 QA-owned JSON schema 而非多份 competing runtime/product schema，原因是它约束 evidence shape 并能真实验证正负结构；runtime/WASM字段与hash语义仍在其专业authority，GWSC提供组合验证算法。代价是 JSON Schema不能证明trust/编码bytes/跨artifact或运行不变量；必须保留明确semantic步骤与full证据，不能因工具schema passed减少matrix。

选择成功execution receipt与no-effectdecision分离，避免DC5的receipt0被committed rejection误计；选择Gitcommit/tree与Hash32分型、U64decimal与UTF8byte guard，防precision/身份碰撞；选择core+detachedwrapper避免循环hash。变更这些字段/domain、artifactretention、proofscheme或消费者version时需对应owner重新审读和新候选证据。

实现/证明缺口由runtime/WASM/P2P各自提供可验证代码与artifact时解除；consumer样本由Agent/Viewer提供真实surface时解除；QA仅在同候选场景完整且proofreadback和适用tier成立后解除组合阻断。原文§7的proxy/endurance/projection/blobclosure/manualcopy风险仍有效；文档采纳、合并或历史run都不自动解除任何实际能力/发行边界。
