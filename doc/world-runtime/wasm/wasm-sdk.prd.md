# WASM SDK 兼容与 Wire 契约

- 专业 owner：`wasm_platform_engineer`；状态：active professional authority（目标合同不等于已实现）。
- 内容审读基线：`eng-cc/oasis7`，源提交 `9c41d57b4436f71f0cf9b481043d882cfdc551ed`；内容整理日期：2026-09-26。
- SDK feature/codec 现状核查基线：`eng-cc/oasis7@e659ee8c6707547905e463a29e60ef910c62276a`，2026-10-12；仅静态核对源码与消费者，具体支持边界见[现状与适用条件](#sdk-current-scope)，不更新其他 SR2 目标的实现或运行证据。
- 独立审读与实际实现/测试候选证据由该 task evidence 记录；此处不预报审读通过、runtime capability、发布或 full proof。下方既有条款仍有效；仅显式日期/历史基线条款按其证据范围解释。

<a id="sr2-acceptance"></a>
## SR2 具体目标验收与专业承接

<a id="sr2-domain-case-1"></a>

### SDK-01 专业接受条件

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；产品承接子范围：[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) 的已确认历史按原block manifest replay；本条仅保持旧wire/default/字节长度与版本兼容，不由SDK选择governing version、激活manifest或执行历史replay。

输入：旧 manifests/defaults/export与target known/unknown/null codec matrix。断言：old bytes/defaults保持；strict new codec显式失败。目标字段、算法和恢复条件由 [SDK-01精确设计](wasm-sdk.design.md#sr2-sdk-contract) 承接；跨域 proof/publication obligation 由 [SDK-01外部接收器](wasm-sdk.design.md#sr2-sdk-contract) 承接。required codec/deterministic fixture 与 full actual finality/candidate artifacts 是不同层，本次未运行。

<a id="sr2-domain-case-2"></a>

### SDK-02 专业接受条件

输入：core no_std+alloc / explicit std / 当前 std-backed wire / unavailable wasm32；启动 no_std wire 扩展时另加实际消费者与 no_std+wire 组合。断言：当前 feature 依赖如实可读；扩展目标要求 host 和 module feature 分隔，wire 不再引入 std；target 缺失标为 skipped。目标字段、算法和恢复条件由 [SDK-02精确设计](wasm-sdk.design.md#sr2-sdk-contract) 承接；feature 的现状与目标分别按[具体验证映射](wasm-sdk.design.md#sdk-feature-codec-validation)判定。跨域 proof/publication obligation 由 [SDK-02外部接收器](wasm-sdk.design.md#sr2-sdk-contract) 承接。required codec/deterministic fixture 与 full actual finality/candidate artifacts 是不同层，本次未运行。

<a id="sr2-domain-case-3"></a>

### SDK-03 专业接受条件

输入：malformed canonical bytes、fallback、multibyte/U64边界，以及编码接口演进时真实可失败分支或明确的故障注入。断言：Result error 不伪造 empty success；既有 fallback 显式；编码 API、调用点与模板一并迁移，保持 wire ABI。当前 decode 返回 Result、encode 返回 Vec 并使用 expect 的差别，按[现状与具体验证映射](wasm-sdk.design.md#sdk-feature-codec-validation)记录，不据此宣称已有可触发 panic 的输入。目标字段、算法和恢复条件由 [SDK-03精确设计](wasm-sdk.design.md#sr2-sdk-contract) 承接；跨域 proof/publication obligation 由 [SDK-03外部接收器](wasm-interface.md#sr2-types) 承接。required codec/deterministic fixture 与 full actual finality/candidate artifacts 是不同层，本次未运行。


既有所有成功标准、验收、limits、公式、historical identities和能力限制保留；下表逐项承接到 [精确target设计](wasm-sdk.design.md#sr2-sdk-contract)，而非以任务链接代替设计。本次文档变更只承担文档合同采纳；现行实现partial与未来所需runtime/proof验证保持独立。typed字段共用 [interface types](wasm-interface.md#sr2-types)；build/signature/SDK/metrics局部结果均不证明canonical世界效果。

| 原 obligation identity（下方完整原文） | 具体承接结果 / 适用条件 | 精确设计 receiver | 验证范围 / evidence |
| --- | --- | --- | --- |
| [PRD-WORLD_RUNTIME-042原文](#sr2-obligation-prd-world_runtime-042) | &#124; PRD-WORLD_RUNTIME-042 &#124; test_tier_required &#124; SDK tests、可用时 wasm32 check、builtin wire usage scan、required-tier build &#124; SDK ABI、builtin 模块编译与 wire 兼容 &#124; | [PRD-WORLD_RUNTIME-042本域合同](wasm-sdk.design.md#sr2-sdk-contract)；下方原详细设计继续规定条款内的具体limits/policy/tooling | [partial验证入口](../../../testing-manual.md)；§11.1精确target场景；本次仅文档采纳，运行proof未执行 |

目标 acceptance 另外要求：有效 ModuleExecutionReceiptV1 仅 committed_success；canonical no-effect由runtime IntentDecisionV1区分；world branch、member_generation、decimal U64与UTF8byte bound全链一致；receipt core不包含递归blockproof。

migration/state/schedule/replay精确字段由interface独占，运行发布点由lifecycle/executor独占。观测不可进入计费/世界hash；Docker构建proof与external release finality独立；SDK旧wasm-1 optional/default不变。

完整DC5 attachment由GWSC/schema接收，SC9全部8cells与provider/headed proof不裁剪。


- 对应设计文档：`doc/world-runtime/wasm/wasm-sdk.design.md`
- 稳定证据入口：`doc/world-runtime/wasm/evidence.md`
- 专业权威：`wasm_platform_engineer`

## 1. 目标

为 builtin 与第三方 WASM 模块提供单一、轻量、可移植的 SDK 契约。SDK 必须默认适配 `no_std` 模块环境，并以唯一的 Canonical-CBOR wire 类型避免模块间协议漂移，同时保持既有 `wasm-1` ABI 和模块行为兼容。

<a id="sdk-current-scope"></a>
### 1.1 现状与适用条件

模块作者当前可以使用默认 core 的 `no_std + alloc` 生命周期与导出能力，或启用依赖 `std` 的 `wire` 来复用共享 CBOR 类型。`no_std + wire` 仍是兼容目标，尚不能作为当前支持能力。

当前 `encode_output` 返回 `Vec<u8>` 并保留内部 `expect`；`decode_input`、`decode_action` 已返回 `Result`。固定源码、代表 builtin 消费者与这些边界的对应关系见[设计现状表](wasm-sdk.design.md#sdk-current-capabilities)。

本轮静态核查尚未识别出被 `no_std + wire` 缺失阻断的实际消费者，也未取得当前 `ModuleOutput` 类型触发编码错误的已知输入；这不证明所有第三方模块或资源边界均已验证。

保留 no_std wire 与编码错误显式返回的目标，由 `wasm_platform_engineer` 按实际 no_std 消费者、codec 演进或可复现失败的需要落实[演进条件](wasm-sdk.design.md#sdk-evolution-triggers)。文档澄清不新增发布门禁，不把这些未实现目标自动列为当前玩家接入或持久世界交付的阻塞条件。

## 2. 范围

### 永久契约

<a id="sr2-sdk-rule-1"></a>
### SDK rule 1 原始条款

1. `oasis7_wasm_sdk` 的默认 core 在非 test、未启用 `std` feature 时使用 `no_std`，动态分配来自 `alloc`。当前 `wire` 会启用 `std`，因此默认 core 的支持范围不能扩张为 `no_std + wire` 已可用。保留宿主便利能力通过显式 `std` 隔离、wire 不隐式依赖 std 的目标；实际 no_std 消费者触发的扩展按 SDK-02 验证。
<a id="sr2-sdk-rule-2"></a>
### SDK rule 2 原始条款

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；产品承接子范围：[REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) 的同版本同输入结果稳定；本条仅约束SDK内部整理不静默改变lifecycle dispatch与业务效果，导出形式或局部hook测试不证明确定性提交。

2. `alloc`、`reduce`、`call` 导出，`LifecycleStage`、`WasmModuleLifecycle`、`dispatch_reduce`、`dispatch_call` 与 `export_wasm_module!` 保持稳定；SDK 内部收敛不得静默改变模块种类、生命周期或业务效果。
<a id="sr2-sdk-rule-3"></a>
### SDK rule 3 原始条款

3. `ModuleCallInput`、`ModuleContext`、`ModuleEffectIntent`、`ModuleEmit`、`ModuleOutput` 及其编码 helper 由 SDK 持有一份 wire 定义。builtin 模块只保留领域结构，不得复制一套并行协议。
<a id="sr2-sdk-rule-4"></a>
### SDK rule 4 原始条款

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；产品承接子范围：[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) 的已确认历史按原block manifest replay；本条仅保持旧wire/default/字节长度与版本兼容，不由SDK选择governing version、激活manifest或执行历史replay。

4. wire 编解码使用与 runtime `wasm-1` 一致的 Canonical CBOR 字段和默认值；输入、输出与 `output_bytes` 口径必须保持兼容。
<a id="sr2-sdk-rule-5"></a>
### SDK rule 5 原始条款

5. codec 失败必须以显式 `Result` 或调用点明确选择的 fallback 处理；SDK 不得把损坏输入静默转换为空输出或 `None`。当前解码 helper 已返回 `Result`，编码 helper 仍为 `Vec<u8>` 加内部 `expect`；编码错误显式返回是保留目标，不能把当前断言写成目标已完成。迁移时同步调用点、模板与可观察失败处理，不以伪空成功消除错误。
<a id="sr2-sdk-rule-6"></a>
### SDK rule 6 原始条款

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；产品承接子范围：[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) 的已确认历史按原block manifest replay；本条仅保持旧wire/default/字节长度与版本兼容，不由SDK选择governing version、激活manifest或执行历史replay。

6. 新字段或 feature 必须说明默认值、旧模块兼容性和 wasm32 构建影响。破坏性 ABI 变化不能通过 SDK 整理任务夹带进入。

## 3. 接口 / 数据

- 权威 crate：`crates/oasis7_wasm_sdk`
- wire feature：当前启用 `std`，承载共享输入、上下文、effects、emits、输出与 helper；no_std wire 为条件触发的兼容扩展目标。
- builtin 使用者：`crates/oasis7_builtin_wasm_modules/*`
- runtime ABI 与执行语义继续由 `wasm-interface.md`、`wasm-executor.prd.md` 和 `oasis7_wasm_abi` 拥有。
- 本文不拥有模块业务规则、hash/manifest 发布链、执行器资源限制或模块存储生命周期。

## 4. 验收

- SDK 实现验收须覆盖生命周期 dispatch、默认分配和 wire round-trip / 失败路径；已有测试与尚待补充的目标场景按[SDK 验证映射](wasm-sdk.design.md#sdk-feature-codec-validation)区分，文档核对不代表测试通过。
- 在 wasm32 target 可用时验证默认 core no_std 编译；启用 wire 的编译须同时核对 feature 依赖，当前 std-backed wire 通过不能代替 no_std wire 目标通过。target 不可用必须记录为 skipped，不能报告为通过。
- builtin 模块迁移需证明不再保留重复 wire 定义，并通过代表性 sync/check 与 required-tier 编译。
- SDK 变更必须明确向后兼容或提供迁移路径。
- feature/codec 演进的消费者、旧字节/default/export 比较与错误传播按 SDK-02、SDK-03 验收；未实施的扩展不因本文更新而升级为发布 gate。

## 5. 风险与非声明

- 当前 wire 已依赖 `std`；未来 no_std wire 扩展需防止 serde feature 组合重新引入 std，且不能由 host 测试推断模块目标已验证。
- 批量复用共享 wire 类型可能暴露历史字段差异，必须逐项校验，不能仅依靠文本替换。
- 本文不宣称所有第三方模块、所有 target 或发布候选已验证。

## 6. Validation & Decision Record

SDK ABI、wire 使用和 wasm32 兼容的稳定验证入口见
[`evidence.md`](evidence.md)。任务批次、当前状态和已完成工作历史由 GitHub
task issue / Project 与 Git history 承接。

<a id="sr2-obligation-prd-world_runtime-042"></a>

| PRD-ID | 测试层级 | 验证方法 | 回归范围 |
| --- | --- | --- | --- |
| PRD-WORLD_RUNTIME-042 | `test_tier_required` | SDK tests、可用时 wasm32 check、builtin wire usage scan、required-tier build | SDK ABI、builtin 模块编译与 wire 兼容 |

| 决策 ID | 选定方案 | 否决方案 | 依据 |
| --- | --- | --- | --- |
| DEC-WR-WASM-SDK-001 | 以稳定 `wasm-sdk` 权威合并 no_std 与 wire-dedup 完成记录 | 长期保留两组三件套作为并列权威 | 两者共同约束同一 SDK 兼容面；单一权威更能防止 feature、wire 和错误语义漂移。 |
| DEC-WR-WASM-SDK-002 | codec 失败显式返回或由调用点选择 fallback | SDK 静默返回空输出/`None` | 当前结构化错误契约要求失败可观测，旧静默措辞已失效。 |
| DEC-WR-WASM-SDK-003 | 明确 core no_std、std-backed wire 与编码 API 现状，保留按实际需求触发的兼容/错误目标 | 仅凭文档目标宣称已实现或新增发布阻塞 | 默认 core 和现有 wire 消费者的支持边界不同；静态接口差距尚不能证明运行故障。owner、触发条件与验证由配对设计承接。 |
