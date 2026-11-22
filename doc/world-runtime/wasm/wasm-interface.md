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
仅 `committed_receipt_count`、`effective_world_effect_count` 的 JSON integer enum 0/1 是明确例外；schema_version 使用类型名 literal，不以 commit OID 代替 schema revision。

目标 `H(domain,body)=SHA256(UTF8(domain)||0x00||CanonicalCBOR(body))`。CBOR 使用 definite lengths、唯一 canonical map keys、shortest integers、无 float、semantic arrays 保序、sets 按完整稳定键排序且唯一。ContentRef locator、自身 ID/signature、本地 timestamp 与递归依赖 containing-block/finality wrapper 均不进入 core。外层 JSON 不是 CBOR hash 替身。

精确域为 `oasis7/execution-result/v1`、`oasis7/module-execution-receipt/v1`、`oasis7/intent-request/v1`、`oasis7/intent-decision/v1`、`oasis7/external-effect-receipt/v1`、`oasis7/version-activation/v1`、`oasis7/module-state-migrated/v1`、`oasis7/world-execution-state-sync-attachment/v1`。现行 `receipt_leaf_hash` 与 modsig domains 保持原义，不是这些域的 alias。
<a id="sr2-execution-contract"></a>
### 6.2 有效执行 core / finality wrapper
以下是 target/unimplemented 精确记录，各处 schema_version 均为对应类型名 literal：

```text
ExecutionResultCoreV1 = { schema_version:"ExecutionResultCoreV1", scope:WorldScopeV1, intent_id:Id, request_hash:Hash32, execution_version:Id, governing_manifest_hash:Hash32, parent_state_root:Hash32, ordered_input_hash:Hash32, activation_id:Hash32 } ExecutionIdentityV1 = { schema_version:"ExecutionIdentityV1", execution_id:Hash32, execution_core:ExecutionResultCoreV1, canonical_inclusion:{canonical_block_hash:Hash32,canonical_block_height:U64,proof:ContentRefV1} } ModuleExecutionReceiptCoreV1 = { schema_version:"ModuleExecutionReceiptCoreV1",execution_id:Hash32, execution_core:ExecutionResultCoreV1, module_identity:{module_id:Id,module_version:Id,instance_id:Id,interface_version:Id,wasm_hash:Hash32}, metering_schedule_version:Id,metering_schedule_hash:Hash32, input_hash:Hash32,output_hash:Hash32,input_bytes:U64,output_bytes:U64, fuel_limit:U64,fuel_used:U64,effect_count:U64,emit_count:U64, access_counts:{component_reads:U64,component_writes:U64,schema_decodes:U64}, state_before_hash:Hash32,state_after_hash:Hash32, ordered_effects_hash:Hash32,ordered_emits_hash:Hash32, charges:[{resource_type:Id,amount:ResourceAmount,balance_before:ResourceAmount,balance_after:ResourceAmount}], journal_start:JournalPositionV1,journal_end:JournalPositionV1,status:"committed_success" } ModuleExecutionReceiptV1 = { schema_version:"ModuleExecutionReceiptV1",receipt_id:Hash32, receipt_core:ModuleExecutionReceiptCoreV1, finality_binding:{canonical_block_hash:Hash32,canonical_block_height:U64,proof:ContentRefV1} }

```
先计算 execution_id=H(execution-result,execution_core)，验证其一致后计算 receipt_id=H(module-execution-receipt,receipt_core)。scope 仅在 execution_core；wrapper 继承，不能再复制。receipt_core 不嵌入 ExecutionIdentityV1 finalized wrapper。proof 必须绑定两 ID、同一 canonical block/version/activation；不同 containing-block wrapper 不改变 core ID，伪造或矛盾 inclusion 拒绝。fuel_used<=fuel_limit；journal_start<=journal_end；charges 按 resource_type 排序唯一且 after=before-amount，无负余额和溢出。

只有 `committed_success` receipt 是有效执行。拒绝/过期/重规划/无效果终止由 [IntentDecisionV1](../module/module-lifecycle.md#sr2-pending-contract) 拥有；它们不产生 module success receipt、不夺 lineage winner。pending/外部 ACK 不算 committed；非 serviceable 或 manifest/head/finality 缺失的新 intent 必须零 receipt、零效果、零 charge/state publication，历史 receipt 仍保留。

完整执行/prepare/publish/失败与 proof checks 由 [host publication](wasm-executor.design.md#sr2-host-publication) 接收。

Decision/Pending hash body 由lifecycle单authority定义：receipt refs 以canonical receipt_core bytes为ContentRef目标，digest=SHA256(CanonicalCBOR(receipt_core))、byte_length为实际core字节数；此digest不是domain-separated receipt_id。接收器重算receipt_id并另外验证detached finalized wrapper，不把包含同一区块的wrapper digest嵌入decision core。
<a id="sr2-metering-contract"></a>
### 6.3 Schedule identity 与确定性计量
ScheduleDescriptorV1 目标形状为 `{schema_version:"ScheduleDescriptorV1",metering_schedule_version:Id,input_kib_bytes:PositiveU64,output_kib_bytes:PositiveU64,compute:{input_kib:ResourceAmount,output_kib:ResourceAmount,effect:ResourceAmount,emit:ResourceAmount},electricity:{call:ResourceAmount,effect:ResourceAmount,emit:ResourceAmount,new_state:ResourceAmount},extension_access:{component_read:ResourceAmount,component_write:ResourceAmount,schema_decode:ResourceAmount}}`。descriptor canonical bytes 的 SHA-256 是 metering_schedule_hash；系数使用相应资源单位，KiB divisors 使用 byte 单位。当前公式映射 divisors=1024、compute coefficients=1/1/2/1、electricity=1/1/1/1；该映射不声称当前代码已有 ScheduleDescriptorV1。

先 bounds-check input/output/fuel/计数，再用 checked `n/1024 + (n%1024 != 0)` 计算 ceil，避免 n+1023 溢出；按 schedule 计算 checked amounts，再验证余额，最后随 staged transaction 提交。

扩展未支持/未使用时 actual counts 为零；支持但 coefficient=0 的访问仍记录实际 counts，不因免费而抹除。扩展费用必须另经 manifest/schedule 治理，不在本文启用。

历史 replay 消费原 schedule/hash/charges，不按当前价重算；timing、unordered collections 和 provider 自报 cost 不参与费用。
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

本节正文已按 #6507 拆分至同目录分册 [`wasm-open-institution-extension-contract.md`](wasm-open-institution-extension-contract.md)（含能力/权限与 Kernel 边界 [`#wasm-capability-permission-kernel-boundary`](wasm-open-institution-extension-contract.md#wasm-capability-permission-kernel-boundary) 及授权验收条款 [`#37-authorization-acceptance-criteria`](wasm-open-institution-extension-contract.md#37-authorization-acceptance-criteria)），不改变既有契约与状态边界。

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
