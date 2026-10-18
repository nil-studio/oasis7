# WASM SDK 兼容与 Wire 设计

- 专业 owner：`wasm_platform_engineer`；状态：active professional authority（目标合同不等于已实现）。
- 内容审读基线：`eng-cc/oasis7`，源提交 `9c41d57b4436f71f0cf9b481043d882cfdc551ed`；内容整理日期：2026-09-26。
- SDK feature/codec 现状核查基线：`eng-cc/oasis7@e659ee8c6707547905e463a29e60ef910c62276a`，2026-10-12；仅静态核对源码与代表消费者，见[当前能力](#sdk-current-capabilities)。其他 SR2 目标与原证据范围保持不变。
- 独立审读与实际实现/测试候选证据由该 task evidence 记录；此处不预报审读通过、runtime capability、发布或 full proof。下方既有条款仍有效；仅显式日期/历史基线条款按其证据范围解释。

## 1. 问题、目标与非目标

本次把既有专业义务承接为可执行的 target contract 与验证设计。成功结果是精确身份、边界、失败和恢复算法能被消费者读取；不改变 ABI/code/经济价格，不宣称目标已落地。下方原有细节、limits、公式、ID 与历史 evidence 原序保留并继续约束其适用范围。

## 2. 上游约束与相关角色

feature组织本身是专业机制，不独立新增玩家要求；历史manifest/wire兼容与确定性边界仍消费 [interface类型与兼容](wasm-interface.md#sr2-types)，不能以专业适用性排除。产品端到端 authority 为 [deterministic world execution](../../product/world-infrastructure/deterministic-world-execution.prd.md) 与 [distributed state availability](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md)；专业细化由 paired PRD、runtime/module 与 P2P 合同拥有。WASM owns codec/artifact/metering，runtime owns staged publication/replay，QA owns same-candidate acceptance，consumer owners own real projection。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [SDK-01](wasm-sdk.prd.md#sr2-domain-case-1) | 输入 旧 manifests/defaults/export与target known/unknown/null codec matrix；断言 old bytes/defaults保持；strict new codec显式失败 | [§6.3 legacy/default/export与target版本](#sdk-legacy) | runtime/QA；[SDK-01外部receiver](wasm-sdk.design.md#sr2-sdk-contract) | 产品只承接本行确定性隔离或历史兼容子范围；interface/runtime负责完整执行、激活、历史replay与finality；本页不代签产品AC |
| [SDK-02](wasm-sdk.prd.md#sr2-domain-case-2) | 区分 core no_std+alloc、当前 std-backed wire 与 explicit std；实际 no_std 消费者触发扩展时验证 no_std+wire；target 缺失 skipped | [§6.2 feature隔离](#sdk-feature-codec-evolution) | runtime/QA；[SDK-02外部receiver](#sr2-sdk-contract) | 产品 N/A；reason: feature依赖隔离及编码Rust返回接口迁移是实现机制；scope: 仅core/wire/std组合与显式错误传播，迁移必须保留另行承接的旧bytes/default/export，不以接口改动证明世界结果 |
| [SDK-03](wasm-sdk.prd.md#sr2-domain-case-3) | malformed bytes、fallback、multibyte/U64 边界；编码演进时同步 API/调用点/模板，显式错误不伪造 empty success | [§6.2 codec错误迁移](#sdk-feature-codec-evolution) | runtime/QA；[SDK-03外部receiver](wasm-interface.md#sr2-types) | 产品 N/A；reason: feature依赖隔离及编码Rust返回接口迁移是实现机制；scope: 仅core/wire/std组合与显式错误传播，迁移必须保留另行承接的旧bytes/default/export，不以接口改动证明世界结果 |
| [PRD-WORLD_RUNTIME-042](wasm-sdk.prd.md#sr2-obligation-prd-world_runtime-042) | &#124; PRD-WORLD_RUNTIME-042 &#124; test_tier_required &#124; SDK tests、可用时 wasm32 check、builtin wire usage scan、required-tier build &#124; SDK ABI、builtin 模块编译与 wire 兼容 &#124; | [§11.2 分层验证](#sdk-feature-codec-validation) | paired专业owner与runtime/QA；具体跨域依赖见本域合同 | 产品 N/A；reason: SDK host/wasm32/消费者构建是专业验证入口组织；scope: 仅测试选择与缺target标记，具体旧wire/lifecycle兼容及产品组合验收不豁免 |
| [SDK rule 1](wasm-sdk.prd.md#sr2-sdk-rule-1) | core 默认 no_std+alloc；当前 wire 启用 std；保留 wire 不隐式 std 的目标，实际消费者触发时验证 feature 隔离 | [§3 当前feature能力](#sdk-current-capabilities) | ABI/runtime owns receipt finality；builtin owner 提供实际消费者 | 产品 N/A；reason: core/wire/std依赖组合是SDK编译支持边界；scope: 仅feature现状与target检查，不排除独立的旧wire兼容义务 |
| [SDK rule 2](wasm-sdk.prd.md#sr2-sdk-rule-2) | `alloc`、`reduce`、`call` 导出，`LifecycleStage`、`WasmModuleLifecycle`、`dispatch_reduce`、`dispatch_call` 与 `export_wasm_module!` 保持稳定；SDK 内部收敛不得静默改变模块种类、生命周期或业务效果。 | [§6.4 lifecycle导出](#sdk-lifecycle) | ABI/runtime owns receipt finality；builtin owns显式fallback | 产品只承接本行确定性隔离或历史兼容子范围；interface/runtime负责完整执行、激活、历史replay与finality；本页不代签产品AC |
| [SDK rule 3](wasm-sdk.prd.md#sr2-sdk-rule-3) | `ModuleCallInput`、`ModuleContext`、`ModuleEffectIntent`、`ModuleEmit`、`ModuleOutput` 及其编码 helper 由 SDK 持有一份 wire 定义。builtin 模块只保留领域结构，不得复制一套并行协议。 | [§6.5 wire单一owner](#sdk-wire-owner) | ABI/runtime owns receipt finality；builtin owns显式fallback | 产品 N/A；reason: 单份wire类型owner消除并行定义属于代码组织机制；scope: 仅SDK类型与builtin领域类型分工，真实字节/default/output_bytes兼容继续由rule4承接 |
| [SDK rule 4](wasm-sdk.prd.md#sr2-sdk-rule-4) | wire 编解码使用与 runtime `wasm-1` 一致的 Canonical CBOR 字段和默认值；输入、输出与 `output_bytes` 口径必须保持兼容。 | [§6.6 CBOR/length](#sdk-cbor) | ABI/runtime owns receipt finality；builtin owns显式fallback | 产品只承接本行确定性隔离或历史兼容子范围；interface/runtime负责完整执行、激活、历史replay与finality；本页不代签产品AC |
| [SDK rule 5](wasm-sdk.prd.md#sr2-sdk-rule-5) | decode 已返回 Result；encode 当前 Vec+expect，显式编码错误为保留目标；失败不得静默变为空输出或 None，迁移同步消费者 | [§6.2 错误传播](#sdk-feature-codec-evolution) | ABI/runtime owns receipt finality；builtin owns显式fallback | 产品 N/A；reason: feature依赖隔离及编码Rust返回接口迁移是实现机制；scope: 仅core/wire/std组合与显式错误传播，迁移必须保留另行承接的旧bytes/default/export，不以接口改动证明世界结果 |
| [SDK rule 6](wasm-sdk.prd.md#sr2-sdk-rule-6) | 新字段或 feature 必须说明默认值、旧模块兼容性和 wasm32 构建影响。破坏性 ABI 变化不能通过 SDK 整理任务夹带进入。 | [§6.7 字段兼容](#sdk-new-fields) | ABI/runtime owns receipt finality；builtin owns显式fallback | 产品只承接本行确定性隔离或历史兼容子范围；interface/runtime负责完整执行、激活、历史replay与finality；本页不代签产品AC |

2026-10-15 在 `f97f2253cdcb163bf984e3c90d59d792fe1c9b19` 再核 SDK、代表builtin和模板：下列feature/API事实仍成立，仅静态核读，不替换原历史运行证据。

## 3. 当前状态、目标状态与差距

<a id="sdk-current-capabilities"></a>
下列 SDK 现状固定于 `e659ee8c6707547905e463a29e60ef910c62276a`，属于源码静态事实，不是编译、运行或发布通过。原 SR2 target 的实现与证据范围仍按原基线解释。

| 对象 | 当前状态 / 基线 | 目标 | 差距与证据边界 |
| --- | --- | --- | --- |
| SDK core | [Cargo default=[]](https://github.com/eng-cc/oasis7/blob/e659ee8c6707547905e463a29e60ef910c62276a/crates/oasis7_wasm_sdk/Cargo.toml#L7-L18)；[cfg_attr 与 alloc](https://github.com/eng-cc/oasis7/blob/e659ee8c6707547905e463a29e60ef910c62276a/crates/oasis7_wasm_sdk/src/lib.rs#L1-L6) 使非 test、未启用 std 的 core 使用 no_std+alloc | 保持默认 core 的可移植生命周期/导出 | 尚未在本轮执行 host 或 wasm32 check；core 支持不能代替 wire 支持 |
| SDK wire feature | Cargo 的 wire 显式包含 std，serde 依赖也未关闭 default features；[m1_rule_move](https://github.com/eng-cc/oasis7/blob/e659ee8c6707547905e463a29e60ef910c62276a/crates/oasis7_builtin_wasm_modules/m1_rule_move/Cargo.toml#L13-L17) 按该组合消费 SDK | 实际 no_std 消费者需要时实现 no_std+alloc+wire，std 保留为显式宿主便利能力 | 当前是 std-backed wire；尚未识别被此差距阻断的实际消费者，不据此保证所有第三方需求 |
| Codec helper | [encode_output](https://github.com/eng-cc/oasis7/blob/e659ee8c6707547905e463a29e60ef910c62276a/crates/oasis7_wasm_sdk/src/lib.rs#L206-L223) 返回 Vec 并 expect；decode_input/decode_action 返回 Result | 编码错误显式返回，调用点明确处理，保持 wire ABI | 固定 ModuleOutput 的序列化断言尚无已知失败输入；接口错误策略不一致不等于已证实 panic bug |
| wasm-1 foundation | 原有代码/契约保留，源commit见身份块 | 兼容consumer及精确target contracts | 原有tests为located，不是本次run |
| receipt / migration / gate | interface仍声明target或partial | `sr2-sdk-contract` 与runtime authority相连 | 代码、full finality/provider/headed证明不在本任务 |
| 文档与schema | 本文目标合同 | 语义/结构/anchors检查 | 文档通过不意味着runtime pass或SC9/DC5闭合 |

## 4. 边界与结构

输入声明和provider response为advisory，host/runtime负责authority/admission；sandbox仅确定性计算。build证明字节来源、SDK证明codec、metrics证明local诊断，各自不能签署world finality。ContentRef必须fetch/hash/length验证，再由相应authority验trust；current caches与local signature不能越过生产许可。

## 5. 关键运行流程

先绑定candidate/world branch/window与exact manifest/artifact/schema；strict decode和limits preflight后验证authority。成功只进入有权的下一阶段，世界效果须canonical commit receipt。缺字段、版本、proof、兼容或head continuity即停止，不隐式fallback或重试另一身份。重试同request hash幂等，changed hash拒绝；恢复重读当前canonical边界，历史receipt按记录版本重放。

## 6. 接口与数据合同

<a id="sr2-sdk-contract"></a>
### 6.1 版本化 consumer codec 合同

SDK 接收端分开 legacy `wasm-1` wire 与 interface 的 [target records](wasm-interface.md#sr2-types)。不把 target receipt/migration 强行加进旧 ModuleContext/ModuleOutput，也不让 SDK 生成 finality。旧 manifest optional fields 与 serde default、tick 缺省 suspend、effect/emit semantic order 均保留。

| 输入 | decode / encode 判定 | compatibility / failure |
| --- | --- | --- |
| 旧 wasm-1、无新增 optional 字段 | 原 wire owner/types/default；bytes round-trip 与 output_bytes 口径不变 | lifecycle exports/dispatch 与 core no_std+alloc 不变；当前 wire 仍依赖 std |
| 新 target records / 已知 type literal | strict field+enum+canonical decoding；U64 decimal JSON checked 转 CBOR；UTF8 byte bound | 验证 payload shape 不代表 proof 信任；runtime 接收器验 finality |
| 未知 target version / enum / null / field | explicit codec Result error；不得 nearest-schema fallback | 未提交 state/effects/charges |
| 非 canonical map / malformed bytes | byte-exact reencode equality，不偷偷 normalize authenticated input | builtin 必须显式选择原有 fallback；不返回伪空 success |
| 缺 wasm32 target | 明确 skipped 或失败 | 禁止将 host std check 当 no_std WASM pass |

目标存储 codec 仅保存 bytes/refs，authority 与 replay ownership 归 runtime；SDK 无持久化 winner/activation 状态。升级先建立旧 fixtures，再分别启用新 consumer，逐项比较 export、defaults、wire byte lengths 和 domain IDs；回滚到当前 wasm-1 consumer，不重写历史 target receipt bytes。保留的 feature 隔离目标是 std 仅供 host/test 显式启用，wire/serde 不引入隐式 std；当前 wire 会启用 std，不能把该目标视为现状。输入长度先验证再 alloc，使用现行 ModuleLimits；codec error bounded，不输出 secret payload。

<a id="sdk-feature-codec-evolution"></a>
### 6.2 Feature 与编码接口的演进边界

本节细化[版本化 consumer codec 合同](#sr2-sdk-contract)中的 feature 与编码接口；其 target records、运行时 authority 和兼容要求继续由主合同约束。

no_std wire 扩展先以实际消费者明确 target、允许的依赖与不支持 std 的原因，再调整 wire 到 std 的 feature 依赖和 serde/CBOR/JSON 的 alloc 配置。core、wire、显式 std 与 std+wire 分别检查，实际模块的依赖也须满足其 no_std 条件；只修改 SDK 的 cfg 或让现有 std builtin 编译成功，均不足以证明目标。扩展不要求所有现有 builtin 改为 no_std，不改生命周期导出或 wasm-1 字段。

编码错误目标按 API 与消费者共同演进：先保留旧输出字节/default/export fixtures，再使编码 helper 返回显式 Result，同批更新 builtin 调用点和 `crates/oasis7_builtin_wasm_modules/_templates/`。实施时明确每个入口如何把编码错误交给现行模块失败/运行时拒绝边界；既有领域 fallback 须说明业务含义，不能以 `empty_output()`、空 Vec 或 None 冒充编码成功。变更 Rust 返回类型需要消费者迁移，但不得夹带 wire ABI、字段默认值、effects/emits 顺序或 output_bytes 口径变化。若尚无可达的编码失败输入，记录该证据限制；不把资源耗尽猜测当作已知反例。


<a id="sdk-legacy"></a>
### 6.3 Legacy defaults 与版本拒绝

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；既有产品要求：[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)。仅承接本节已限定的局部义务，不替代该要求的完整验收。

保持§6.1旧manifest optional/default、tick缺省及effect/emit顺序；新target known/unknown/null严格解码与历史wasm-1分开，SDK不生成finality。

<a id="sdk-lifecycle"></a>
### 6.4 Lifecycle 导出

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；既有产品要求：[REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)。仅承接本节已限定的局部义务，不替代该要求的完整验收。

alloc/reduce/call及两种dispatch沿用原签名与init/teardown顺序；导出宏只是ABI入口，不能重定义模块业务或世界发布。同版本相同输入下，SDK内部整理不得改变dispatch产出的业务效果；验证需比较旧/新consumer结果，不能仅比较导出名。

<a id="sdk-wire-owner"></a>
### 6.5 单份 wire owner

ModuleCallInput/Context/EffectIntent/Emit/Output由SDK wire单一拥有；代表builtin与模板复用，不把领域类型或显式旧fallback误认为并行协议。

<a id="sdk-cbor"></a>
### 6.6 CBOR 与长度兼容

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；既有产品要求：[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)。仅承接本节已限定的局部义务，不替代该要求的完整验收。

沿用runtime wasm-1字段/default/output_bytes和序列顺序。现有serde_cbor helper调用并不自行证明所有target strict canonical重编码规则已实现；新codec须以旧字节fixtures逐项比较。

<a id="sdk-new-fields"></a>
### 6.7 新字段与回滚

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；既有产品要求：[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)。仅承接本节已限定的局部义务，不替代该要求的完整验收。

新增字段/feature沿用§10兼容规则，先说明默认值、旧模块与wasm32影响；禁止借内部API迁移破坏ABI或重写历史bytes。

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
| [SDK-01](wasm-sdk.prd.md#sr2-domain-case-1) | [§6.3 legacy/default/export与target版本](#sdk-legacy) | old bytes/defaults保持；strict new codec显式失败 | [实施验证手册](../../../testing-manual.md)；具体源码/测试与断言：`crates/oasis7_wasm_sdk/src/lib.rs` (SDK源码/tests) 静态核对serde defaults/export；现有三个坐标测试不覆盖完整wire roundtrip；按§11.2补旧bytes、known/unknown/null matrix；本轮未运行，原required/full适用义务不变 | 本文档采纳；实际task evidence存candidate/artifact/proof与exitcode | 本次未执行；proof语义与真实环境未证明 |
| [SDK-02](wasm-sdk.prd.md#sr2-domain-case-2) | [§6.2 feature隔离](#sdk-feature-codec-evolution) | 当前 feature 边界准确；实际消费者触发时证明 no_std wire；target 缺失 skipped | [实施验证手册](../../../testing-manual.md)；具体源码/测试与断言：`crates/oasis7_wasm_sdk/Cargo.toml` (Cargo features) default=[]、wire含std；按§11.2同candidate core/wire/std/std+wire及实际消费者target矩阵，缺target skipped；本轮未运行，原required/full适用义务不变 | 本文档采纳；实现 PR 记录实际 features/target/candidate、结果与原有 proof evidence | 本次未执行；不由 std wire check 推断 no_std wire，proof语义与真实环境未证明 |
| [SDK-03](wasm-sdk.prd.md#sr2-domain-case-3) | [§6.2 codec错误迁移](#sdk-feature-codec-evolution) | 显式编码错误与 API/调用点/模板同步；Result error 不伪造 empty success；既有 fallback 显式 | [实施验证手册](../../../testing-manual.md)；具体源码/测试与断言：`crates/oasis7_wasm_sdk/src/lib.rs` (SDK helpers) decode Result/encode Vec+expect；按§11.2补malformed、UTF8/U64和实际失败或故障注入，迁移builtin与模板；本轮未运行，原required/full适用义务不变 | 本文档采纳；实现 PR 记录具体测试路径、实际分支、候选与结果 | 当前 encode 仍 Vec+expect；本次未执行，不宣称已有编码故障反例或 full proof |
| [PRD-WORLD_RUNTIME-042](wasm-sdk.prd.md#sr2-obligation-prd-world_runtime-042) | [§11.2 分层验证](#sdk-feature-codec-validation) | &#124; PRD-WORLD_RUNTIME-042 &#124; test_tier_required &#124; SDK tests、可用时 wasm32 check、builtin wire usage scan、required-tier build &#124; SDK ABI、builtin 模块编译与 wire 兼容 &#124; | [实施验证手册](../../../testing-manual.md)；具体源码/测试与断言：逐项记录SDK tests、可用wasm32、builtin wire scan及required-tier build；`crates/oasis7_wasm_sdk/src/lib.rs` (SDK tests)仅已存在的六项基础/坐标断言，不能代签完整wire/编译矩阵；本轮未运行，原required/full适用义务不变 | 本文档采纳；真实implementation task evidence记录input/candidate/environment/exit/artifact | existing source仅partial，本次未运行；原有full/provider/headed义务保留 |
| [SDK rule 1](wasm-sdk.prd.md#sr2-sdk-rule-1) | [§3 当前feature能力](#sdk-current-capabilities) | core 默认 no_std+alloc；当前 wire 依赖 std；扩展按真实消费者验证隔离 | [实施验证手册](../../../testing-manual.md)；具体源码/测试与断言：`crates/oasis7_wasm_sdk/Cargo.toml` (Cargo) 与cfg静态核读，未来扩展按§11.2验证依赖树/实际模块；std-backed wire不算no_std wire通过；本轮未运行，原required/full适用义务不变 | 实际 SDK implementation evidence 记录 candidate/features/target/exit；本文档只澄清边界 | 未执行；host check 与当前 std wire check 不能代 no_std WASM |
| [SDK rule 2](wasm-sdk.prd.md#sr2-sdk-rule-2) | [§6.4 lifecycle导出](#sdk-lifecycle) | `alloc`、`reduce`、`call` 导出，`LifecycleStage`、`WasmModuleLifecycle`、`dispatch_reduce`、`dispatch_call` 与 `export_wasm_module!` 保持稳定；SDK 内部收敛不得静默改变模块种类、生命周期或业务效果。 | [实施验证手册](../../../testing-manual.md)；具体源码/测试与断言：`crates/oasis7_wasm_sdk/src/lib.rs` (SDK tests) default_alloc_handles_non_positive断言0/-1；两项dispatch测试断言输出及init/teardown各1次；宏导出及wasm32兼容需实际模块构建；本轮未运行，原required/full适用义务不变 | 实际SDK implementation evidence记录candidate/features/target/exit；本文档交付范围仅限文档 | 未执行；host check不能代wasm32 |
| [SDK rule 3](wasm-sdk.prd.md#sr2-sdk-rule-3) | [§6.5 wire单一owner](#sdk-wire-owner) | `ModuleCallInput`、`ModuleContext`、`ModuleEffectIntent`、`ModuleEmit`、`ModuleOutput` 及其编码 helper 由 SDK 持有一份 wire 定义。builtin 模块只保留领域结构，不得复制一套并行协议。 | [实施验证手册](../../../testing-manual.md)；具体源码/测试与断言：静态核对 `crates/oasis7_builtin_wasm_modules/m1_rule_move/src/lib.rs` (m1_rule_move) 与 `crates/oasis7_builtin_wasm_modules/_templates/m4_factory_module_template.rs` (factory模板)导入SDK wire；实施受影响builtin全量usage scan，代表消费者不代签所有第三方；本轮未运行，原required/full适用义务不变 | 实际SDK implementation evidence记录candidate/features/target/exit；本文档交付范围仅限文档 | 未执行；host check不能代wasm32 |
| [SDK rule 4](wasm-sdk.prd.md#sr2-sdk-rule-4) | [§6.6 CBOR/length](#sdk-cbor) | wire 编解码使用与 runtime `wasm-1` 一致的 Canonical CBOR 字段和默认值；输入、输出与 `output_bytes` 口径必须保持兼容。 | [实施验证手册](../../../testing-manual.md)；具体源码/测试与断言：对照SDK与runtime ABI类型，按§11.2旧input/output bytes/default/effects-emits顺序/output_bytes fixtures；现有坐标测试仅integer/legacy float边界，完整wire目标待补；本轮未运行，原required/full适用义务不变 | 实际SDK implementation evidence记录candidate/features/target/exit；本文档交付范围仅限文档 | 未执行；host check不能代wasm32 |
| [SDK rule 5](wasm-sdk.prd.md#sr2-sdk-rule-5) | [§6.2 错误传播](#sdk-feature-codec-evolution) | decode 的 Result 与当前 encode Vec+expect 区分；保留显式编码错误目标和不伪造空成功的规则 | [实施验证手册](../../../testing-manual.md)；具体源码/测试与断言：按§11.2核对decode Result、encode迁移、调用点明确fallback及实际故障；`crates/oasis7_wasm_sdk/src/lib.rs` (SDK源码)的expect不证明存在可达失败输入；本轮未运行，原required/full适用义务不变 | 实际 SDK implementation evidence 记录 candidate/测试分支/结果；本文档只澄清边界 | 未执行；无已知编码失败输入不等于所有边界已证明 |
| [SDK rule 6](wasm-sdk.prd.md#sr2-sdk-rule-6) | [§6.7 字段兼容](#sdk-new-fields) | 新字段或 feature 必须说明默认值、旧模块兼容性和 wasm32 构建影响。破坏性 ABI 变化不能通过 SDK 整理任务夹带进入。 | [实施验证手册](../../../testing-manual.md)；具体源码/测试与断言：按§11.2为每项新字段列default/旧fixture/target构建影响，对比历史bytes和回滚consumer；不由host测试推断wasm32通过；本轮未运行，原required/full适用义务不变 | 实际SDK implementation evidence记录candidate/features/target/exit；本文档交付范围仅限文档 | 未执行；host check不能代wasm32 |

现有partial/unrun来源：`crates/oasis7_wasm_executor/src/tests.rs`（output/fuel/memory/cache），`crates/oasis7_wasm_abi/tests/open_module_commands.rs`（legacy/envelope），`crates/oasis7_runtime/src/world/module_runtime_tests.rs`（metrics/due-preflight），`scripts/oasis7-node-wasm-metrics-monitor.test.sh`（window summary），`scripts/ci-verify-m1-wasm-summaries.py`（build对账）。这些不是新target receipt/migration/DC5已覆盖证据。GWSC [negative matrix](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix) 接收全部17QA cases，[runtime scenarios](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios) 接收10组runtime；本页domain rows为具体输入/assertion分配。

<a id="sdk-feature-codec-validation"></a>
### 11.2 SDK 现状与未来 feature/codec 的具体验证

下表维护验证设计，实际执行版本、命令、结果和未执行项留在实施 PR。2026-10-12 核查仅读取固定源码；未运行以下 Cargo、模块构建或发布验证。新增目标测试的名称与路径应在实现时落到真实测试，不把本表当作测试已存在。

| 对应要求 / 场景 | 验证方法、源码或测试位置 | 通过口径与范围 |
| --- | --- | --- |
| 当前支持边界 / SDK-02、SDK-03 | 静态核对 `crates/oasis7_wasm_sdk/Cargo.toml` 的 default/std/wire 依赖、`src/lib.rs` 的 cfg 与三个 codec 签名，并检查代表 builtin 的 Cargo 依赖和调用点 | 文档准确表述 core no_std、wire std、encode Vec+expect、decode Result；只证明源码描述一致 |
| 现有 core 与 wire 测试 | `crates/oasis7_wasm_sdk/src/lib.rs::tests` 的 `default_alloc_handles_non_positive`、`dispatch_reduce_runs_lifecycle_hooks`、`dispatch_call_runs_lifecycle_hooks`；wire feature 另选择该文件现有三项厘米坐标解析测试。入口为 `env -u RUSTC_WRAPPER cargo test -p oasis7_wasm_sdk`，wire 另加 `--features wire` | 只验收实际选中的生命周期/分配/坐标兼容场景；现有坐标测试不能代替 ModuleCallInput/ModuleOutput round-trip 或编码错误传播 |
| no_std wire 扩展 / SDK-02 | 实际消费者明确 target 后，对无 feature、wire、std、std+wire 分别核对 Cargo feature tree 与 SDK check，再检查该消费者。以 `env -u RUSTC_WRAPPER cargo check -p oasis7_wasm_sdk --no-default-features --target wasm32-unknown-unknown` 为 core 入口，分别选择对应 features；依赖检查使用同一组合的 Cargo tree/metadata | core 和目标 no_std wire 依赖中无 std，显式 std 组合仍可用，实际模块也满足其约束；当前 wire 含 std 是基线事实，不算目标通过；wasm32 缺失记 skipped，host 不能代签 |
| codec 与兼容 / SDK-01、SDK-03 | 在 SDK tests 增补旧 ModuleCallInput/ModuleOutput fixtures、损坏输入、缺失 action、UTF8/U64 边界与 target strict codec 场景；对编码真实失败分支或显式故障注入检查 Result 传播，随后核对 builtin 与 `_templates/` 调用点 | 旧字节/default/export/effects-emits 顺序/output_bytes 不变；错误可观察，既有 fallback 有业务定义，无 empty success；无可达编码失败分支时如实记录未覆盖，不虚构 panic 反例 |
| 受影响消费者 / SDK rule 2–6 | 对实际改动的 builtin、模板生成模块选择原有 sync/check 与 required-tier 编译入口；旧模块与迁移后模块使用同一 fixtures 比较 | 证明源码 API 迁移与既有 wasm-1 消费面兼容；不扩大成全部第三方、cross-host、full finality 或发布通过 |

## 12. 决策、长期风险与未决问题

选择单authority typedtarget而非重复wire，是为避免scope/hash/版本漂移。已知风险是implementation缺失、proof不可读、U64/UTF8投影误读、递归hash与历史误作新候选证据；WASM/runtime/QA分别按本页case和runtimereceiver解除。触发器为新ABI/schedule/schema、trust epoch、activation或consumer变更；需重新冻结合同与专业联审。SC9八fullcells及activeLLM/provider API parity、真实desktop+narrow/headed证据保持产品/QAauthority，不因本页文档完成消除。

<a id="sdk-evolution-triggers"></a>
SDK 的补充演进条件由 `wasm_platform_engineer` 维护：实际模块/第三方消费者提出不支持 std 的 target 和最小需求时，按 SDK-02 实施 no_std wire；codec 演进、消费者需要可恢复的编码错误，或出现可复现序列化失败时，按 SDK-03 实施编码错误与调用点迁移。当前未发现上述消费者阻塞或编码失败反例，保留目标及其验收，不承诺排期，也不将本文整理升级为玩家接入、持久世界或发布的新 gate。实施选择不改变已有适用的 ABI、required/full 或发布义务。


- 对应需求文档：`doc/world-runtime/wasm/wasm-sdk.prd.md`
- 稳定证据入口：`doc/world-runtime/wasm/evidence.md`

## 1. 设计原则

本页只定义 SDK 的稳定 wire/ABI 契约；实施任务、状态和历史验收由 GitHub
task issue / Project 与 Git history 追溯。可重复执行的验证入口见
[`evidence.md`](evidence.md)。

- 核心 SDK 默认 `no_std`，宿主便利能力通过显式 feature 隔离。
- wire schema 只有一个 owner；模块代码复用类型并只实现领域逻辑。
- ABI 稳定优先于内部去重，任何字段变更都必须保留 serde/default 兼容。
- 编解码错误的目标是显式接口结果，不能伪装为空业务成功；当前编码/解码 API 的差别见[当前能力](#sdk-current-capabilities)。

## 2. 分层

| 层 | 权威入口 | 要求 |
| --- | --- | --- |
| 生命周期与导出 | `crates/oasis7_wasm_sdk/src/lib.rs` | 保持 `alloc/reduce/call`、lifecycle trait、dispatch 和 export macro 稳定。 |
| Wire schema | `oasis7_wasm_sdk::wire` | 统一 input/context/effect/emit/output 类型与 Canonical-CBOR helper。 |
| Builtin 模块 | `crates/oasis7_builtin_wasm_modules/*` | 复用 SDK wire 类型；不得复制协议结构；codec fallback 必须显式。 |
| Runtime 接收端 | `oasis7_wasm_abi` 与 executor/runtime | 校验 ABI、schema、limits 与输出；不由 SDK 文档重定义。 |

## 3. Feature 与构建模型

| 构建组合 | 当前行为 | 保留目标 / 验证条件 |
| --- | --- | --- |
| 默认 core，非 test、无 std | no_std + alloc，稳定 lifecycle/export | 持续保持；需用真实 target check 验证 |
| wire | 启用 std 与共享 Canonical CBOR helper | 实际 no_std 消费者触发后支持 no_std+alloc+wire，并核对完整依赖 |
| 显式 std，或 std+wire | 提供标准库环境；wire 按选择启用 | 宿主便利能力保持显式；与 no_std 组合分别验证 |
| test | cfg(test) 使用标准库，wire 测试需选择 wire feature | host 测试不证明 no_std 模块或 wasm32 已通过 |

- `cfg_attr` 必须保持默认 core 不依赖 `std`；wire 不隐式依赖 std 是扩展目标，当前尚未满足。
- feature 组合需由 Cargo metadata/check 覆盖，避免 check-cfg 漂移。
- wasm32 target 缺失时验证应 fail clearly 或显式 skipped，不得产生虚假成功。

## 4. Wire 演进

- 增加字段时使用兼容默认值，并同时更新 runtime schema/evidence。
- 删除或改变字段语义属于 ABI 迁移，需独立兼容方案。
- 解码 helper 当前返回 `Result`；编码 helper 返回 `Result` 是保留目标，当前 `encode_output` 为 Vec+expect。迁移时按[接口演进](#sdk-feature-codec-evolution)同步调用点和模板；兼容 fallback 必须记录选择及其业务含义，不伪造空成功。
- `ModuleOutput` 默认值、effect/emit 顺序和 `output_bytes` 计算保持确定。

## 5. 验证图

- SDK unit tests：保留生命周期、分配与现有 wire 坐标测试；wire round-trip、损坏输入及错误传播的具体目标按[验证映射](#sdk-feature-codec-validation)补齐，不把测试位置视为已执行证据。
- wasm32 check：仅在 target 可用时作为通过证据。
- builtin scan/build：没有重复协议定义，代表性模块 sync/check 通过。
- required-tier：证明 runtime 与 builtin 消费面仍可编译。

## 6. 演进边界

SDK 文档不拥有 sandbox limits、artifact integrity、module storage、治理许可或玩家规则。相关变化必须回到 executor、module lifecycle/storage 与产品治理权威。
