# oasis7 Runtime：WASM 执行器接入（设计分册）设计

- 对应需求文档: `doc/world-runtime/wasm/wasm-executor.prd.md`
- 稳定证据入口: `doc/world-runtime/wasm/evidence.md`

## 1. 设计定位

任务拆解、当前阶段和发布候选状态由 GitHub task issue / Project 承接；本设计
只定义稳定的执行器契约。可复用的实现和回归入口见
[`evidence.md`](evidence.md)。
定义 WASM 执行器接入设计，统一执行入口、宿主交互、资源限制与错误回传。

## 2. 设计结构
- 执行入口层：定义模块加载、实例化与调用入口。
- 宿主交互层：明确 host function、上下文注入与结果回传。
- 资源约束层：对 fuel、内存和中断语义实施限制。
- 错误观测层：把 trap、超时和权限失败映射为稳定错误。
- ABI/capability 层：以可选兼容字段承载 schema、cap slot、policy hook 与 ModuleContext 元信息。
- 工件完整性层：校验存储 hash 与 compiled-cache wrapper/compatibility domain。

## 3. 关键接口 / 入口
- WASM 执行入口
- host function 接口
- 资源限制配置
- 执行错误映射
- `ModuleManifest.abi_contract`、`ModuleEffectIntent.cap_slot` 与 pure policy hooks
- serialized compiled artifact cache 与持久化工件 hash 校验

## 4. 约束与边界
- 执行器必须可观测、可中断。
- 资源限制优先保证宿主安全。
- 不在本专题扩展新的字节码格式。
- 新 ABI/schema 字段保持 optional/default compatible，不把 agent-os 参考实现升级为 Oasis ABI 替代品。
- pure policy hook 只判定，不产生递归副作用；cap slot 未声明或冲突时 fail closed。
- `max_gas=0` 回退到配置 fuel；epoch 与 memory limiter 保证可抢占和有界资源。
- compiled cache 按 engine/OS/arch 隔离，损坏缓存降级为 miss；storage lifecycle 仍由 root world-runtime authority 拥有。

### Wasmtime Cargo 功能选择与执行兼容

执行器使用 Core WASM、Cranelift 和同步 `Module/Store/Linker/TypedFunc` 调用。
Wasmtime 的 Cargo 功能通过[执行器 manifest](../../../crates/oasis7_wasm_executor/Cargo.toml)
显式选择，减少未使用的宿主能力及其 Rust 构建依赖；实际 WASM proposal、fuel、
内存、NaN 和正式回放配置继续由执行器的 `Config` 构造统一设置。

| 功能选择 | 设计依据与边界 |
| --- | --- |
| 保留 `std/runtime/cranelift`、`parallel-compilation`、`wat` | 支持新模块编译、同步执行、并行编译 guest 函数及现有 `Module::new` 文本输入能力。并行编译影响运行时首次加载延迟，不能仅按 Rust 构建时间裁剪。 |
| 保留 `gc` 及三个 collector、`threads` | Wasmtime 49 的 `wasm_reference_types`、`wasm_exceptions` 配置 API 依赖 `gc`，`wasm_threads` API 依赖 `threads`。`wasm_gc(false)` 不代表可删除 Cargo `gc`；保留 collector 集合及选择顺序，避免改变现有 reference/function-reference 支持与资源行为。 |
| 保留 `anyhow/backtrace/demangle/addr2line` | 保留错误转换和诊断信息；结构化失败及其文本可能进入 runtime 事件，不能把诊断变化一概视为无共识影响。 |
| 不启用 component/WIT、`async/debug`、`stack-switching`、`compile-time-builtins` | 当前执行路径没有组件、异步宿主调用、guest 调试或自定义编译期 builtin；保留同步 epoch trap。调整时须检查上游功能依赖，避免其他功能重新启用整个功能族。 |
| 不启用内置 `cache`、`profiling`、`pooling-allocator`、`coredump/debug-builtins` | 当前使用自建 serialized artifact cache 和默认 on-demand 分配器，没有配置对应宿主能力。内置 `cache` 与 `Module::serialize/deserialize` 是独立功能。 |

功能裁剪沿用现有[确定性执行要求](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)，
不以 Wasmtime 版本号相同推断语义自动相同。修改功能集合时须复核上游
`Config` 的条件默认值，在完整功能与候选功能下运行同一组 guest 回归，比较
三种同步 ABI 的 `ModuleOutput`、reference/call-ref、NaN 位模式、正式回放
relaxed SIMD、fuel、epoch 和失败映射；世界事件/hash 仍由 runtime 回归验证。
[执行器功能回归](../../../crates/oasis7_wasm_executor/src/tests/engine_contract.rs)
验证可观察结果与新执行器的磁盘命中，既有资源限制和损坏恢复回归继续适用。

Cargo 功能变化可能改变 engine compatibility key；缓存继续以
`precompile_compatibility_hash` 隔离并校验 wrapper/version/checksum，允许旧缓存
正常 miss 后重编译。缓存 key 或原生编译工件字节一致不作为确定性执行的判据，
也不得为维持命中而绕过兼容检查。

## 5. 设计演进计划
- 先接执行入口。
- 再补宿主交互与资源约束。
- 最后沉淀错误观测与回归。
- 修改 capability、sandbox limits 或 cache 格式时，同步更新 ABI 兼容与损坏恢复测试。

<a id="wasmtime-49-coordinated-release"></a>
### Wasmtime 49 执行兼容与发布边界

Wasmtime 49 将 bulk operation 的 fuel debit 移到操作成功完成后；同时触发越界与
fuel 不足的调用，48 与 49 可能分别返回 `OutOfFuel` 与 `Trap`。两者都拒绝调用且不
发布 guest state/effects/output，但 runtime 将结构化失败写入 `ModuleCallFailed`
或 `ActionRejected`，其内容进入 tick `events_hash` 和 block hash。因此相同 state root
不能证明两个引擎可共同执行/验证同一候选。当前 tick `executor_version` 是 crate
package version，不标识或强制校验 Wasmtime 依赖版本。

引擎依赖升级必须采用同一 world 的完整执行集合协调切换：所有能够 author 或独立
execute/validate tick 的进程须在维护边界全部停止，在完整目标 lockfile 精确版本工件回读通过后
才恢复；不得使用重叠 48/49 执行窗口。具体 inventory、quiescence、readback、恢复与
回滚 hold 见 [Wasmtime49 维护流程](../../p2p/blockchain/wasmtime-49-coordinated-maintenance.runbook.md#wasmtime-49-maintenance)。
现有单节点升级、health/preflight 或 checkpoint 一致性检查均不构成自动 fleet engine
admission fence；无法控制完整执行集合时阻断部署。此说明不增加协议字段或错误归一化。

历史 journal recovery 消费已记录的失败事件，不重跑 guest WASM；不得用 49 的新错误
分类重写旧事件、confirmed receipt 或历史 hash。PR 合入只交付依赖与发布约束，不证明
实际 fleet homogeneity，也不授权部署。实际维护仍需独立授权和新鲜运行证据。
本次分析及证据边界见 [runtime disposition](https://github.com/eng-cc/oasis7/issues/4269#issuecomment-5972995460)
与 [协调发布设计](https://github.com/eng-cc/oasis7/issues/4269#issuecomment-5973104108)。

## SR2 目标执行与验收合同

Owner runtime_engineer；source baseline `9c41d57b4436f71f0cf9b481043d882cfdc551ed`；2026-09-26 作者审读，独立 review pending。上文 current obligations 保留；本节目标/unimplemented，不修改 wasm-1/current fees/cache、也不宣称 target fixture 已运行。

## 1. 问题、目标与非目标
当前 sandbox result、prepared publication 和 local receipt 不足以证明 finalized world effect。目标将 validation、bounded execution、deterministic usage、prepare 和 canonical publication 交给具体 receiver；不更改经济率、不引入新字节码、不从 compiled-cache 或可编译推断 release。

## 2. 上游约束与相关角色
WASM owns wire/codec/schedule；runtime owns host publication；P2P owns finality/head；QA owns actual attachment 和合并 verdict；Agent/Viewer owns实际 consumer projection。所有 source refs 是 canonical `eng-cc/oasis7` path#fragment，不隐式引用另一个仓库的同名条款。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [req-dcs-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) | actual same-candidate semantic attachment | [sr2-dc5-semantic-validator](wasm-executor.design.md#sr2-dc5-semantic-validator) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | schema pass 无真实 proof |
| [req-dwe-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | successful receipt bound to charge/state and no partial output；专业约束 [execution contract](wasm-interface.md#sr2-execution-contract) | [sr2-host-publication](wasm-executor.design.md#sr2-host-publication) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | located tests 未运行且不覆盖全目标 |
| [req-dwe-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) | old schedule recorded, local clocks excluded；专业约束 [metering contract](wasm-interface.md#sr2-metering-contract) | [sr2-host-publication](wasm-executor.design.md#sr2-host-publication) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | process metrics 不是 authority |
| [req-dwe-003](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-003) | pending replacement and version authority | [sr2-host-publication](wasm-executor.design.md#sr2-host-publication) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | current prepared seam 非完整 durable lineage |

## 3. 当前状态、目标状态与差距
Current：Wasmtime fuel/memory/watchdog、hash-before-cache、kind/state/CBOR、pure-state rejection 与 optional capability/context contracts 仍由 PRD/interface/evidence 约束。Current governed proposal seam 仅限定入口；ModuleExecutionReceiptV1、schedule identity、migration/DC5 joined evidence 是 target。Located tests 和历史 evidence 不证明当前候选通过，更不证明完整 crash-safe external outbox。

## 4. 边界与结构
Host 承认 immutable world baseline、active manifest、artifact、schedule 与 caller authority；sandbox 只产出 bounded bytes/effects/emits/usage。Receipt core 由 host 构建，sandbox 不能生成 canonical finality。Publication transaction 包含 state/resources/events/decision/receipt；canonical certificate 由 P2P 外部验证。Metering local clock/metrics、cache hit 与 process timestamp 不进入 consensus。

## 5. 关键运行流程
<a id="sr2-host-publication"></a>
### 请求、执行、usage、prepare、publish
1. decode/version/UTF8 byte/U64/resource bounds；拒绝 malformed、unknown enum/fields/version，保留原 opaque payload bytes。
2. 验证 world/branch/request/module/version/instance/interface/wasm、manifest、activation、descriptor 与 schedule exact identity；读取并 rehash artifact，hash before compiled cache；unknown schedule/schema 拒绝。
3. 验证 canonical bytes/digests 和 detached proof/trust root/epoch/inclusion；proof 不可达只 blocked，不 unsigned downgrade。
4. 当前 gate 必须 verified_serviceable 且 compatibility/head-continuity/finality-append 全部 verified；对所有受影响 NEW intents 否则 count=0/effects=0、state/resources 不变，保留历史 receipt。无 finality 时 local audit 不是 finalized decision。
5. 根据 final committed canonical block/activation 选择版本；重新检查 live permission、resources、expiry、target 和 explicit lineage，旧 acceptance/quote 无执行优先权。
6. bounded deterministic execution 后 validate output/effects/emits/caps、fuel_used<=fuel_limit、input/output bounds、journal lexicographic era/event_id 与 checked amounts；unsupported/not-used access 为0，supported but uncharged actual access 可非0。
7. 构建 [R5 execution/receipt cores](wasm-interface.md#sr2-execution-contract)，先核验 execution_id，再 hash receipt_core；staged charge/state/effect/emit/decision/journal/cache/schedule mutation 全部基于同一 immutable base，winner 与 exclusive loser 同 transaction。
8. validate all prepare steps、durability/journal/backpressure 与 canonical append。单一不可失败 publication seam 安装；durable commit 后输出 original receipt，detached inclusion 同时绑定 execution_id/receipt_id/version/activation。同 wrapper 错 block 不可接受。
9. Retry same immutable key/hash 返回 pending/decision/receipt；changed body 拒绝。Crash before durable seam no effect 或完整 committed transaction；after durable seam 不重跑 WASM、重扣费或发送新 effect identity。

Failure matrix：decode/identity/trust/gate/precondition/execution/output/usage/prepare 任一失败均不发布新的 state/effects/charges/success receipt；only verified canonical decision 可以结束 pending。任何 post-prepare injected failure 保持 registry/instance/artifact/schedule/resources/event era/journal/receipt 原基线。Publish 是已准备的不可失败安装，不能在半发布后调用 fallible validator；持久恢复须通过 transaction commit marker 区分完整与未提交。

49.0.4 是修复 [组件流类型 panic](https://github.com/bytecodealliance/wasmtime/security/advisories/GHSA-9pvw-hwfg-92h5) 与 [aarch64 大型 `br_table` 错误跳转](https://github.com/bytecodealliance/wasmtime/security/advisories/GHSA-mghf-wjfp-p5xm) 的补丁基线。当前 Core WASM 路径未启用 component，但 Cranelift 修复适用于执行器；补丁版本也不能据此推断混合执行确定性。49.0.3 与 49.0.4 的切换继续遵守完整执行集合停机、精确版本回读和禁止旧目标混合加入的维护边界。依赖更新不授予部署或正式世界激活；执行器 feature、fuel/epoch/NaN 配置与缓存兼容校验保持原合同。

## 6. 接口与数据合同
唯一字段 authority：[types](wasm-interface.md#sr2-types)、[execution cores/wrappers](wasm-interface.md#sr2-execution-contract)、[metering](wasm-interface.md#sr2-metering-contract)、[migration](wasm-interface.md#sr2-migration-contract)；runtime pending/activation/external authority：[lifecycle](../module/module-lifecycle.md#sr2-pending-contract)。
`H(domain,body)=SHA256(UTF8(domain)||0x00||CanonicalCBOR(body))`，execution/result、receipt、intent/request/decision、external effect、activation、migrated、attachment 各使用明确 versioned domain。Canonical CBOR shortest unsigned integer、definite length、canonical encoded-key map order、unique keys/no floats，semantic arrays 保序，sets 按 complete key 排序去重；ContentRef commits digest+byte_length，locator 不属于 authority。Id UTF8 1..256 BYTES 无归一化；所有 U64 lossless canonical decimal JSON string、checked max18446744073709551615；counts0|1 为显式 JSON integer exception。Local timestamp/self-id/signatures/递归 containing-block proofs 不进入 core。
ExecutionIdentity wrapper scope 只继承 execution_core；ModuleExecutionReceipt wrapper 只继承 receipt_core.execution_core，无 duplicate scope。仅 committed_success 是 execution receipt；no-effect rejected/expired/replan/terminated 属 IntentDecision，不允许 committed_rejection 偷渡。

## 7. 状态、事务与持久化
accepted pending != committed effect != durable receipt != published consumer。Immutable pending request 与 terminal durable index 分离；activation、schedule 和历史 payload 不因最新版本而重写。Snapshot+tail 包含原 request/lineage/winner、journal era、state/resource hashes 与 receipt identities；restart 返回 original terminal record。历史 reexecution 只 audit compare，不替代 recorded result/charge；旧价格重算禁止。

## 8. 部署、安全与运行约束
Production external proof、trust root、branch 与 authority snapshot 必须 actual readback。Current local effect signer、prototype finality、provider response、CI 只能证明各自层。Unknown scheme/key/manifest、missing proof bytes、不兼容 runtime 均 fail closed。本地编译遵循[开发流程规范](../../engineering/workflow/source-of-truth.md)，默认使用各 worktree 的 target；本次没有实际 runtime/provider/browser执行。

## 9. 质量与容量
Required tier 为 candidate-bound deterministic fault fixtures：每个 step 注入 failure、检查 full bytes/state/resource/journal/charge/receipt 相等；same inputs/core 更换合法 detached inclusion wrapper 不能改变 core IDs。Full 增加真实 canonical proof、多节点恢复和 consumer evidence。预算沿用 PRD fuel/memory/output 与 current compute/electricity ceil-KiB formula，clock/reset/cache variation 不改变费率/ID；不存在本次 benchmark pass。

## 10. 兼容、迁移与回滚
旧 wasm-1 optional/default、CBOR fixtures、compiled cache version/checksum/domain 和损坏 miss 保留。新目标 schema/version 使用独立 closed records；unknown 新版本拒绝，不要求旧 ABI 突然携带新字段。Activation 与 migration [lifecycle sequencing](../module/module-lifecycle.md#sr2-migration-publication) 明确旧字节/旧 schedule retained；回滚经治理新记录，不撤销历史 committed effects。

## 11. 验证设计与可追溯性
<a id="sr2-dc5-semantic-validator"></a>
### DC-5 target attachment semantic receiver
Physical closed JSON receiver 是 [actual target schema](../../testing/schemas/world-execution-state-sync-attachment.schema.json)，attachment 字段 authority 和 actual scenario receiver 为 [GWSC](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-attachment)。以下算法是目标 semantic validator，未实现；JSON Schema 或 fixture pass 不能代替它。
1. Validate shape/version，再 semantic UTF8 bytes、lossless U64 maximum、GitOid40 commit/commit/tree readback 与 Hash32 区分；diagnostic code 分别 utf8_id_byte_bound、u64_out_of_range、u64_noncanonical_decimal、git_object_type_mismatch、hash_domain_mismatch。
2. Resolve execution_attachment/state_sync_attachment 和所有 evidence/proof refs；fetch bytes、check byte_length+digest+proof body，locator 变更但相同 bytes 不改变 identity；missing/unreadable/wrong proof bytes blocked。
3. Join exact scope/candidate（source_oid/integration_base_oid/tested_tree_oid/execution_version/protocol_version/test_contract_version）与 canonical window start/end heights+hashes；禁止混候选、嵌套 scope shadow 或另一个 window。
4. Require complete cases、transition strictly increasing sequences、真实 last_trusted/observed head、manifest/version、compatibility/head/finality verdict、affected operations/blocker/meaningful next_step evidence。verified_serviceable→stale_catching_up→verified_read_only→verified_serviceable 与 conflict→unavailable_isolated 皆须覆盖。Chain recovery alone 仅 read-only；health/reconnect 不升级 authority。
5. 每 row 新 receipt refs length=count0|1；count1 需 exact successful canonical receipt/core/decision/proof；nonserviceable、invalid governing manifest/head/finality 为 count0/effects0、before==after state/resources。不得混入 historical receipts 充数；historical receipt bytes 不变。
6. 校验 admitted explicit exclusive set lineage sum effective<=1；independent 不取消；loser/reject/expire 无 mutation；pending 恢复以新 live conditions revalidate，outage 不延长 expiry。Validate generation/predecessor 与 canonical first-effective ordering。
7. Consumer sample joins transition_sequence/intent/version、visible grade/disposition、last trusted boundary、operation/blocker/next_step；证据必须实际可读并对应 original/replacement，不接受 template/not_collected/author-set pass。
8. Compute combined verdict only after all joins/proof/scenarios succeed；任何 missing/conflicting/incompatible 或 coverage gap 使 blocked，保留精确诊断与 evidence，绝不从本地 metrics 或 schema valid 推断 DC-5 full pass。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [req-dcs-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) | [sr2-dc5-semantic-validator](wasm-executor.design.md#sr2-dc5-semantic-validator) | actual same-candidate semantic attachment | DC5-01..05, SR2-RT-01/09; required swap candidate/world/window, missing proof/consumer, regressing head, absent append, waiting intent proof revoked; full actual execution+state-sync+consumer joins；[located partial source](../../../scripts/game-world-state-sync-commit-module-required.test.sh) | GWSC #sr2-qa-negative-matrix / Task candidate artifact | schema pass 无真实 proof |
| [req-dwe-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | [sr2-host-publication](wasm-executor.design.md#sr2-host-publication) | successful receipt bound to charge/state and no partial output；专业约束 [execution contract](wasm-interface.md#sr2-execution-contract) | MET-02, SR2-RT-07/08; overflow/fuel/balance/collection and each prepare failure; located crates/oasis7_wasm_executor/src/tests.rs + module_output_publication_transaction_regressions.rs；[located partial source](../../../crates/oasis7/tests/module_lifecycle.rs) | required before/after roots/journal/receipt comparison | located tests 未运行且不覆盖全目标 |
| [req-dwe-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) | [sr2-host-publication](wasm-executor.design.md#sr2-host-publication) | old schedule recorded, local clocks excluded；专业约束 [metering contract](wasm-interface.md#sr2-metering-contract) | MET-01, SR2-RT-10; change clock/reset/degraded/current prices, expect identical core IDs/charges; full historical replay original schedule；[located partial source](../../../scripts/oasis7-node-wasm-metrics-monitor.test.sh) | candidate-bound canonical bytes and replay | process metrics 不是 authority |
| [req-dwe-003](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-003) | [sr2-host-publication](wasm-executor.design.md#sr2-host-publication) | pending replacement and version authority | LIN-01..04 / ACT-01 / SR2-RT-02..05; required both winner orders, crash reservation/pre/post commit, old/new activation; located governed_module_lifecycle_transaction_regressions.rs；[located partial source](../../../crates/oasis7/tests/module_lifecycle.rs) | lifecycle + GWSC runtime scenarios | current prepared seam 非完整 durable lineage |

## 12. 决策、长期风险与未决问题

### SR2 runtime 十组目标 fixture allocation
每行是目标 fixture contract；required 为同候选 deterministic test，full 为同候选真实 canonical/proof/consumer readback。尚未运行；现有 narrow tests 仅 partial locator，全部结果必须写 candidate-bound 的 actual evidence，不伪造未来 task IDs。

| Group | 注入与步骤 | 必须断言 | receiver / tier / current locator |
| --- | --- | --- | --- |
| SR2-RT-01 | submit/recovery前 loss finality，再reconnect | pending/expiry不延长，0receipt/effect/charge，state/resources不变 | #sr2-dc5-semantic-validator；DC5-03..05；required fault/full proof；effect_publication_transaction_regressions.rs partial |
| SR2-RT-02 | original/replacement/withdrawal两 canonical orders、same-block tie、duplicate transport、snapshot-tail restart | first effective one winner，exclusive losers0，reject/expire仅自己，independent能成功 | lifecycle #sr2-pending-contract；LIN-01..04；required/full consumer；current Agent ledger不是general test |
| SR2-RT-03 | recovery时撤权、耗尽resource、旧quote过期 | fresh revalidation reject/replan/expire，不partial publish | lifecycle #sr2-pending-contract；DC5-05；required/full actual state；module_runtime_tests.rs due-record preflight partial |
| SR2-RT-04 | activation前/at/后、oldsubmit/newfinalize、crossversionreplacement | finalized block chooses version，最多oneeffect，无payloadtranslation | lifecycle #sr2-activation-contract；ACT-01/LIN-01；required/full proof；governed_module_lifecycle_transaction_regressions.rs partial |
| SR2-RT-05 | missing/conflicting activation/historicalartifact、改变currentprice | restore failclosed，historicalbytes/schedule/charges不变 | lifecycle #sr2-activation-contract；MET-01；required/full replay；module_store.rs partial |
| SR2-RT-06 | forged/wrong world/request/provider/key/digest/root/branch/orphan receipt，exactduplicate | no unverified effects，originalduplicate stable，无productionselfsignfallback | lifecycle #sr2-external-contract/release #sr2-release-trust；RCP-01/02；required/full trust；effect_publication_transaction_regressions.rs partial |
| SR2-RT-07 | twoobjectmigration secondfail、outputcap/hash mismatch、eachpreparefailure | ALL oldbytes/registry/artifacts/schedule/resources/fees/events intact，success replay root equal | lifecycle #sr2-migration-publication；MIG-01/02；required/full replay；module_instance_publication_transaction_regressions.rs partial |
| SR2-RT-08 | oldSDK/manifestgolden/defaults、old/newevent exact replay、unknownnewversion | currentwire compatible，newunknownfailclosed，不混marker/contentaddress | interface #sr2-migration-contract；MIG-03；required/full replay；module_input_cbor.rs/module_lifecycle.rs partial |
| SR2-RT-09 | attachment swaps/missing/digestwrong/mixedtree/consumerblockermissing | schema+semantic reject，不DC5pass，newnonserviceablecount0 | #sr2-dc5-semantic-validator；DC5-01..05；required shape/full joined actual proof；game-world-state-sync-commit-module-required.sh partial |
| SR2-RT-10 | clock/reset/degraded/cache/uncharged-supported access变化 | unchanged deterministically coreID/state/root/fees；actualaccesscounts不因zero coefficient清零 | #sr2-host-publication；MET-01/02；required/full recorded replay；oasis7-node-wasm-metrics-monitor.test.sh partial |

### QA 十七负例在 runtime authority 的精确分配
同一候选/世界/窗口选择规则与 proof tier 继承上一表；schema negatives只能证明shape，runtime fault证明mutation，full才证明实际combined evidence。完整 QA receiver [GWSC negative matrix](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix) 与本表是分层分配而非重复 authority。

| Case | exact receiver | 注入 → assertion | required / full proof boundary |
| --- | --- | --- | --- |
| DC5-01 | #sr2-dc5-semantic-validator steps2/3 | swap candidate/world/source/window → invalid/blocked | structure+semantic joins / actual attachments |
| DC5-02 | #sr2-dc5-semantic-validator steps2/4/7 | remove attachment/proof/transition/consumer或template → blocked | schema/ref fixtures / actual bytes |
| DC5-03 | #sr2-dc5-semantic-validator steps4/5 | headregress/manifestconflict/compatmismatch/orphanfinality → no service/count0 | runtime fault / canonical proof |
| DC5-04 | #sr2-dc5-semantic-validator step4 | historychain有效但append/version缺失 → read-only/count0 | gate fixture / actual sync+execution |
| DC5-05 | #sr2-host-publication steps4/5 | greenreconnect后waiting intent proofrevoked → revalidate/zeroeffects/realblocker | fault / consumer readback |
| RCP-01 | lifecycle #sr2-external-contract | request/target/block/hash错或proof不可达 → unverified/rejected zeroeffects | trust fixture / provider+canonical bytes |
| RCP-02 | release #sr2-release-trust | local signer/provider/CI冒充proof → reject | negative fixtures / external trust readback |
| LIN-01 | lifecycle #sr2-pending-contract | activation original/replacement races bothorders → one winner losers0 | runtime / actual consumer+proof |
| LIN-02 | lifecycle #sr2-pending-contract | samehashduplicate then changedhash → stable result then no-mutation conflict | idempotency fixture / crossentry retry |
| LIN-03 | lifecycle #sr2-pending-contract | dangling/cycle/worldsubjectmismatch/regressedgeneration → reject independentunaffected | lineage fixture / actual snapshot |
| LIN-04 | lifecycle #sr2-migration-publication | crash reservation/prepublication/postpublication → none or one durablewinner no rerun | restart fixtures / durable actual journal |
| MIG-01 | lifecycle #sr2-migration-publication | missingdeclaration/badfromto/hash/artifact/state → oldstate/versionretained | migration fixture / retained bytes readback |
| MIG-02 | lifecycle #sr2-migration-publication | oversize/noncanonical/namespace/forgedcontext → wholeabort | bounded fixture / runtime replay |
| MIG-03 | lifecycle #sr2-migration-publication | oldbytesacrossmigration+restart → exactstate/hash/receipt no rewrite | replay fixture / actual snapshot-tail |
| MET-01 | #sr2-host-publication + lifecycle #sr2-activation-contract | changeprice/scheduleaftercommit → historicalcostunchanged | replay fixture / recorded receipt |
| MET-02 | #sr2-host-publication steps6/7 | count/chargeoverflow/fuel/balance/unsorted → no partialcharge/state/effect | bounds fixture / runtime transaction |
| ACT-01 | lifecycle #sr2-activation-contract | client/time/candidateblock disagrees → finalizedactivation selects; missingproof0effect | boundary fixture / canonical proof |

Additional encoding fixtures target exact semantic receiver: multibyte256byte overflow despite JSON character bound; U64 max/max+1/>2^53/leadingzero/exponent; Git SHA256 confusion and commit/tree swap; conflicting shadow scope; committed_rejection receipt; execution_core hash mismatch/circular inclusion; locatorchanged samebytes identity equal; matching digest wrongproofbody; supported unchargedaccess incorrectlyzeroed. Each rejects before authoritative mutation except valid equivalent-locator case; these are planned not executed.

选择 core/wrapper 分离防递归 hash，选择 canonical successful receipt count 防 rejection冒充效果。Current JSON receipt_leaf_hash/domain、local signer 与 target H 无替代关系。完整 durable implementation、P2P proof、consumer adapters 由 respective specialist 实现并由 QA fresh full evidence 解除；本次仅 author/check source。SC-9 八完整 proof cells、required provider-backed API parity、full真实 headed desktop/narrow S6 与 provider-backed Agent parity 保留 [GWSC SC9](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-sc9-eight-cells)，generic runtime scenarios 不 discharge 工业 gameplay obligation。
