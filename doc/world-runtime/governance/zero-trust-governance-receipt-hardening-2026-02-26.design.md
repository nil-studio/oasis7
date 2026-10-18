# 零信任多节点治理与签名加固设计（2026-02-26）

- 对应需求文档: `doc/world-runtime/governance/zero-trust-governance-receipt-hardening-2026-02-26.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

<a id="1-设计定位"></a>
## 1. 设计定位与目标
定义 runtime 在不可信多节点环境下的工件验签、治理最终性绑定、收据签名升级与执行错误可观测性设计。

## 2. 设计结构
- 工件真实性链：统一 `artifact_identity` 校验、签名方案与受信任签名人集合。
- 治理原子 apply：把 proposal 校验、多签验证、模块变更应用与事件落档收敛到单一事务顺序。
- 收据签名层：兼容 HMAC 历史方案，同时引入节点签名/阈值签名与共识锚定字段。
- 错误观测层：将执行 trap 明确映射为 `OutOfFuel` / `Interrupted`。

## 3. 关键接口 / 入口
- `apply_proposal(proposal_id, finality_certificate)`
  - 历史提案拼写；current Rust wrapper 为 `apply_proposal(proposal_id)`，explicit finality 入口为 `apply_proposal_with_finality(proposal_id, &certificate)`；本设计下文区分现行调用和目标证书合同。
- `GovernanceFinalityCertificate` / `ReceiptSignature` / `SignatureAlgorithm`
- `validate_module_manifest` / `load_module_store_from_dir`

## 4. 约束与边界
- 不破坏现有 WASM ABI 与事件溯源框架。
- 任意治理落地必须绑定最终性证明与多签门限。
- 收据锚定字段必须可追溯到 `consensus_height` 与 `receipts_root`。

## 5. 设计演进计划
- 先完成设计补齐与互链回写。
- 再按项目文档任务拆解推进实现与验证。

## 零信任适用设计视图
owner runtime_engineer，WASM/签名/P2P 联审；eng-cc/oasis7 审读基线 f9d5a552d9af04c1b1398262808198a58e560230。旧 §§1–5 retained active contract 与历史 proposal wording 分开；目标是 artifact authenticity/certificate bound atomic apply/anchored receipt/structured trap，无新 PKI/BFT/业务/ABI/schema。测试定义与历史迁移 green 均不是实际 security proof。

<a id="hardening-artifact-boundary"></a>
### 工件 admission、加载与信任
producer supplies content-addressed bytes/ModuleArtifactIdentity 与 authority-approved signer/public key；validate/shadow/register 和 load/persist 都按同一 authenticity/trust/hash 检查，identity 必填、不接受 unsigned。missing/invalid/hash/trust failure 不安装模块或悄换 bytes，旧非法工件需显式迁移批准，不能运行时自动补签。WASM owns [ABI/artifact/executor authority](../wasm/wasm-interface.md)，cache compiled representation 不是 canonical artifact 或 activation permission。current module_store hydration owned replacement 全部验证再 install，late sorted record failure 保持 registry/artifact/cache，legacy no-store compatibility 不授予新 unsigned 资格。

<a id="hardening-governance-boundary"></a>
### 当前治理 API 与原子顺序
current apply_proposal(proposal_id) 受 local policy，explicit proof path apply_proposal_with_finality(proposal_id,&certificate)；旧两参数 spelling 是历史 proposal，保留不作为现有 Rust API。certificate 必须绑定 same world/proposal/manifest/height 与适用 required_signers/签名门限，proposal 状态/manifest 一致后验证材料；P2P owns finality/validator/round eligibility，runtime 的治理 certificate check 不构造完整 BFT。
当前 governed proposal typed stage 准备 registry/artifact/schedule/cache/manifest/proposal/journal/allocator/backpressure/consensus，ordered ModuleEvent → ManifestUpdated → Governance::Applied，全部成功一次 install，错误优先级不变，postprepare fail 无中间 applied/registry 状态。缺证/错绑定/threshold/signature/module application failure 原子拒绝；目标全 root/instance/restore/outbox 仍 partial，不把 current 局部 seam 扩成整体 generation proof。activation version 由 [root canonical block](../design.md#runtime-version-design) 决定，不由此 wrapper 或节点升级选择。

<a id="hardening-receipt-boundary"></a>
### Receipt 签名、历史与失败
HmacSha256 历史 compatibility retained；Ed25519 单签 signer 必填，ThresholdEd25519 V1 participants/minimum signature-set validation 并核 threshold/participants，不声明真实 aggregate crypto。签名 payload 必须 bind consensus_height/receipts_root；错 anchor/signature/auth linkage 不消费 pending、不写部分业务 receipt。current ingestion typed stage 是有界发布证明，现 DTO 未完成 receipt-domain descriptor/idempotency ledger，签名成功不证明 duplicate dedup 或互斥 winner。历史 replay 依原 receipt/block manifest，不当前算法/manifest 重解释；信任材料缺失保留诊断 fail closed，不隐式用 HMAC downgrade。

<a id="hardening-trap-boundary"></a>
<a id="trap、状态事务持久化与演进"></a>
### Trap、状态/事务/持久化与演进风险
Wasmtime OutOfFuel→OutOfFuel，Interrupt→Interrupted，其余 Trap/Timeout/InvalidOutput 等 current error vocabulary 保持；executor init/codec/output/limits 拒绝不同于 governance rejection，watchdog local timeout 不作为 deterministic clock。sandbox 无 I/O/world authority，module/fees/state/events root stage 后发布；失败 business delta 丢弃后既有格式 ModuleCallFailed 单独 atomic audit 的 current 局部语义保留。
state 包括 trust registry/proposal/manifest/module/receipt anchor；transaction stage 与持久 generation 是独立边界，后者服从 root §6.2.3 和 ModuleStore/restoration 合同，不能由 Applied append 推导 durable fsync。旧 ABI/event/HMAC reading compatibility 保留，迁移缺签名工件须 explicit authority approval；资源/output/call/cache bounds 使用现有 WASM limits，不新设阈值。observability 返回结构化错误/审计而不额外泄露 secret/payload，status 不驱动 apply。残余是完整激活历史/crypto aggregate/full BFT/统一 outbox proof，安全材料/ABI/epoch/消费者或入口变化触发 runtime/WASM/P2P/QA 复核。

### 2.1 需求承接与分配表

输入身份 eng-cc/oasis7@f9d5a552d9af04c1b1398262808198a58e560230；新增 local anchors 是本文技术接受关系，非机器 schema。每行范围独立，外部未决保留。

| 上游 requirement / acceptance | 具体 obligation 与条件 | 本设计条款 | 外部 owner / dependency | 排除或未覆盖 |
| --- | --- | --- | --- | --- |
| [REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)；Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc | 专业依据：[条款](zero-trust-governance-receipt-hardening-2026-02-26.prd.md#hardening-acceptance-registry)；identity必填/no unsigned/registration+load signature/trust/hash，late fail无registry/artifact/cache半写 | [设计](#hardening-artifact-boundary) | WASM artifact/ABI；runtime module store | 无PKI/完整release授权 |
| [REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)；Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607 | 专业依据：[条款](zero-trust-governance-receipt-hardening-2026-02-26.prd.md#hardening-acceptance-registry)；current API wrapper/explicit finality，world/proposal/manifest/height/threshold/signature绑定，orderedprepare/install | [设计](#hardening-governance-boundary) | P2Pfinality/validators；WASMmodules | localcert/atomicapply非BFT/全root |
| [REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)；Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc | 专业依据：[条款](zero-trust-governance-receipt-hardening-2026-02-26.prd.md#hardening-acceptance-registry)；HMAClegacy、Ed25519/threshold participants与consensus_height/receipts_root签名，错anchor无队列消费 | [设计](#hardening-receipt-boundary) | P2Pproof/signer材料；runtime ingestion | 无aggregatecrypto/lineagewinner/完整DTO去重 |
| [REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)；Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607 | 专业依据：[条款](zero-trust-governance-receipt-hardening-2026-02-26.prd.md#hardening-acceptance-registry)；OutOfFuel/Interrupted明确映射、output/codec/init结构错，无部分业务输出 | [设计](#hardening-trap-boundary) | WASM executor/ABI；runtime output stage | localwatchdog非consensusmeter/fullproof |

### 11.1 验证映射表

所有下列行为验证在本次文档编辑中未运行。定义/计划与当前实现、实际执行、发布分别成立；执行须另固定 source/integration/tested tree、config/world/entry/environment/window、exit/result/artifacts，并回 PR 与实际验证记录。target场景尚无完整runner时明确保持待证明，现有test/manual只是有界接收入口，不能伪称已实现或通过。

| 上游 requirement / acceptance | 本设计条款 | 独立 obligation / 条件 | 验证 source / ID、层级及候选环境 | 证据目标 | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)；Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc | [设计](#hardening-artifact-boundary) | 专业依据：[条款](zero-trust-governance-receipt-hardening-2026-02-26.prd.md#hardening-acceptance-registry)；identity必填/no unsigned/registration+load signature/trust/hash，late fail无registry/artifact/cache半写 | [现行 manual](../../../testing-manual.md)；精确局部 source ../../../crates/oasis7_runtime/src/world/module_store_load_transaction_regressions.rs（定义/入口，非执行证据）；required missing/tamper/late-record/legacy no-store定义；full target trust rotation/迁移与两路径对比，记录原root/cache | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 无PKI/完整release授权 |
| [REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)；Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607 | [设计](#hardening-governance-boundary) | 专业依据：[条款](zero-trust-governance-receipt-hardening-2026-02-26.prd.md#hardening-acceptance-registry)；current API wrapper/explicit finality，world/proposal/manifest/height/threshold/signature绑定，orderedprepare/install | [现有 test/manual](../../../crates/oasis7_runtime/src/tests/module_lifecycle_transaction_regressions.rs)；required register/upgrade/activate/deactivate/mixed batch postprepare rollback定义；full target wrong-world/proposal/manifest/threshold/signature与activation窗口 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | localcert/atomicapply非BFT/全root |
| [REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)；Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc | [设计](#hardening-receipt-boundary) | 专业依据：[条款](zero-trust-governance-receipt-hardening-2026-02-26.prd.md#hardening-acceptance-registry)；HMAClegacy、Ed25519/threshold participants与consensus_height/receipts_root签名，错anchor无队列消费 | [现有 test/manual](../../../crates/oasis7_runtime/src/tests/effects.rs)；required effect_pipeline_signs_receipt_with_ed25519_anchor/ingest_receipt_rejects_ed25519_anchor_mismatch/effect_pipeline_signs_receipt_with_threshold_ed25519定义；full target历史manifest/receipt恢复 | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | 无aggregatecrypto/lineagewinner/完整DTO去重 |
| [REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)；Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607 | [设计](#hardening-trap-boundary) | 专业依据：[条款](zero-trust-governance-receipt-hardening-2026-02-26.prd.md#hardening-acceptance-registry)；OutOfFuel/Interrupted明确映射、output/codec/init结构错，无部分业务输出 | [现行 manual](../../../testing-manual.md)；精确局部 source ../../../crates/oasis7_wasm_executor/src/tests.rs（定义/入口，非执行证据）；required fuel/interrupt/memory/output/init定义；full target runtime调用+费用/state/event rollback与兼容replay | 未运行；未来同候选 log/root/receipt/metrics或consumer artifact入task evidence，QA判定 | localwatchdog非consensusmeter/fullproof |
