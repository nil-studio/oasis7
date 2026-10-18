# oasis7 Runtime：WASM 扩展接口与 ABI（设计分册）
- 专业 owner：`wasm_platform_engineer`；状态：active professional authority（目标合同不等于已实现）。
- 内容审读基线：`eng-cc/oasis7`，源提交 `9c41d57b4436f71f0cf9b481043d882cfdc551ed`；内容整理日期：2026-09-26。
- 独立审读与实际实现/测试候选证据由该 task evidence 记录；此处不预报审读通过、runtime capability、发布或 full proof。下方既有条款仍有效；仅显式日期/历史基线条款按其证据范围解释。
## 1. 问题、目标与非目标
本次把既有专业义务承接为可执行的 target contract 与验证设计。成功结果是精确身份、边界、失败和恢复算法能被消费者读取；不改变 ABI/code/经济价格，不宣称目标已落地。下方原有细节、limits、公式、ID 与历史 evidence 原序保留并继续约束其适用范围。
## 2. 上游约束与相关角色
产品端到端 authority 为 [deterministic world execution](../../product/world-infrastructure/deterministic-world-execution.prd.md) 与 [distributed state availability](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md)；专业细化由 paired PRD、runtime/module 与 P2P 合同拥有。WASM owns codec/artifact/metering，runtime owns staged publication/replay，QA owns same-candidate acceptance，consumer owners own real projection。
### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [RCP-01 / RCP-02](wasm-executor.prd.md#sr2-domain-case-1) | 输入 wrong request/target/block/digest、不可获取 proof、local signer/provider/CI 伪装；断言 不计 success；external trust/finality 检查失败；新 effects=0 | [本域合同](wasm-interface.md#sr2-execution-contract) | runtime/QA；[RCP-01 / RCP-02外部receiver](wasm-executor.design.md#sr2-host-publication) | 当前localfixture不能证明full finality/provider/headed |
| [MIG-01 / MIG-02 / MIG-03](wasm-executor.prd.md#sr2-domain-case-2) | 输入 wrong from/to/hash、oversize/noncanonical output、namespace/caller forgery、旧 bytes跨迁移 restart；断言 拒绝完整 staged publication；old bytes保留；重放 recorded hashes一致 | [本域合同](wasm-interface.md#sr2-execution-contract) | runtime/QA；[MIG-01 / MIG-02 / MIG-03外部receiver](../module/module-lifecycle.md#sr2-migration-publication) | 当前localfixture不能证明full finality/provider/headed |
| [MET-01 / MET-02](wasm-executor.prd.md#sr2-domain-case-3) | 输入 历史 commit后改价、overflow/fuel/balance不足、unordered访问；断言 历史费用不变；提交前拒绝；free coefficient不抹 actual access | [本域合同](wasm-interface.md#sr2-execution-contract) | runtime/QA；[MET-01 / MET-02外部receiver](wasm-interface.md#sr2-metering-contract) | 当前localfixture不能证明full finality/provider/headed |
| [ENC-01](wasm-executor.prd.md#sr2-domain-case-4) | 输入 UTF8>256 bytes、U64 max/max+1/>2^53/leading0/exponent、GitHash混用/tree swap、scope冲突；断言 strict receiver拒绝；max lossless roundtrip；CBOR/JSON shape区分 | [本域合同](wasm-interface.md#sr2-execution-contract) | runtime/QA；[ENC-01外部receiver](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix) | 当前localfixture不能证明full finality/provider/headed |
| [HASH-01](wasm-executor.prd.md#sr2-domain-case-5) | 输入 detached inclusion变化、circular inclusion进入core、matching digest错误proof body、locator改变；断言 同core IDs稳定；fake inclusion拒绝；same bytes locator不改变content identity | [本域合同](wasm-interface.md#sr2-execution-contract) | runtime/QA；[HASH-01外部receiver](wasm-executor.design.md#sr2-dc5-semantic-validator) | 当前localfixture不能证明full finality/provider/headed |
| [E2](wasm-executor.prd.md#sr2-obligation-e2) | ### 实现要点（E2） | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E3](wasm-executor.prd.md#sr2-obligation-e3) | ### 实现要点（E3） | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E4](wasm-executor.prd.md#sr2-obligation-e4) | ### 实现要点（E4） | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E5](wasm-executor.prd.md#sr2-obligation-e5) | ### 实现要点（E5） | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E6](wasm-executor.prd.md#sr2-obligation-e6) | ### 实现要点（E6） | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E7](wasm-executor.prd.md#sr2-obligation-e7) | ### 实现要点（E7） | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E8](wasm-executor.prd.md#sr2-obligation-e8) | ### 实现要点（E8） | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E10](wasm-executor.prd.md#sr2-obligation-e10) | ### 实现要点（E10） | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E11](wasm-executor.prd.md#sr2-obligation-e11) | ### 实现要点（E11） | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E12](wasm-executor.prd.md#sr2-obligation-e12) | ### 实现要点（E12） | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E1](wasm-executor.prd.md#sr2-obligation-e1) | E1：选择 WASM 引擎并完成配置结构体与沙箱实现骨架。 | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E9](wasm-executor.prd.md#sr2-obligation-e9) | E9**：模块调用入口按 ModuleKind 选择并补充测试。 | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E13](wasm-executor.prd.md#sr2-obligation-e13) | E13**：将磁盘编译缓存从“原始 wasm 回盘”修正为“序列化 compiled artifact 回盘”，补齐 round-trip 与损坏恢复回归。 | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [E14](wasm-executor.prd.md#sr2-obligation-e14) | E14**：将 agent-os 对齐增强与 sandbox 安全硬化的永久契约并入本稳定权威，退役重复专题文档。 | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [PRD-ENGINEERING-006](wasm-executor.prd.md#sr2-obligation-prd-engineering-006) | &#124; PRD-ENGINEERING-006 &#124; 文档内既有任务条目 &#124; test_tier_required &#124; ./scripts/doc-governance-check.sh + 引用可达性扫描 &#124; 迁移文档命名一致性与可追溯性 &#124; | [本域合同](wasm-interface.md#sr2-execution-contract) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 下方原文限定范围不扩张，未执行能力不升级 |
| [journal suffix recovery](../prd.md#runtime-journal-suffix-recovery) | 成功 tick 的 wake/clear 可由较早 checkpoint 加非空 suffix 恢复，旧输出缺省不代替历史证据 | [tick 合同](#wasm-tick-journal-replay) | [runtime 宿主记录与验证](../runtime/runtime-integration.md#module-tick-journal-replay)；WASM/QA | 新宿主记录为 target，保持当前 ABI 与失败策略 |
## 3. 当前状态、目标状态与差距

| 对象 | 当前状态 / 基线 | 目标 | 差距与证据边界 |
| --- | --- | --- | --- |
| wasm-1 foundation | 原有代码/契约保留，源commit见身份块 | 兼容consumer及精确target contracts | 原有tests为located，不是本次run |
| receipt / migration / gate | interface仍声明target或partial | `sr2-execution-contract` 与runtime authority相连 | 代码、full finality/provider/headed证明不在本任务 |
| 文档与schema | 本文目标合同 | 语义/结构/anchors检查 | 文档通过不意味着runtime pass或SC9/DC5闭合 |
## 4. 边界与结构
输入声明和provider response为advisory，host/runtime负责authority/admission；sandbox仅确定性计算。build证明字节来源、SDK证明codec、metrics证明local诊断，各自不能签署world finality。ContentRef必须fetch/hash/length验证，再由相应authority验trust；current caches与local signature不能越过生产许可。
## 5. 关键运行流程
先绑定candidate/world branch/window与exact manifest/artifact/schema；strict decode和limits preflight后验证authority。成功只进入有权的下一阶段，世界效果须canonical commit receipt。缺字段、版本、proof、兼容或head continuity即停止，不隐式fallback或重试另一身份。重试同request hash幂等，changed hash拒绝；恢复重读当前canonical边界，历史receipt按记录版本重放。
## 6. 接口与数据合同
<a id="sr2-types"></a>
### 6.1 共用目标类型与编码
本节是以下新目标记录的唯一类型 authority；它不改变现行 `wasm-1`，也不把已有 envelope 的 opaque payload 校验升级为语义 descriptor 校验。目标记录拒绝未知 schema_version、字段、enum、重复 key 和 null；可选字段仅可省略。

| 类型 | 精确形状 / 约束 | 判定 authority |
| --- | --- | --- |
| Id | UTF-8 文本，1..256 **bytes**；不 trim/casefold/Unicode normalize | 语义接收器检查 UTF-8 encoded length；JSON maxLength 仅词法近似 |
| U64 / PositiveU64 | 0..18446744073709551615 / 1..18446744073709551615，checked arithmetic | 所有 JSON U64 为 `^(0\|[1-9][0-9]*)$` 十进制字符串并检查上界；CBOR unsigned shortest integer；拒绝符号、前导零、指数和 float |
| Hash32 | SHA-256 32 bytes；JSON 64 lowercase hex | 不得与 Git OID 混用 |
| GitOid40 | Git SHA-1 40 lowercase ASCII hex | source_oid/integration_base_oid 回读 commit，tested_tree_oid 回读 tree；缩写和对象种类互换拒绝 |
| ResourceAmount | 当前 runtime 非负整数范围及显式 resource unit；JSON decimal string | 不新增价格，checked current-range conversion；balance 与 amount 同单位 |
| WorldScopeV1 | `{world_id:Id,branch_id:Id}` | canonical world branch，不是 Git branch；独立记录包含一次，嵌套行继承且禁止冲突 |
| JournalPositionV1 | `{event_id_era:U64,event_id:U64}` | lexicographic order，era rollover 不能混同普通单个索引 |
| ContentRefV1 | `{digest:Hash32,byte_length:U64,locator:Id}` | fetch 后长度/hash 验证；commitment 只含 digest+length，locator 不证明 authority |
| CandidateIdentityV1 | `{source_oid:GitOid40,integration_base_oid:GitOid40,tested_tree_oid:GitOid40,execution_version:Id,protocol_version:Id,test_contract_version:Id}` | task/run 是 evidence provenance，不进入世界执行 core |
| ServiceGradeV1 | `verified_serviceable / stale_catching_up / verified_read_only / unavailable_isolated` | 不存在第五个 quarantined enum；health green 不能升级 write gate |
| VerificationVerdictV1 | `verified / missing / conflicting / incompatible` | proof readback 计算，作者 pass 字段不构成证据 |
仅 `committed_receipt_count`、`effective_world_effect_count` 的 JSON integer enum 0/1 是明确例外；schema_version 使用类型名 literal，不以 commit OID 代替 schema revision。 目标 `H(domain,body)=SHA256(UTF8(domain)||0x00||CanonicalCBOR(body))`。CBOR 使用 definite lengths、唯一 canonical map keys、shortest integers、无 float、semantic arrays 保序、sets 按完整稳定键排序且唯一。ContentRef locator、自身 ID/signature、本地 timestamp 与递归依赖 containing-block/finality wrapper 均不进入 core。外层 JSON 不是 CBOR hash 替身。 精确域为 `oasis7/execution-result/v1`、`oasis7/module-execution-receipt/v1`、`oasis7/intent-request/v1`、`oasis7/intent-decision/v1`、`oasis7/external-effect-receipt/v1`、`oasis7/version-activation/v1`、`oasis7/module-state-migrated/v1`、`oasis7/world-execution-state-sync-attachment/v1`。现行 `receipt_leaf_hash` 与 modsig domains 保持原义，不是这些域的 alias。
<a id="sr2-execution-contract"></a>
### 6.2 有效执行 core / finality wrapper
以下是 target/unimplemented 精确记录，schema_version 每一处为对应类型名 literal：

```text
ExecutionResultCoreV1 = { schema_version:"ExecutionResultCoreV1", scope:WorldScopeV1, intent_id:Id, request_hash:Hash32, execution_version:Id, governing_manifest_hash:Hash32, parent_state_root:Hash32, ordered_input_hash:Hash32, activation_id:Hash32 } ExecutionIdentityV1 = { schema_version:"ExecutionIdentityV1", execution_id:Hash32, execution_core:ExecutionResultCoreV1, canonical_inclusion:{canonical_block_hash:Hash32,canonical_block_height:U64,proof:ContentRefV1} } ModuleExecutionReceiptCoreV1 = { schema_version:"ModuleExecutionReceiptCoreV1",execution_id:Hash32, execution_core:ExecutionResultCoreV1, module_identity:{module_id:Id,module_version:Id,instance_id:Id,interface_version:Id,wasm_hash:Hash32}, metering_schedule_version:Id,metering_schedule_hash:Hash32, input_hash:Hash32,output_hash:Hash32,input_bytes:U64,output_bytes:U64, fuel_limit:U64,fuel_used:U64,effect_count:U64,emit_count:U64, access_counts:{component_reads:U64,component_writes:U64,schema_decodes:U64}, state_before_hash:Hash32,state_after_hash:Hash32, ordered_effects_hash:Hash32,ordered_emits_hash:Hash32, charges:[{resource_type:Id,amount:ResourceAmount,balance_before:ResourceAmount,balance_after:ResourceAmount}], journal_start:JournalPositionV1,journal_end:JournalPositionV1,status:"committed_success" } ModuleExecutionReceiptV1 = { schema_version:"ModuleExecutionReceiptV1",receipt_id:Hash32, receipt_core:ModuleExecutionReceiptCoreV1, finality_binding:{canonical_block_hash:Hash32,canonical_block_height:U64,proof:ContentRefV1} }

```
先计算 execution_id=H(execution-result,execution_core)，验证其一致后计算 receipt_id=H(module-execution-receipt,receipt_core)。scope 仅在 execution_core；wrapper 继承，不能再复制。receipt_core 不嵌入 ExecutionIdentityV1 finalized wrapper。proof 必须绑定两 ID、同一 canonical block/version/activation；不同 containing-block wrapper 不改变 core ID，伪造或矛盾 inclusion 拒绝。fuel_used<=fuel_limit；journal_start<=journal_end；charges 按 resource_type 排序唯一且 after=before-amount，无负余额和溢出。 只有 `committed_success` receipt 是有效执行。拒绝/过期/重规划/无效果终止由 [IntentDecisionV1](../module/module-lifecycle.md#sr2-pending-contract) 拥有；它们不产生 module success receipt、不夺 lineage winner。pending/外部 ACK 不算 committed；非 serviceable 或 manifest/head/finality 缺失的新 intent 必须零 receipt、零效果、零 charge/state publication，历史 receipt 仍保留。完整执行/prepare/publish/失败与 proof checks 由 [host publication](wasm-executor.design.md#sr2-host-publication) 接收。 Decision/Pending hash body 由lifecycle单authority定义：receipt refs 以canonical receipt_core bytes为ContentRef目标，digest=SHA256(CanonicalCBOR(receipt_core))、byte_length为实际core字节数；此digest不是domain-separated receipt_id。接收器重算receipt_id并另外验证detached finalized wrapper，不把包含同一区块的wrapper digest嵌入decision core。
<a id="sr2-metering-contract"></a>
### 6.3 Schedule identity 与确定性计量
ScheduleDescriptorV1 目标形状为 `{schema_version:"ScheduleDescriptorV1",metering_schedule_version:Id,input_kib_bytes:PositiveU64,output_kib_bytes:PositiveU64,compute:{input_kib:ResourceAmount,output_kib:ResourceAmount,effect:ResourceAmount,emit:ResourceAmount},electricity:{call:ResourceAmount,effect:ResourceAmount,emit:ResourceAmount,new_state:ResourceAmount},extension_access:{component_read:ResourceAmount,component_write:ResourceAmount,schema_decode:ResourceAmount}}`。descriptor canonical bytes 的 SHA-256 是 metering_schedule_hash；系数使用相应资源单位，KiB divisors 使用 byte 单位。当前公式映射 divisors=1024、compute coefficients=1/1/2/1、electricity=1/1/1/1；该映射不声称当前代码已有 ScheduleDescriptorV1。 先 bounds-check input/output/fuel/计数，再用 checked `n/1024 + (n%1024 != 0)` 计算 ceil，避免 n+1023 溢出；按 schedule 计算 checked amounts，再验证余额，最后随 staged transaction 提交。扩展未支持/未使用时 actual counts 为零；支持但 coefficient=0 的访问仍记录实际 counts，不因免费而抹除。扩展费用必须另经 manifest/schedule 治理，不在本文启用。历史 replay 消费原 schedule/hash/charges，不按当前价重算；timing、unordered collections 和 provider 自报 cost 不参与费用。
<a id="sr2-migration-contract"></a>
### 6.4 精确 schema migration 与事件身份

```text
SchemaMigrationV1 = { object_kind:component|event,namespace:Id,name:Id, from_schema_version:PositiveU64,to_schema_version:PositiveU64, from_schema_hash:Hash32,to_schema_hash:Hash32, migration_wasm_hash:Hash32,migration_manifest_hash:Hash32, instance_id:Id,entity_id:Id,output_limit_bytes:PositiveU64 } ModuleStateMigratedCoreV1 = { schema_version:"ModuleStateMigratedV1",scope:WorldScopeV1,migration:SchemaMigrationV1, from_state_hash:Hash32,to_state_hash:Hash32,base_manifest_hash:Hash32, governing_manifest_hash:Hash32,activation_id:Hash32,caused_by:ContentRefV1, journal_position:JournalPositionV1 } ModuleStateMigratedV1 = { schema_version:"ModuleStateMigratedV1",scope:WorldScopeV1,migration:SchemaMigrationV1, from_state_hash:Hash32,to_state_hash:Hash32,base_manifest_hash:Hash32, governing_manifest_hash:Hash32,activation_id:Hash32,caused_by:ContentRefV1, journal_position:JournalPositionV1,canonical_block_hash:Hash32,canonical_block_height:U64 }

```
ModuleStateMigratedCoreV1 是上方事件排除 containing-block hash/height 的 hash body 投影（不是第二种 event wire）；domain module-state-migrated 的 ID 由该 body 计算，block wrapper 与 governance proof 单独校验。旧 `component` 示例精确映射 migration.name；不删除旧示例，不把其缺字段形状当新合同。from/to versions 必须为确切声明且不等，numeric 大小不授予迁移权；instance/entity/namespace/name/object 和所有 descriptor/artifact/state/manifest hashes 必须匹配。
迁移只能输出 declared namespace 的 bounded canonical state；event adapter 不改历史 caller/time/caused_by/kernel identity。失败保留旧 state/version/artifact，零部分 event/charge。old schema replay 到边界，执行显式迁移后 new schema replay；旧 bytes 永不改写，无 migration 仅可由声明支持旧版本的 reducer 处理。[publication ordering](../module/module-lifecycle.md#sr2-migration-publication) 与 [activation](../module/module-lifecycle.md#sr2-activation-contract) 是 runtime authority。
## 7. 状态、事务与持久化
本域记录不能把 accepted/built/decoded/observed 当 applied。SDK/build/metrics 的本地输出不拥有世界事务；interface定义receipt core，runtime staged commit拥有state/effects/charges/registry/journal/winner同一发布点。失败全体不发布，crash在prepublish与postpublish由durable receipt/winner/index恢复，禁止重跑已commit WASM。实际prototype atomic seam仅bounded prepared proposal，不能推导完整事务。snapshot保留原manifest/artifact/schedule/lineage与历史bytes。
## 8. 部署、安全与运行约束
生产binary-only及external finality仍由release/runtime policy；无Docker/wasm32/provider环境不能伪报通过。modules无clock/random/I/O，host注入caller/time/target；namespace/capability/AUTHZ约束全部仍有效。计数、字节和费用checked bounds-before-allocation/mutation，limit来源为现行ModuleLimits与各paired PRD；diagnostics不得泄露raw payload/keys/proof。
## 9. 质量与容量

| 环境 / 输入 | 预期响应 | 阈值来源 | 验证入口 / 当前范围 |
| --- | --- | --- | --- |
| required canonical fixture / malformed bytes、integer overflow、wrongscope | structured rejection且不发布state/effect/charge | interface类型、现行limits及paired PRD | §11.1各case；target设计未运行 |
| full实际canonical环境 / restart、activation、proof撤销 | durable replay/identity与no-new-effect gate一致 | runtime/GWSC各精确receiver | samecandidate artifacts mandatory，未证明 |
| 现行资源/perf输入 | 不扩大既有budget，不用localtiming计费 | 下方原有limits、NFR与perf方案 | 既有tests/scripts仅located partial |
## 10. 兼容、迁移与回滚
保留legacy serde defaults、wire exports、工件不可覆盖与历史bytes。新target先固定合同再逐consumer启用，unknown version拒绝；当前ABI不变。回滚到身份块源码/旧consumer，仅影响新请求；已canonical commit的事件、charges、receipt不可撤销或改价。迁移须explicit from/to descriptor及governed artifact；缺proof停在旧版，不能以代码合入或健康恢复启用。
## 11. 验证设计与可追溯性
本次文档变更只交付文档合同采纳与local checks；实际实现、runtime required/full、provider、headful、production proof分别在真实实现task evidence记录candidate/source/integration/tested tree与环境，缺失保持未证明，不能用FutureIssue占位代替本页receiver。全DC5验证由 [GWSC receiver](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-attachment) 与其 [schema](../../testing/schemas/world-execution-state-sync-attachment.schema.json) 拥有。
### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [RCP-01 / RCP-02](wasm-executor.prd.md#sr2-domain-case-1) | [本域合同](wasm-interface.md#sr2-execution-contract) | 不计 success；external trust/finality 检查失败；新 effects=0 | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；target case `RCP-01 / RCP-02` 输入 wrong request/target/block/digest、不可获取 proof、local signer/provider/CI 伪装；required deterministic/codec、full actual canonical，同candidate/world/window选择；existing source不宣称已覆盖target | 本文档采纳；实际task evidence存candidate/artifact/proof与exitcode | 本次未执行；proof语义与真实环境未证明 |
| [MIG-01 / MIG-02 / MIG-03](wasm-executor.prd.md#sr2-domain-case-2) | [本域合同](wasm-interface.md#sr2-execution-contract) | 拒绝完整 staged publication；old bytes保留；重放 recorded hashes一致 | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；target case `MIG-01 / MIG-02 / MIG-03` 输入 wrong from/to/hash、oversize/noncanonical output、namespace/caller forgery、旧 bytes跨迁移 restart；required deterministic/codec、full actual canonical，同candidate/world/window选择；existing source不宣称已覆盖target | 本文档采纳；实际task evidence存candidate/artifact/proof与exitcode | 本次未执行；proof语义与真实环境未证明 |
| [MET-01 / MET-02](wasm-executor.prd.md#sr2-domain-case-3) | [本域合同](wasm-interface.md#sr2-execution-contract) | 历史费用不变；提交前拒绝；free coefficient不抹 actual access | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；target case `MET-01 / MET-02` 输入 历史 commit后改价、overflow/fuel/balance不足、unordered访问；required deterministic/codec、full actual canonical，同candidate/world/window选择；existing source不宣称已覆盖target | 本文档采纳；实际task evidence存candidate/artifact/proof与exitcode | 本次未执行；proof语义与真实环境未证明 |
| [ENC-01](wasm-executor.prd.md#sr2-domain-case-4) | [本域合同](wasm-interface.md#sr2-execution-contract) | strict receiver拒绝；max lossless roundtrip；CBOR/JSON shape区分 | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；target case `ENC-01` 输入 UTF8>256 bytes、U64 max/max+1/>2^53/leading0/exponent、GitHash混用/tree swap、scope冲突；required deterministic/codec、full actual canonical，同candidate/world/window选择；existing source不宣称已覆盖target | 本文档采纳；实际task evidence存candidate/artifact/proof与exitcode | 本次未执行；proof语义与真实环境未证明 |
| [HASH-01](wasm-executor.prd.md#sr2-domain-case-5) | [本域合同](wasm-interface.md#sr2-execution-contract) | 同core IDs稳定；fake inclusion拒绝；same bytes locator不改变content identity | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；target case `HASH-01` 输入 detached inclusion变化、circular inclusion进入core、matching digest错误proof body、locator改变；required deterministic/codec、full actual canonical，同candidate/world/window选择；existing source不宣称已覆盖target | 本文档采纳；实际task evidence存candidate/artifact/proof与exitcode | 本次未执行；proof语义与真实环境未证明 |
| [E2](wasm-executor.prd.md#sr2-obligation-e2) | [本域合同](wasm-interface.md#sr2-execution-contract) | ### 实现要点（E2） | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E3](wasm-executor.prd.md#sr2-obligation-e3) | [本域合同](wasm-interface.md#sr2-execution-contract) | ### 实现要点（E3） | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E4](wasm-executor.prd.md#sr2-obligation-e4) | [本域合同](wasm-interface.md#sr2-execution-contract) | ### 实现要点（E4） | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E5](wasm-executor.prd.md#sr2-obligation-e5) | [本域合同](wasm-interface.md#sr2-execution-contract) | ### 实现要点（E5） | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E6](wasm-executor.prd.md#sr2-obligation-e6) | [本域合同](wasm-interface.md#sr2-execution-contract) | ### 实现要点（E6） | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E7](wasm-executor.prd.md#sr2-obligation-e7) | [本域合同](wasm-interface.md#sr2-execution-contract) | ### 实现要点（E7） | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E8](wasm-executor.prd.md#sr2-obligation-e8) | [本域合同](wasm-interface.md#sr2-execution-contract) | ### 实现要点（E8） | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E10](wasm-executor.prd.md#sr2-obligation-e10) | [本域合同](wasm-interface.md#sr2-execution-contract) | ### 实现要点（E10） | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E11](wasm-executor.prd.md#sr2-obligation-e11) | [本域合同](wasm-interface.md#sr2-execution-contract) | ### 实现要点（E11） | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E12](wasm-executor.prd.md#sr2-obligation-e12) | [本域合同](wasm-interface.md#sr2-execution-contract) | ### 实现要点（E12） | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E1](wasm-executor.prd.md#sr2-obligation-e1) | [本域合同](wasm-interface.md#sr2-execution-contract) | E1：选择 WASM 引擎并完成配置结构体与沙箱实现骨架。 | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E9](wasm-executor.prd.md#sr2-obligation-e9) | [本域合同](wasm-interface.md#sr2-execution-contract) | E9**：模块调用入口按 ModuleKind 选择并补充测试。 | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E13](wasm-executor.prd.md#sr2-obligation-e13) | [本域合同](wasm-interface.md#sr2-execution-contract) | E13**：将磁盘编译缓存从“原始 wasm 回盘”修正为“序列化 compiled artifact 回盘”，补齐 round-trip 与损坏恢复回归。 | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [E14](wasm-executor.prd.md#sr2-obligation-e14) | [本域合同](wasm-interface.md#sr2-execution-contract) | E14**：将 agent-os 对齐增强与 sandbox 安全硬化的永久契约并入本稳定权威，退役重复专题文档。 | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [PRD-ENGINEERING-006](wasm-executor.prd.md#sr2-obligation-prd-engineering-006) | [本域合同](wasm-interface.md#sr2-execution-contract) | &#124; PRD-ENGINEERING-006 &#124; 文档内既有任务条目 &#124; test_tier_required &#124; ./scripts/doc-governance-check.sh + 引用可达性扫描 &#124; 迁移文档命名一致性与可追溯性 &#124; | [现有partial ABI source](../../../crates/oasis7_wasm_abi/tests/open_module_commands.rs)；按原条款输入/阈值及本页domain cases验证，required兼容/codec/build/窗口fixture，full仅真实同candidate跨宿主/authority proof；原文manual/run来源仍适用 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [journal suffix recovery](../prd.md#runtime-journal-suffix-recovery) | [tick 合同](#wasm-tick-journal-replay) | 成功 tick 的 wake/clear、失败 route 保留原 schedule；C<T 恢复和后续调用等价 | [现有 tick 事务测试](../../../crates/oasis7_runtime/src/tests/module_tick_routing_transaction_regressions.rs) 扩展宿主 TICK-JOURNAL-01..05；required、真实 WASM、固定 world/manifest/C/T 与非空 suffix | planned/未运行；schedule、调用、费用与兼容负例对照 | 现有 snapshot/mock 不证明该矩阵，不升级整体 capability |
现有partial/unrun来源：`crates/oasis7_wasm_executor/src/tests.rs`（output/fuel/memory/cache），`crates/oasis7_wasm_abi/tests/open_module_commands.rs`（legacy/envelope），`crates/oasis7_runtime/src/world/module_runtime_tests.rs`（metrics/due-preflight），`scripts/oasis7-node-wasm-metrics-monitor.test.sh`（window summary），`scripts/ci-verify-m1-wasm-summaries.py`（build对账）。这些不是新target receipt/migration/DC5已覆盖证据。GWSC [negative matrix](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix) 接收全部17QA cases，[runtime scenarios](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios) 接收10组runtime；本页domain rows为具体输入/assertion分配。
## 12. 决策、长期风险与未决问题
选择单authority typedtarget而非重复wire，是为避免scope/hash/版本漂移。已知风险是implementation缺失、proof不可读、U64/UTF8投影误读、递归hash与历史误作新候选证据；WASM/runtime/QA分别按本页case和runtimereceiver解除。触发器为新ABI/schedule/schema、trust epoch、activation或consumer变更；需重新冻结合同与专业联审。SC9八fullcells及activeLLM/provider API parity、真实desktop+narrow/headed证据保持产品/QAauthority，不因本页文档完成消除。
审计轮次: 5
本分册为 `doc/world-runtime/prd.md` 的详细展开。
SDK 的默认 no_std、共享 Canonical-CBOR wire 类型、codec 错误与 builtin 兼容 evidence 由 `doc/world-runtime/wasm/wasm-sdk.prd.md` 承载；本文继续作为 runtime ABI/reference 入口，不承担 SDK 实现任务账本。
`crates/oasis7_wasm_abi` 是 `ModuleManifest`、ABI contract、artifact identity 与 limits 类型的单一代码来源；net/proto/viewer 不得维护重复定义。历史 distributed Phase 7 移除 `oasis7_net` 重复 manifest 属于已完成 crate-boundary provenance，不构成第二套现行规范。
## 状态与证据等级
本文用 `current`（现行 wire/行为）、`partial`（已有受限路径但未闭环）、`target`（待冻结合同）和 `proven`（有实现与回归证据的边界）四个标签，避免把接口存在
误读为端到端能力。`proven` 只能外推到同一 ABI/版本/路径，不能外推到相邻目标能力。
- **Proven foundation：** 版本化 command 声明/admission、active-module 确定性 catalog、Canonical-CBOR envelope byte-exact 校验、sandbox 前的 active/module/schema/hash/大小检查和既有计量链。
- **Partial bridge/schema：** `CapabilityCatalogSnapshot`、`AgentCommandResponse`、subject-bound context 和 provider/loopback 校验已存在；自动生产接线、真实 Agent E2E、caller capability、最终 receipt 及 payload 语义解码仍未闭合。当前 `schema_hash` 只绑定声明/envelope 标识，payload 仍是 opaque byte string。
- **Partial instance：** lifecycle、tick schedule 和部分 state 已按 instance 工作；direct/trusted command state、catalog target 和 provenance 仍有 `module_id` 路径。
- **Target closure：** per-command semantic descriptor、受信 host typed-args、完整 instance target、统一 staged transaction、execution receipt、schema migration 和生产 Agent E2E。
因此，“WASM/provider 基础设施已进入代码”是当前准确结论；“新制度已能被 Agent 动态发现、理解、授权、执行并回放”仍是试点验收结论，不能由单项接口或局部测试代签。
## WASM 扩展接口（草案）
> 目标：允许 Agent 自行设计“新事物”模块（Rust → WASM），由世界内核以事件/接口动态调用；模块只产生确定性计算与显式 Effect 意图。
**ModuleManifest（控制面条目）**
```rust
struct ModuleManifest {
    module_id: String,         // 内容地址或哈希
    name: String,
    version: String,           // 语义化版本
    kind: ModuleKind,          // Reducer / Pure（后续可扩展）
    role: ModuleRole,          // Rule / Domain / Gameplay / Body / AgentInternal
    wasm_hash: String,         // 模块工件哈希
    interface_version: String, // 例如 "wasm-1"
    exports: Vec<String>,      // 导出函数名
    subscriptions: Vec<ModuleSubscription>,
    required_caps: Vec<CapabilityRef>,
    limits: ModuleLimits,      // 沙箱资源上限
}
```
**ModuleKind**
- `Reducer`：有状态的确定性 reducer（输入事件 → 新状态 + Effect 意图）
- `Pure`：无状态纯函数组件（输入 → 输出）
**ModuleRole**
- `Rule`：动作校验/成本评估等规则模块
- `Domain`：经济、社会、治理等领域规则
- `Gameplay`：战争/危机/经济玩法协议与状态机模块
- `Body`：机体/零件/耐久等物理外层逻辑
- `AgentInternal`：记忆/工具/规划等内部能力
**Agent 内部模块（记忆/工具）**
- 仍使用 `ModuleManifest`/`ModuleLimits` 与统一 ABI；由 Agent runtime 触发调用，不走 event/action 订阅路由。
- 记忆模块通常使用 `Reducer`，以 `state` 作为受限持久存储；运行时对状态大小/条目数施加配额（实现可为专用存储或状态分片）。
- 模块保持确定性计算，不直接调用外部 I/O；LLM/推理服务调用在模块外部完成。
**ModuleSubscription**
- `event_kinds`: Vec<String>（订阅的事件类型）
- `action_kinds`: Vec<String>（可选，订阅的动作类型）
- `stage`: `"pre_action" | "post_action" | "post_event" | "tick"`（动作/事件/周期路由阶段）
- `filters`: 可选过滤条件（例如仅关注某类 owner/地点）
<a id="wasm-tick-journal-replay"></a>
**Tick 生命周期调度（执行器合同）**
- 只有激活且订阅 `tick` 的实例进入调度表，激活时初排于当前 logical tick。`instance_id -> next_wake_tick` 随 snapshot 持久化；restart/replay 不得提前唤醒、丢失未来 wake 或复活 suspend。较早 checkpoint 的 suffix 恢复依 [宿主调度结果合同](../runtime/runtime-integration.md#module-tick-journal-replay)。
- `step_with_modules` 每次推进 logical time 后只调用到期实例。tick 调用的 `ctx.stage` 为 `"tick"`，`event` 与 `action` 均省略；reducer 仍携带其受限 `state`，pure 模块不携带 `state`。
- 执行器在暂存视图中移除旧 schedule；成功 route 原子发布最终 wake/clear。只有 `tick_lifecycle.wake_after_ticks` 写入下一次 schedule，`ticks=0` 按 `1` clamp；`tick_lifecycle.suspend` 或字段缺省均 clear。整条 route 失败不安装暂存结果，保留 route 前 schedule，并沿既有结构化错误与失败审计规则返回，不在失败返回前静默重试。
- 这一定时语义不改变模块权限、artifact identity 或 manifest hash 规则；新增/变更 tick subscription 仍按普通 manifest 和 artifact 校验链处理。
**ModuleLimits（示意字段）**
```rust
struct ModuleLimits {
    max_mem_bytes: u64,        // 线性内存上限
    max_gas: u64,              // 指令燃料
    max_call_rate: u32,        // 每 tick 最大调用次数
    max_output_bytes: u64,     // 输出上限（ModuleOutput 编码后大小）
    max_effects: u32,          // 单次调用最大 effect 数量
    max_emits: u32,            // 单次调用最大 event 数量
}
```
**Reducer 调用签名（示意）**
```rust
fn reduce(event: WorldEvent, state: Bytes, ctx: ModuleContext) -> ModuleOutput
```
**Pure 调用签名（示意）**
```rust
fn call(input: Bytes, ctx: ModuleContext) -> Bytes
```
**ModuleContext / ModuleOutput（示意）**
- `ModuleContext`：`{ time, origin, stage?, world_config, module_id, trace_id }`
- `ModuleOutput`：`{ new_state, effects: Vec<EffectIntent>, emits: Vec<WorldEvent> }`
**模块生命周期事件（占位）**
- `RegisterModule / ActivateModule / DeactivateModule / UpgradeModule`
- 以事件写入日志，支持审计与回放
**模块失败事件（当前）**
- `ModuleCallFailed { module_id, trace_id, reason }`
- 加载/校验阶段失败当前映射到 `ModuleCallFailed`（由 `code/detail` 区分）。
**规则决策事件（占位）**
- `RuleDecisionRecorded { action_id, module_id, stage, verdict, override_action?, cost, notes }`
- `ActionOverridden { action_id, original_action, override_action }`
- Rule 模块通过 `ModuleOutput.emits` 输出 `kind="rule.decision"`，`payload` 为 `RuleDecision` 的 JSON 序列化。
- simulator 的 historical `WorldKernel` adapter 可将单个 pre-action call 映射到这一 payload；它不扩展本 ABI 的 module lifecycle、artifact governance 或多模块编排语义，且无效 payload / action id 必须由 adapter 结构化拒绝，不能静默降级。
## ABI 与序列化（草案）
> 目标：模块与宿主之间的输入/输出采用**确定性**编码，保证回放与跨平台一致性。
**编码格式**
- 使用 **Canonical CBOR**（键排序、确定性编码）。
- 禁止 NaN；浮点仅在明确字段允许时使用（默认使用整数与字节串）。
- `GeoPos` 与所有 `*_cm` 坐标/尺寸字段在 action/event/observation 边界上只接受并输出 CBOR/JSON 整数厘米值，不接受 float 类型；仅模块持久化状态解码可兼容读取历史上的整值浮点厘米表示。
- `Bytes` 一律使用 CBOR byte string。
- WasmExecutor 输出解码使用 Canonical CBOR（失败映射为 InvalidOutput）。
**ModuleContext（CBOR Map）**
```
{
  "v": "wasm-1",
  "module_id": "...",
  "trace_id": "...",
  "time": i64,
  "origin": { "kind": "event|action|system", "id": "..." },
  "stage": "pre_action|post_action|post_event|tick", // action/event/tick 路由阶段（可选）
  "world_config_hash": "...", // 当前 manifest 哈希
  "limits": { "max_mem_bytes": u64, "max_gas": u64, "max_output_bytes": u64 }
}
```
**Reducer 输入（CBOR Map）**
```
{
  "ctx": ModuleContext,
  "event": Bytes,   // WorldEvent 的 canonical CBOR
  "state": Bytes    // reducer 当前状态（canonical CBOR，若无则为空字节串）
}
```
**Pure 输入（CBOR Map）**
```
{ "ctx": ModuleContext, "input": Bytes }
```
**ModuleCallInput（当前运行时）**
- 当前 runtime 以 `ModuleCallInput { ctx, event?, action?, state? }` 作为调用输入封装。
- `event`/`action` 字段均为 canonical CBOR bytes。
- reducer 调用会携带 `state`（空字节串代表无历史状态），pure 调用省略 `state`。
- `pre_action` 阶段仅提供 `action`；`post_action` 阶段提供 `action` + `event`（动作落盘后的结果事件）；`post_event` 阶段仅提供 `event`。
**ModuleOutput（CBOR Map）**
```
{
  "new_state": Bytes | null,
  "effects": [ Bytes ], // EffectIntent 的 canonical CBOR 列表
  "emits": [ Bytes ],   // WorldEvent 的 canonical CBOR 列表
  "tick_lifecycle": { "mode": "wake_after_ticks", "ticks": u64 }
                    | { "mode": "suspend" } // 可选；缺省表示不重排
}
```
- 当 `new_state` 为非空时，运行时记录 `ModuleStateUpdated` 事件并更新模块状态。
- Pure 模块不得返回 `new_state`，否则视为 InvalidOutput。
- `tick_lifecycle` 是 ABI 的可选、serde-defaulted 扩展：旧模块输出省略该字段仍可解码，且其既有非-tick 调用行为不变；若该输出来自 tick 调用，缺省的兼容含义是 suspend（不再调度），而不是隐式每 tick 重试。
- 回归至少覆盖初排、到期、wake、成功 route 的 suspend/缺省、零 ticks clamp，以及失败 route 丢弃业务与 wake 暂存、保留原 schedule 并返回既有结构化错误/失败审计；不承诺自动重试。另覆盖 snapshot round-trip 和 [非空 suffix 与恢复后续跑验证](../runtime/runtime-integration.md#module-tick-journal-replay-validation)。
**错误约定**
- 模块返回非规范 CBOR、输出超限或字段缺失时，宿主记录 `ModuleCallFailed` 事件并拒绝输出。
## 开放制度扩展合同（试点，非全面迁移）
本节把“制度能力由 WASM 扩展”的合理部分落成一个可审计的增量合同。它是
现有 `wasm-1` ABI 的可选扩展，不是立即把 `WorldState` 改成 ECS、删除既有
native action，或允许任意未治理工件进入生产执行路径。
> **状态边界：command 与 provider 各有基础切片，但尚未闭环。** 当前
> `ModuleAbiContract.declarations.commands` 已提供版本化 command 声明；manifest
> admission 会校验声明，active module 可投影为确定性的 command catalog，
> `World::execute_module_command` 会在触碰 sandbox 前校验 active 状态、声明匹配、
> schema/hash/payload 边界，并复用现有计量调用链。`CapabilityCatalogSnapshot`、
> subject-bound context、`AgentCommandResponse` 及 provider/loopback 校验也已进入
> 代码。这些是可审计的基础切片，不是完整协议：payload 语义 schema、trusted host
> typed-args 编码、instance target、component/event/migration、execution receipt、
> 完整 capability scope、生产 context 自动接线、真实 Agent E2E 和 staged atomic
> commit 仍是目标扩展。下面的完整结构和字段名仍是语义草图，不能直接当作现行 wire
> 定义；试点实现前还必须补齐 ABI 版本、编码规范、hash/receipt 绑定、迁移策略和
> 失败路径测试。当前生产路径仍同时承诺本文件前文定义的现有 `wasm-1` 行为与上述
> 已落地的 command/provider 基础切片。
试点只选择一个有界制度（例如 Alliance 或 EconomicContract，由跨角色治理
确定），以现有 native vertical slice 作为兼容基线；试点成功前不得把所有
经济、战争或治理能力迁出内核。模块治理、工件发布和实例生命周期继续遵循
`doc/world-runtime/module/module-lifecycle.md` 的 `propose -> shadow ->
approve -> apply` 闭环。
### 1. 命名空间、声明和版本
模块的开放对象**目标上**必须在 manifest 中声明。当前 command 已是
`ModuleAbiContract.declarations.commands` 中 serde-defaulted 的版本化字段，并有
声明 admission；但当前声明集合仍只覆盖 command，不包含 component、event 或
migration。扩展这些对象前必须定义旧 manifest 的读取方式、新旧 ABI 的
admission/migration 规则，并证明历史 `wasm-1` 模块仍走原有路径。
```rust
// Target sketch for the full declaration set; the current wire type only supports commands.
struct ModuleSchemaDeclarations {
    commands: Vec<CommandSchema>,
    components: Vec<ComponentSchema>,
    events: Vec<EventSchema>,
    migrations: Vec<SchemaMigration>,
}
// Target sketch only; canonical encoding and capability binding remain prerequisites.
struct SchemaRef {
    name: String,
    schema_version: u32,
    schema_hash: String, // SHA-256 of the canonical schema descriptor
    max_payload_bytes: u64,
}
// Target sketch only; migration_manifest_hash must be bound before pilot admission.
struct SchemaMigration {
    object_kind: String, // "component" or "event"
    namespace: String,
    name: String,
    from_schema_version: u32,
    to_schema_version: u32,
    from_schema_hash: String,
    to_schema_hash: String,
    migration_wasm_hash: String,
    migration_manifest_hash: String,
}
```
未来版本可在 `ModuleManifest.abi_contract` 下继续承载等价的 declarations；当前
`ModuleAbiContract.declarations.commands` 已定义并参与 admission，但当前 wire 尚未
定义 component、event 或 migration declarations。这些声明不能替代
`interface_version`、`input_schema`、`output_schema`、`required_caps` 或
`cap_slots`。ABI 版本、对象 schema 版本、manifest hash 和工件 `wasm_hash`
是四个不同的目标身份维度：任一声明、权限、limits 或 metering schedule 的
hash 影响规则，都必须在版本化 ABI 和 canonical hash 规范中先定义，再作为
试点 admission 条件；工件字节变化仍必须产生新的 `wasm_hash`，不得同名覆盖。
命名约束如下：
- `namespace` 是由治理绑定到 module identity 的小写、稳定标识（示例
  `bank.acme`）；`command`、`component`、`event` 是 namespace 内的本地名，
  三者的完整键分别为 `namespace:command`、`namespace:component`、
  `namespace:event`。
- `core.*`、`kernel.*` 和内核保留对象不可由模块注册或覆盖。命名空间不能
  通过上传新工件重新归属；归属变更必须走新的治理提案和 manifest hash。
- 同一完整键的 schema 版本必须显式递增。旧版本仍可用于历史事件和回放；
  未知的未来版本在边界处 fail closed，不能按“最接近字段”猜测解码。
- schema descriptor、声明列表、capability 绑定和 limits 在计算 manifest
  hash 前按完整键和版本排序；模块执行时的访问集合也必须按同一稳定顺序
  处理，不能依赖 map/hash iteration 顺序。
### 2. Command / component / event wire
当前已经落地一个受界限的 module-command 入口，不要求为每一种银行、联盟或合约
操作新增闭合的 Rust `Action` 变体。`ModuleCommandEnvelope` 承载如下 command
字段；runtime 会在进入 sandbox 前校验 active module 的声明、namespace/name、
schema version/hash 和 payload 边界，再复用现有计量调用链。该入口仍只是基础切片；
provider/loopback 已有 response 校验，但尚未证明生产自动接线，也不等于完整的 caller capability、通用
action/router、component/event/migration wire、staged atomic commit 或 execution
receipt 协议。
```text
{
  "namespace": "bank.acme",
  "command": "open_account",
  "schema_version": 1,
  "schema_hash": "<sha256-hex>",
  "payload": <canonical-cbor bytes>
}
```
这里的 **Canonical-CBOR 保证只覆盖 envelope**。`payload` 是 opaque CBOR byte string；
`decode_canonical` 验证外层 map 的 byte-exact 编码，`schema_hash` 只与 active
manifest declaration 相等并检查大小上限。runtime 不递归验证 payload 的字段类型、
范围或内部 canonical 编码，不能把 canonical envelope 写成 canonical payload schema。
目标上，每条 command 必须有可寻址的语义 descriptor，并由内容地址绑定：
```text
CommandSchemaDescriptor = {
  namespace, command, schema_version,
  encoding: "canonical-cbor", request_shape, response_shape?,
  field_constraints, max_payload_bytes,
}
schema_hash = SHA-256(Canonical-CBOR(CommandSchemaDescriptor))
```
descriptor bytes 由受信 schema registry 按 `schema_hash` 提供；manifest declaration
和 catalog 只接受该内容地址，不能由 provider 临时改写。`request_shape` 至少表达
字段名、标量/容器类型、必选性、整数范围和嵌套 schema hash；response/error shape
也必须版本化。排序和 hash 域需在试点前冻结；当前 `ModuleCommandDeclaration` 的
`schema_hash` 仍只是声明级标识，不是 descriptor registry 或语义验证器。
目标上 Agent/provider 只返回 typed args。受信 host 按精确 `schema_hash` 取得
descriptor，校验字段/类型/范围/大小，以唯一 Canonical-CBOR encoder 生成 payload，
再构造并重验 `ModuleCommandEnvelope`；provider 不得决定 caller、target、grant 或
provenance。当前 response 仍携带 raw payload，typed-args encoder 和生产 registry
尚未实现；raw envelope 仅是兼容/试验路径，不能作为语义安全证明。
试点宿主从受信调用边界注入 caller、origin、logical time、trace/journal 位置；模块
不得在 payload 中伪造。当前 `ModuleContext` 已承载 host-injected caller、manifest
和 journal provenance，但没有完整 subject/instance/entity target；action 路由也
没有这套 admission，须在试点前补齐并绑定回放。
目标宿主还必须先确认模块处于 active 状态、完整命中 manifest 声明、`payload`
哈希/大小和 schema 版本有效，再进入 WASM；未知 namespace、command 或版本均
结构化拒绝。
目标 command 请求必须把实例寻址放在受信 envelope/context 外层，而不是由 payload 自报：
```text
ModuleCommandTarget = {
  module_id, module_version, instance_id, entity_id?,
}
ModuleInvocationProvenance = {
  caller, origin, target: ModuleCommandTarget, manifest_hash, wasm_hash,
  journal_position?,
}
```
catalog entry、capability scope、nonce 和 execution receipt 必须绑定同一精确 target；
singleton 也必须使用稳定 `instance_id`，不能退化为 module-global 状态。当前
lifecycle/tick/state 更新路径已有 instance-scoped 证据，但 direct/trusted command
的 state lookup、catalog target 和部分 provenance 仍按 `module_id`；在 target 字段
进入 admission、snapshot/replay 和 receipt 前，不能宣称 instance-complete。
未来扩展可采用逻辑 component 记录，但这不是当前 `WorldState` 已有的 component
存储，也不是本期承诺的完整 ECS 或 `WorldState` 重写。稳定 instance/entity
寻址、journal 所有权、snapshot/replay 绑定和读写 admission 都是试点前置条件：
```text
ComponentKey   = { namespace, component, schema_version, entity_id }
ComponentValue = { schema_version, data: canonical-cbor bytes }
```
目标规则是模块只能读写自己声明且被授予 capability 的 namespace；模块局部
singleton 使用稳定的 instance/entity key。核心位置、身份、所有权、时间、资源
守恒和其他 Kernel invariant 不作为可写 component 暴露。需要改变核心状态时，
模块只能产生经过 capability/policy 校验的 `EffectIntent`，由 Kernel 原子应用
或拒绝；这些 scope、原子边界和拒绝语义在当前 ABI 中尚未实现，不能当作现行
安全保证。
目标扩展中，模块发出的事件必须匹配 manifest 的 event declaration，且完整键为
`namespace:event`；目标宿主负责填充 event id、logical time、caused-by 和当前
manifest hash。payload 按声明的 schema 以 Canonical CBOR 校验。当前
`ModuleEmit` 仍是 JSON payload，legacy adapter、原始 bytes 保留、schema/hash
绑定和 replay 规则都尚未定义；它们是试点前置条件，不能宣称当前 event wire
已经满足该目标。目标版本仍不得原地重写旧事件。
### 3. Capability、权限和 Kernel 边界
本小节是目标扩展的权限合同和试点前置条件，不是当前 `CapabilityGrant` 或
`cap_slots` 已经提供的 scope 证明。当前 grant 不能表达下列完整的
namespace/object/entity 作用域；在 ABI、manifest hash 和执行前拒绝路径补齐前，
不能把这些规则作为现行 fail-closed 保证。
- **当前实现边界。** 现行 `CapabilityGrant` 只有全局 `name`、`effect_kinds`
  和可选 `expiry`；`WorldState` 的 capability map 以 grant name 索引，
  `ModuleAbiContract.cap_slots` 只把 slot 映射到这个名称。当前的
  `EffectOrigin`/`ModuleInvocationProvenance` 能记录调用来源类别和模块/Agent
  provenance，但不能证明某个 Agent 对某个 module version、namespace、command
  或 entity 的授权。`module_command_catalog` 也是 active module 声明的只读、
  subject-agnostic 投影；它可以驱动 provider 的 schema 发现，但不是 grant，
  不能仅凭 catalog 或 provider response 执行命令。`allow_all` 只保留为既有
  native effect 的兼容能力，不能成为新 module command 的默认权限。
- **目标声明边界。** command、component read/write、event emit 都必须在
  manifest 中声明所需 `cap_ref`；`cap_slot` 只能解析到 manifest 中唯一且已
  通过治理授予的 capability。模块不能自授予 capability、扩大 entity 范围、
  把 read 提升为 write，或把一个 namespace 的授权转给另一个 namespace。
- **目标最小权限。** capability 是绑定主体、模块身份、对象、操作和资源范围
  的最小治理授予；缺失、过期、冲突、撤销、快照过期或 policy deny，应在进入
  sandbox、计量、journal 或 effect dispatcher 前返回结构化
  `CapabilityDenied`/`PolicyDenied`。拒绝不得应用 state、effect、emit 或部分
  扣费。具体 grant wire、scope canonicalization、签名/receipt 和 replay binding
  必须先版本化，以下定义是目标合同而非当前 wire。
- 目标 host 不暴露真实时间、随机数、网络、文件或任意 I/O。所有外部影响
  必须表达成带 `cap_ref` 的 `EffectIntent`，由宿主 policy 决定是否落账。
- 目标扩展永远不能覆盖 Kernel invariant、绕过资源守恒、跳过 logical time、
  直接移动/写入核心 component，或把制度权限转化为共识/节点权限。制度语义
  仍由其领域治理和试点分册拥有，本合同只定义待实现、可验证的扩展边界。
#### 3.1 目标授权对象：subject、issuer、scope 和 grant identity
下面的结构保留为目标授权语义模型；`CapabilityGrantV2` Rust 类型已在
`oasis7_wasm_abi` 中实现，grant body 使用 Canonical CBOR 编码并以 SHA-256 计算 body hash；
runtime 基于已最终确认的 authority evidence 验证 issuer 与 issuance 的 Ed25519 签名。
类型和这些实现不表示下方每个 target variant 与规则均已支持；当前受支持的 target
及其边界以 runtime 实现和证据为准。
```text
CapabilitySubject =
  Agent { agent_id, owner_binding, generation }
  | Module { module_id, module_version, instance_id }
  | System { system_id, epoch }
CapabilityPresenter = {
  presenter_id,
  presenter_kind,              // provider | agent_client | module | viewer
  session_id?,
  attestation_ref?,
}
CapabilityAudience = {
  world_id,
  branch_id,
  finality_epoch,
  target_kind,                 // world | institution | module_instance
  target_id?,
}
CapabilityIssuer = {
  issuer_id,                 // governed authority, not the calling subject
  issuer_kind,               // governance | kernel_migration | system
  governance_epoch,
  finalized_receipt_id,
  key_id,
  issuer_key_epoch,
  authority_rotation_receipt_id?,
  signature,
}
CapabilityScope = {
  module_id,
  module_version,
  namespace,
  object_kind,                // command | component | event | effect
  object_name,
  operation,                  // execute | read | write | emit | invoke
  entity_selector?,           // exact ids or governed bounded set
  resource_selector?,         // exact resource/type/ledger range
  max_payload_bytes?,
  policy_class?,
}
CapabilityGrantV2 = {
  grant_id,                    // hash of canonical grant body
  grant_version,
  subject,
  audience,
  issuer,
  scope,
  issued_at_tick,
  expires_at_tick?,
  grant_nonce,
  parent_grant_id?,
  delegation_depth,
  revocation_epoch,
  status,
  canonical_body_hash,
  issuance_signature,
}
```
Normative target rules:
1. `subject` answers **who may call** and is not inferred from a payload or from a
   module's self-reported origin. An Agent generation changes on transfer, reset or
   re-claim so that a stale grant cannot follow the identity silently. A Module grant
   binds both immutable `module_id` and exact active `module_version`; an upgrade must
   obtain a new grant unless governance explicitly declares a compatible replacement.
2. `presenter` is the transport or actor presenting a request (for example a provider,
   Agent client, viewer, or module); it is not the `subject` and cannot substitute for
   it. A provider may present an Agent's response only when the host has a separately
   governed binding between presenter and subject. Provider identity, session and
   attestation are audit inputs, never an implicit capability.
3. `audience` binds the grant to one world/institution/module-instance and its branch and
   finality epoch. A grant issued for world A, another institution, or an orphaned branch
   cannot be replayed in world B or a new branch. Audience is part of the scope and of
   every discovery, nonce and receipt hash.
4. `issuer` answers **who was trusted to issue** the grant. Only a finalized governance
   receipt, a narrowly scoped Kernel migration authority, or an explicitly registered
   system authority can issue a root grant. The calling Agent, module, provider, viewer,
   local catalog, pending proposal, or unfinalized block cannot act as issuer. The
   issuer key, `issuer_key_epoch`, governance epoch and authority-rotation receipt are
   part of the signed grant body; a key outside the current trusted epoch fails closed.
   Authority rotation must itself be finalized and cannot silently extend grants issued
   under an older epoch.
5. `scope` is conjunctive, not additive: module identity, namespace, object, operation,
   and every supplied entity/resource selector must match. Omitted selectors mean
   **no authority** for a target grant unless a governed scope policy explicitly
   defines a bounded default; they never mean unrestricted access. Wildcards are
   disallowed for new grants except for a named, audited legacy compatibility class.
6. `grant_id` is the stable audit/replay identity. `grant_nonce` is unique per issuer
   and subject and prevents replay of an issuance or delegation request. A new scope,
   subject generation, expiry, issuer epoch, or parent creates a new grant id; grants
   are immutable and are superseded/revoked rather than edited in place. The canonical
   grant body is deterministically encoded, hashed, and signed by the trusted issuer;
   a database row, provider response, or local grant name is not an issuance proof.
7. `expires_at_tick` uses logical world time and is inclusive only through the exact
   declared boundary. Wall-clock time is never an authorization input. A grant that
   lacks expiry must be explicitly marked non-expiring by governance and remains
   revocable; absence of the field is not an accidental infinite lifetime.
The target issuance receipt for a root or delegated grant must include the canonical
body hash, grant id, subject, presenter (when one initiated the request), audience,
issuer id/key id/key epoch, authority rotation receipt, scope hash, grant/parent nonce,
governance proposal or policy id, `finality_status`, `finality_block_hash` when verified,
branch id, finality epoch, issued/expiry ticks, revocation epoch, and the signed bytes
hash. High-impact scopes (assets, ownership, governance, irreversible institution
changes, or cross-world effects) require a finalized governance certificate and a
non-empty finality block hash; a pending or merely locally observed signer result is
never sufficient. Lower-impact registered system issuance still requires the same
canonical signed body and trusted issuer/key epoch.
The two receipt surfaces are distinct but both are target contracts:
```text
CapabilityAuthorizationRootReceipt = {
  receipt_id,
  grant_id,
  canonical_grant_hash,
  subject,
  presenter?,
  audience,
  issuer_id,
  issuer_key_id,
  issuer_key_epoch,
  authority_rotation_receipt_id?,
  scope_hash,
  grant_nonce,
  parent_grant_id?,
  governance_proposal_id?,
  finality_status,              // pending | verified | reorged | suspended
  finality_block_hash?,
  branch_id,
  finality_epoch,
  revocation_epoch,
  issued_at_tick,
  expires_at_tick?,
  signed_receipt_hash,
}
CapabilityAuthorizationAuditReceipt = {
  receipt_id,
  root_receipt_id?,
  grant_id?,
  subject,
  presenter?,
  audience,
  scope_hash,
  module_id?,
  module_version?,
  manifest_hash?,
  catalog_snapshot_id?,
  response_nonce?,
  authorization_nonce_key_hash?,
  decision,                     // accepted | denied | idempotent | pending
  denial_code?,
  budget_before,
  budget_after,
  world_head_before,
  world_head_after?,
  branch_id,
  finality_epoch,
  finality_block_hash?,
  finality_status,
  state_hash_before,
  state_hash_after?,
  committed_effect_receipt_id?,
  canonical_request_hash,
  canonical_result_hash,
}
```
`CapabilityAuthorizationRootReceipt` proves how a grant entered the authorization
registry; `CapabilityAuthorizationAuditReceipt` proves how one presented request was
checked and, if accepted, atomically committed. Neither receipt is itself a new grant:
the runtime still rechecks the live grant, revocation epoch, branch/finality state and
nonce record for every new request. `budget_before`/`budget_after` are deterministic
world-resource values (including zero where the operation has no budget), never
wall-clock or provider-local estimates.
#### 3.2 Delegation attenuation and revocation
Delegation is an opt-in governance capability, not an automatic property of every
grant. A delegated grant must carry `parent_grant_id` and satisfy all of these
constraints:
- the parent is active, non-expired, non-revoked, and explicitly `delegable`;
- child subject, module identity, namespace, operation, entity/resource selector and
  policy class are a subset of the parent's scope; `read` cannot become `write`, and
  a bounded selector cannot become a wildcard;
- child expiry is no later than the parent expiry, delegation depth strictly decreases,
  and the child nonce is fresh under the issuer/parent pair;
- the child issuer is authorized by the parent grant's delegation rule and cannot
  change the governance epoch, issuer trust root, or grant's audit policy;
- revoking a parent immediately invalidates all descendants, while revoking a child
  does not rewrite or revoke its parent. A delegation chain is replayed and checked in
  canonical parent-to-child order.
The target revocation registry is versioned by `revocation_epoch`:
```text
CapabilityRevocationState = {
  epoch,
  revoked_grant_ids: sorted set<grant_id>,
  superseded_by: map<grant_id, grant_id>,
  trusted_issuer_key_epochs: map<issuer_id, key_epoch>,
  finalized_receipt_id,
}
```
An issuance, delegation, expiry transition, parent revocation, child revocation or
trust-root rotation is an auditable governance event. Revocation takes effect at its
finalized logical tick; a command already committed before that boundary remains a
historical result, but an uncommitted request or a retry must be checked against the
new epoch. If the runtime cannot prove that its revocation state is at least the grant's
required epoch, authorization is denied; stale caches never extend authority. A trust
root/key rotation likewise rejects grants whose issuer key epoch is no longer trusted,
unless a finalized rotation policy explicitly preserves a bounded historical read-only
use. It never preserves new writes or high-impact effects by default.
#### 3.3 Dynamic Agent catalog and provider response binding
The Agent/provider surface is a two-step contract. Discovery is an observation;
execution is a fresh authorization decision. **Current/partial:** snapshot/response
types, subject binding, provider/loopback validation and selected-entry mismatch
rejection exist; automatic production context injection and real Agent E2E are not
proven. **Target:** host catalog carries trusted descriptor and exact instance target,
and provider returns typed args for host encoding. Discovery never becomes authority.
```text
CapabilityCatalogSnapshot = {
  snapshot_id,                 // hash of canonical snapshot
  world_id,
  world_head,
  branch_id,
  finality_epoch, finality_block_hash,
  logical_tick,
  module_registry_hash,
  policy_hash, policy_epoch,
  revocation_epoch,
  subject, presenter, audience,
  entries: [
    { module_id, module_version, namespace, command, schema_version,
      schema_hash, max_payload_bytes, instance_target,
      eligible_grant_ids }
  ],
  valid_until_tick,
}
AgentCommandResponse = {
  response_nonce,
  subject, presenter, audience,
  catalog_snapshot_id,
  selected_entry,              // exact module/version/instance/namespace/command/schema
  typed_args,                  // target: host validates against schema_hash
  envelope?,                   // current compatibility lane: raw envelope
  provider_id?,
  trace_id,
}
```
`instance_target` is mandatory for a module-instance audience and carries stable `(world_id,
module_id, instance_id)`; a governed non-instance command uses an explicit variant, never global
fallback. Snapshot hash and bound nonce/context/receipt cover world, branch, finality block/epoch,
policy hash/epoch, revocation epoch, subject/audience, entries and validity.
Catalog entries intersect active declarations, the subject's grants and audience policy.
Verified revoked/superseded grants are excluded without hiding unrelated valid grants;
history remains persisted. Corrupt signature/authority/proof or unexplained conflicts
still fail closed; see [discovery validation](../module/module-lifecycle.md#revoked-grant-discovery).
Sorted entries bind `module_registry_hash`, policy hash, branch, finality and revocation
epochs. `eligible_grant_ids` is an audit hint, not a bearer token or new authority.
Providers may derive dynamic schemas and propose a response, but cannot add commands,
change subject/audience, substitute grants or issue scope. Selected entry, envelope
schema, subject, presenter binding, audience, snapshot id and response nonce must all
match the request; execution independently rechecks live authority below.
The current response may carry a raw `ModuleCommandEnvelope`; the trusted host treats it
as compatibility input and re-runs declaration/hash/size checks. Once `typed_args` exists,
only the host encoder produces payload bytes. Provider cannot choose another descriptor,
add undeclared fields, widen constraints, or use another instance. Missing descriptor,
stale registry, ambiguous field or encoding error is a structured denial; no JSON/CBOR
guessing fallback exists.
At execution time the host must re-read live state and validate, in this order:
1. canonical response/envelope encoding, fresh response nonce, subject generation,
   presenter-to-subject binding and exact audience;
2. catalog snapshot existence, `world_head`/policy hash compatibility and
   `valid_until_tick`; branch/finality status must be verified. A policy may permit a
   bounded head drift only when that rule is explicit and included in the signed
   snapshot; it may not permit an unverified branch or reorged finality evidence;
3. active module record, exact module/version, declaration, schema hash and payload
   limit;
4. issuer trust, grant signature, parent chain, scope intersection, expiry and current
   revocation epoch/list;
5. current policy, entity/resource ownership and one-use/replay nonce state;
6. only then enter metering, WASM sandbox, staged state/effect/event application and
   receipt generation.
Any mismatch returns a structured denial with the earliest failing reason. It must not
fall back to a static Action, a legacy global grant, the provider's claimed schema, or
the stale catalog. This preserves existing closed `ActionCatalogEntry` tools while
allowing a new dynamic module-command tool lane to be added incrementally.
The persistent authorization nonce key is the complete tuple below; a process-local
nonce cache is not an authority:
```text
AuthorizationNonceKey = {
  subject,
  issuer_id,
  issuer_key_epoch,
  grant_id,
  audience,
  branch_id,
  finality_epoch,
  nonce,
}
AuthorizationNonceRecord = {
  key,
  request_hash,
  outcome_hash,
  committed_receipt_id?,
  state,                         // reserved | committed | aborted
}
```
For one key, the same canonical `request_hash` is idempotent and returns the original
receipt (or the same pending status); a different hash is a deterministic
`NonceConflict` and cannot execute a second request. The nonce record, budget
reservation/debit, module state, world effects, emitted events and authorization
receipt must commit in one staged transaction. A failed or aborted transaction rolls
back all of them, including the nonce reservation and budget; a committed retry reads
the durable receipt instead of re-running WASM. Recovery resolves `reserved` records
from the journal before accepting new work.
#### 3.4 Governance, finality, persistence, replay and audit
- **Issuance/finality:** governance creates `CapabilityGrantIssued` or
  `CapabilityGrantDelegated` only after the applicable proposal, signer/quorum and
  finality receipt are valid, and the canonical grant body has a valid trusted issuer
  signature. A pending governance action can be displayed as a preview but cannot enter
  the active authorization registry. During a network partition, an unverified branch,
  or a reorg before finality, the grant is `pending`/`unverified` and may only be shown
  as a non-authorizing preview; it cannot be used for a command, budget debit or effect.
  If its finality block is orphaned, the grant becomes `reorged`/`suspended` and requires
  re-issuance or a fresh finalized proof on the canonical branch. Consensus finality
  confirms the governance event; it does not turn a module command into a consensus
  primitive or grant node/validator authority.
- **Persistence:** snapshots must include the active immutable grant records, grant
  parent links, issuer trust-root epoch, revocation epoch/list, supersession map and
  consumed issuance/delegation/execution nonces and durable budget state. The snapshot
  also records the `module_registry_hash`, policy hash, branch/finality epoch and the
  last finalized governance receipt and finality block hash used to construct its
  authorization view. A catalog snapshot may be retained as an audit object, but it is
  never the sole source of authority.
- **Replay:** replay reconstructs the grant registry and revocation state from
  governance events up to the historical world head, then validates each command
  against that historical state. It never consults today's grant list or current
  revocation epoch to reinterpret a committed historical receipt. A response/catalog
  hash is replay evidence, not a grant; an uncommitted response is discarded after
  recovery and must be re-discovered. A branch or finality proof that was pending,
  unverified, or orphaned is not promoted during replay. Historical committed receipts
  retain their original branch/finality block hash and status; they are not silently
  replayed as new canonical authority.
- **Audit events:** the target event set is
  `CapabilityGrantIssued`, `CapabilityGrantDelegated`, `CapabilityGrantRevoked`,
  `CapabilityTrustRootRotated`, `CapabilityCatalogPublished`,
  `CapabilityAuthorizationChecked`, `CapabilityAuthorizationDenied` and
  `ModuleCommandSubmitted`. Each event binds `grant_id` (when applicable), subject,
  presenter, audience/world/branch, issuer/key epoch, scope hash, catalog snapshot id,
  response nonce, nonce-key hash, module/manifest hash, logical tick, world head,
  finality epoch/block hash/status, budget before/after, and caused-by/trace id.
  The authorization/audit receipt additionally records the canonical grant/response
  hashes, decision, denial code or committed receipt id, and the state/effect/charge
  result. Denial events contain a structured reason and no payload secrets. An audit
  append is transaction-neutral; failure to append it cannot turn a denied request into
  an accepted one.
- **Fail closed:** unknown issuer, malformed signature, missing or stale revocation
  state, expired/revoked/superseded grant, invalid parent chain, subject mismatch,
  presenter mismatch, audience mismatch, scope widening, stale snapshot, nonce reuse
  or hash conflict, inactive module, schema/hash mismatch, unverified/reorged finality,
  unavailable policy, exceeded limit/budget or unavailable audit/journal authority all
  deny before any world mutation. The only permitted observable result is a bounded,
  structured denial/audit record; retries require a new response nonce and fresh
  discovery.
#### 3.5 Forbidden authorization shortcuts
The following are explicitly forbidden in the target implementation and are not
acceptable substitutes during migration:
- `CapabilityGrant::allow_all`, `World::add_capability` or any global
  grant-map mutation may not mint, widen, or refresh an actor-scoped v2 grant. These
  APIs remain legacy native-effect compatibility only until their declared migration
  cutoff.
- `EffectOrigin`, `ModuleInvocationProvenance`, a provider attestation, process identity,
  session cookie or caller-supplied `subject` is provenance, not proof of issuer,
  grant, scope, audience, finality or revocation status. Host provenance must be
  cross-checked against the persistent authorization registry.
- Process-local catalog, grant, revocation, budget or nonce caches may accelerate a
  lookup only when their digest and epoch are checked against durable world state. They
  cannot authorize during a partition, after expiry/revocation, or when the durable
  source is unavailable; stale/unknown cache state denies.
- A provider response, `ActionCatalogEntry`, static fallback action, receipt signature,
  consensus block, or module manifest alone cannot authorize a high-impact effect.
  Each still requires the live subject/issuer/scope/audience/nonce and finality checks.
- No implementation may reserve a nonce, debit budget, or append an authorization
  receipt separately from the staged world effect. Partial success is a denial/recovery
  case, not an accepted command.
#### 3.6 Migration from the current global `CapabilityGrant`
Migration must be explicit and one-way for new authority. A migration adapter may read
the current global map during a bounded compatibility window, but it must not silently
upgrade a broad grant into an actor-scoped grant:
1. For each legacy grant, governance emits a `CapabilityGrantMigrated` event containing
   the old grant name/hash, a synthetic `kernel_migration` issuer, the migration epoch,
   and an immutable `LegacyGlobal` subject. The old `effect_kinds` become exact legacy
   effect scopes; `expiry` maps to logical `expires_at_tick`.
2. `cap_slots` are resolved to the migrated grant id for existing native effect paths.
   A legacy `*`/`allow_all` grant remains restricted to the pre-existing native effect
   compatibility class. It cannot satisfy a new module command, component, event or
   Agent/provider request.
3. New module declarations must reference `CapabilityGrantV2` ids and exact scopes.
   An Agent or module that still presents only a legacy name receives a structured
   migration-required denial; there is no implicit subject or issuer inference.
4. Snapshot/replay supports both formats only until the declared migration cutoff.
   Before cutoff, old events remain readable through the adapter and new issuance is
   dual-recorded with the v2 grant id; at cutoff, the active legacy map is frozen and
   all remaining authority requires finalized v2 issuance. Historical legacy receipts
   remain queryable and are not rewritten.
5. Migration acceptance requires proving that no migrated grant gains a wider effect,
   namespace, entity, resource, expiry or delegation scope than its source grant, and
   that a revoke/expiry in either representation cannot leave a live alias.
#### 3.7 Authorization acceptance criteria
The following IDs are testable gates for an authorization implementation; they are
target acceptance criteria, not current test claims:
| ID | Acceptance criterion | Required tier |
| --- | --- | --- |
| AUTHZ-01 | A valid Agent/Module/System subject and finalized trusted issuer produce a stable grant id; tampered issuer, signature, nonce or grant body is rejected. | `test_tier_required` |
| AUTHZ-02 | Scope matching is conjunctive and exact across module id/version, namespace, object, operation and entity/resource selectors; omitted or widened selectors fail. | `test_tier_required` |
| AUTHZ-03 | Parent delegation can only attenuate scope, expiry and depth; parent or child revocation invalidates the correct descendants without mutating historical receipts. | `test_tier_required` |
| AUTHZ-04 | Expiry, revocation epoch/list, trust-root rotation and stale registry state fail closed at execution time. | `test_tier_required` |
| AUTHZ-05 | Dynamic catalog entries are filtered by the current subject and bind module registry, policy and revocation hashes; catalog exposure alone grants no authority. | `test_tier_required` |
| AUTHZ-06 | Provider responses must bind subject, exact selected schema, catalog snapshot and fresh nonce; forged/unknown/stale responses are rejected before sandbox or journal mutation. | `test_tier_required` |
| AUTHZ-07 | A live recheck catches grant revocation, module deactivation, policy change and entity/resource ownership change between discovery and execution. | `test_tier_required` |
| AUTHZ-08 | Governance issuance requires finalized authority; pending proposals and consensus receipts without governance grant semantics cannot authorize module effects. | `test_tier_required` |
| AUTHZ-09 | Snapshot/restart/replay preserves grant, parent, revocation, trust-root and nonce state and reproduces the same accept/deny decision and hashes at the same historical head. | `test_tier_full` |
| AUTHZ-10 | Legacy migration is scope-preserving; `allow_all` remains native-only, new module commands require v2 grants, and the cutoff rejects unmigrated aliases. | `test_tier_full` |
| AUTHZ-11 | Every issuance, delegation, revocation, catalog, check, denial and command submission has deterministic audit bindings without leaking payload secrets. | `test_tier_full` |
| AUTHZ-12 | Any missing policy, issuer, revocation, journal or audit authority fails closed with no partial state/effect/charge; accepted commands enter the same staged transaction and receipt path. | `test_tier_full` |
| AUTHZ-13 | Presenter/provider, subject and audience/world are distinct; a provider cannot substitute identity, audience, grant or scope, and cross-world/branch replay is rejected. | `test_tier_required` |
| AUTHZ-14 | Issuance is deterministic canonical signing bound to issuer key epoch and finalized authority rotation; high-impact grants require a verified finality block hash. | `test_tier_required` |
| AUTHZ-15 | The persistent nonce tuple `(subject, issuer/key_epoch, grant, audience, branch/finality epoch, nonce)` is idempotent for the same request hash and rejects a different hash without mutation. | `test_tier_required` |
| AUTHZ-16 | Nonce reservation, budget before/after, world effects, module state, events and authorization receipt commit atomically; crash/recovery resolves reserved records deterministically. | `test_tier_full` |
| AUTHZ-17 | Partition, pending/unverified finality and reorg/orphaned branch evidence are preview-only or suspended; no unverified grant can authorize a command or high-impact effect. | `test_tier_full` |
| AUTHZ-18 | `allow_all`, `add_capability`, provenance, process-local caches, provider responses and consensus receipts cannot bypass live v2 authorization. | `test_tier_required` |
### 4. 确定性计量和执行收据
执行目标上必须同时受 WASM fuel/`ModuleLimits.max_gas` 和资源计量 schedule
约束。本节的 schedule identity、扩展访问计量和 execution receipt 都是试点
前置条件，不是当前 `wasm-1` wire/journal 已有的 receipt contract。
当前 executor 基线在其既有计费路径中使用如下确定性费用口径；这段公式不等于
开放 component/schema 扩展已经获得同一 schedule，也不为未来访问费用冻结 ABI：
```text
compute = ceil(input_bytes / 1024)
        + ceil(output_bytes / 1024)
        + 2 * effect_count
        + emit_count
electricity = 1 + effect_count + emit_count + has_new_state
```
若试点增加 component read/write 或 schema decode 费用，必须先定义新的、明确
命名的 `metering_schedule_version`，并将其加入版本化 manifest/receipt/journal
合同；当前代码没有该字段。费用只能由 canonical 字节长度、声明的访问数量、
WASM fuel、effect/emit 数等确定性输入计算，不得使用 wall-clock 耗时或未排序
的集合。目标 execution receipt 至少绑定 module/instance、manifest hash、
`wasm_hash`、schedule version、输入/输出大小、fuel/费用、effect/emit 数、
状态前后 hash 和 journal position；该 receipt schema、事件承载和回放规则必须
在试点实现前补齐。
目标上，余额或 fuel 不足必须在提交前返回结构化拒绝；成功调用才产生计费和
状态/事件变更。回放应消费已记录的执行结果与计量收据，不重新按当前价格或
当前 schedule 计算历史费用；新 schedule 只影响新 manifest/新执行。当前
`ModuleRuntimeChargeEvent` 尚未承载这些 identity/receipt commitments，故不能
把上述收据语义报告为已实现。
### 5. 安装、升级、禁用和迁移
开放对象的 install/register、activate、upgrade、deactivate **目标上**必须与
manifest、artifact identity 和 `ModuleChangeSet` 同一治理闭环绑定。以下条目
是试点前置条件；当前 lifecycle wire 没有完整的 migration identity 或 atomic
batch 语义，不能把本节当作现有操作的保证：
- 目标 install 只接受已计算并重新校验的 SHA-256 `wasm_hash`，以及发布链提供的
  artifact identity/build receipt；本地任意源码编译或上传不会自动激活模块。发布
  链字段、验证顺序和错误事件必须与现有 release authority 对齐。
- 目标 upgrade 必须引用当前 `base_manifest_hash` 和 active version。保持同一
  ABI/schema 的兼容升级可复用已有 state；若 component/event schema hash
  改变，manifest 必须声明对应对象类型的确定性 `SchemaMigration`，否则拒绝
  升级。也可以注册一个新的完整 event key，让旧 event 保持原 schema；这些
  from/to/hash/migration 绑定当前尚未进入 `ModuleUpgrade` wire。
- 目标 disable 在逻辑 tick 边界停止新调用，但保留 module state、旧 manifest、
  artifact hash 和完整事件历史；禁用不是删除，也不能让回放选择另一版本。旧
  版本保留和 replay 选择规则必须先有 lifecycle event/snapshot 证据。
- 目标 apply 失败是原子的：不写半套 register/activate/upgrade/deactivate 事件，
  不更新 active registry，不覆盖旧 artifact/state。实现前必须提供 journal
  transaction 或等价 staged commit、failure injection 和 replay fixtures；不能
  从 `module-lifecycle.md` 的现有事件排序直接推导出该原子性。
目标迁移声明至少绑定对象类型、`namespace/name`、from/to schema version/hash、
迁移工件 `wasm_hash` 和迁移 manifest hash。component 迁移函数必须是确定性、
受限、无外部 I/O 的纯 state 转换，只能产生目标 namespace 的 state；event
迁移只能把历史 payload 转成已声明的 reducer 输入，不能伪造 caller、time、
caused-by 或 Kernel event。以下是目标 event shape，不是当前 `WorldEvent` 的
已实现类型；提交前必须定义其 wire、原子写入和 replay 绑定：
```text
ModuleStateMigrated { // target shape; not a current event type
  instance_id, entity_id, namespace, component,
  from_schema_version, to_schema_version,
  from_state_hash, to_state_hash,
  migration_wasm_hash, manifest_hash, caused_by,
}
```
目标回放先按旧 schema 重放历史事件，到迁移边界应用 state/event 的声明迁移，
再按新 schema 继续；历史 event/state bytes 不重写。没有 event migration 时，
旧 event 必须按其原版本交给仍声明支持它的 reducer，不能静默按新 schema
重新解释。迁移失败、hash 不匹配或输出超限时应保留旧版本并结构化拒绝。
这些规则和 manifest 自身的版本迁移（包括 `ManifestMigrated` 语义）都必须在
试点前与现有 lifecycle authority 对齐并提供 from/to hash 证据，不能由本节
单方面冻结。
### 5.1 兼容性、fail-closed 与迁移证明
兼容性按 wire surface 和证据等级记录；“有默认值”不能代替迁移证明：
| Surface | 当前兼容行为 | 目标证明 |
| --- | --- | --- |
| `ModuleManifest` / `ModuleAbiContract` | 缺省 ABI contract、declarations 与可选 artifact identity 可读取历史 manifest；未知字段不能改变既有 hash/admission 语义。 | 用旧 manifest fixture 做 decode、canonical re-encode、admission 和 snapshot/replay，证明 `module_id`、version、`wasm_hash` 与 manifest hash 不漂移。 |
| `ModuleContext` / `ModuleOutput` | 缺失 host caller 与可选 `tick_lifecycle` 的旧 wire 仍可解码；legacy caller 不能被当作新 subject 授权，tick 缺省按 suspend 处理。 | 旧模块 golden bytes 在新 host 上得到同一结果；新字段只在明确的 ABI version/manifest admission 下启用。 |
| `ModuleCommandEnvelope` | 外层 Canonical-CBOR、声明 identity 和 payload bound 有 proven 检查；payload 内容仍是 opaque，raw envelope 是兼容路径。 | descriptor registry、typed-args host encoder、semantic decode 与 schema hash 绑定必须有正反 fixture；未知 schema/version/encoding 一律拒绝。 |
| module lifecycle / state | install、activate、upgrade、deactivate 与 instance/tick snapshot persistence 已有切片；成功 tick suffix replay 依 [宿主合同](../runtime/runtime-integration.md#module-tick-journal-replay)，完整 migration atomicity 与 receipt 仍 partial/target。 | 从 `base_manifest_hash` 到 from/to schema、migration `wasm_hash`、新 manifest hash 的单 journal 证明；失败注入后旧 registry/state/artifact 完整保留；tick 另验非空 suffix 与续跑。 |
下列身份不可互换：`wasm_hash` 是工件 bytes 内容地址，`manifest_hash` 是 manifest 控制面 hash；当前 `schema_hash` 只是 declaration marker，target registry 落地后才是 descriptor bytes 内容地址；`artifact_identity` 绑定
source/build manifest/signer/signature。任一 identity、ABI version、权限、limits 或
metering schedule 变化都必须重新 admit；不能同名覆盖、只改版本字符串或依赖 provider
声明维持兼容。
迁移证明最小闭环是：保留旧 bytes；新 bytes 由显式 from/to schema 和受治理 migration
artifact 产生；migration 输出、state/event 前后 hash、manifest hash、instance target
和 journal position 可回放复算。任一 hash、descriptor、权限、artifact、实例或
finality 不可证明时，在 sandbox、计量、journal/effect commit 前结构化拒绝；历史
event/state 不原地改写，失败不得部分扣费、写 state、发 emit 或覆盖 active module。
### 6. 试点验收与非目标
一个 Alliance 或 EconomicContract 试点在进入实现前，至少需要完成下列
前置条件，并用代码、ABI fixtures、journal/replay evidence 证明；这些条目是
目标合同的 admission gate，不是对当前 runtime 已支持能力的陈述：
1. 一个 namespace 能声明 command/component/event schema，并以 canonical
   envelope 被发现、校验、调用和拒绝未知版本。
2. capability 不足、Kernel component 写入、fuel/limits 超限、余额不足和
   artifact/manifest hash 不一致均 fail closed，且无部分 state/effect/event。
3. register/activate/upgrade/deactivate、state migration、snapshot/replay
   在同一 journal 上得到一致 state/event/hash/计量收据。
4. 既有 native vertical slice、旧 `wasm-1` manifest 和未参与试点的模块行为
   保持兼容；若试点接收既有 `ModuleEmit`，必须先提供版本化 legacy JSON
   adapter，明确原始 bytes、schema/hash 和 replay 处理，不能以“兼容槽位”代替。
本试点不承诺全面 ECS、删除 `tick_consensus`、把所有制度迁入 WASM、动态
  world database、分片，或接受任意未治理上传；这些仍需独立的 runtime、
  consensus、产品和安全评审。
## 关键数据结构（草案）
- `WorldEvent`：`{ id, time, kind, payload, caused_by }`
- `EffectIntent`：`{ intent_id, kind, params, cap_ref, origin }`
- `EffectReceipt`：`{ intent_id, status, payload, cost?, timestamps, hash }`
- `CapabilityGrant`：`{ name, cap_type, params, expiry? }`
- `PolicyRule`：`{ when, decision }`
- `Manifest`：`{ reducers, modules, module_changes?, effects, caps, policies, routing, defaults }`
- `ManifestPatch`：`{ base_manifest_hash, ops[], new_version? }`，支持 set/remove（merge 要求基于同一 base hash）
- `PatchMergeResult`：`{ patch, conflicts[] }`，冲突包含路径与涉及的 patch 索引
- `PatchConflict`：`{ path, kind, patches[], ops[] }`（kind: same_path/prefix_overlap）
- `Proposal`：`{ id, author, base_manifest_hash, manifest, status }`
- `GovernanceEvent`：`Proposed/ShadowReport/Approved/Applied`
- `RollbackEvent`：`{ snapshot_hash, snapshot_journal_len, prior_journal_len, reason }`
- `SnapshotCatalog`：`{ records[], retention }`
- `SnapshotRetentionPolicy`：`{ max_snapshots }`
- `AuditFilter`：`{ kinds?, from_time?, to_time?, from_event_id?, to_event_id?, caused_by? }`
