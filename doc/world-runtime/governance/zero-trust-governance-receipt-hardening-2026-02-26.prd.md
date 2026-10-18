# oasis7 Runtime：零信任多节点治理与签名加固（2026-02-26）

- 对应设计文档: `doc/world-runtime/governance/zero-trust-governance-receipt-hardening-2026-02-26.design.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history；本文与 design 保留治理、签名与原子 apply 合同。

审计轮次: 4


## 1. Executive Summary
- 在“不可信多节点/对抗环境”前提下，补齐 runtime 在工件真实性、治理落地原子性、收据签名可信度、执行错误可观测性上的关键缺口。
- 保持现有 WASM ABI 与事件溯源框架可回放，不引入破坏性迁移。

## 2. User Experience & Functionality
### In Scope
- P0 工件真实性链：
  - `artifact_identity` 从“可选”收敛为“必填”。
  - 禁止 `unsigned:*` 身份签名。
  - 注册（shadow/validate）与加载（load/persist）两条路径都执行工件签名验真与信任校验。
- P0 治理与共识绑定：
  - `apply_proposal` 需绑定共识最终性证明与多签门限校验。
  - `apply` 顺序改成原子提交语义：先验证与模块事件应用，最后写 `Governance::Applied` 与 `ManifestUpdated`。
- P1 收据签名升级：
  - 在现有 `HmacSha256` 之外增加节点签名与阈值签名算法。
  - 收据签名负载纳入 `consensus_height` 和 `receipts_root`，形成链上锚定。
- P1 错误可观测性：
  - Wasmtime 执行错误区分 `OutOfFuel` 与 `Interrupt`，不再统一折叠为 `Timeout`。

### Out of Scope
- 完整 PKI（证书吊销、在线信任服务、跨域 CA）。
- 共识模块内的拜占庭协议升级（仅消费最终性结果与签名材料）。
- 变更 WASM 模块业务语义。


## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（文档迁移任务）。
- Evaluation Strategy: 通过文档治理校验、引用扫描与任务日志检查验证迁移质量。

## 4. Technical Specifications
### 1) 工件真实性链
- `ModuleArtifactIdentity`：
  - 新增 `signature_scheme`（例如 `ed25519`）与 `signer_node_id`。
  - `artifact_signature` 不再允许 `unsigned:*`。
- `World`：
  - 新增受信任签名人集合（按 `node_id -> public_key`）。
  - `validate_module_manifest` 和 `load_module_store_from_dir` 都调用统一验签函数。

### 2) 治理共识绑定与原子 apply
- 新增 `GovernanceFinalityCertificate`：
  - `world_id`、`proposal_id`、`manifest_hash`、`consensus_height`、`required_signers`、`signatures`。
- `apply_proposal` 新签名：
  - `apply_proposal(proposal_id, finality_certificate)`。
  - 此两参数拼写保留历史提案，不是 current Rust API；现行入口为 local-policy-gated `apply_proposal(proposal_id)` 与 `apply_proposal_with_finality(proposal_id, &certificate)`，准确边界见本文 retained active contract 及配对设计。
- 原子序：
  1. 校验 proposal 状态、manifest 与 certificate 一致性。
  2. 校验多签门限与签名有效性。
  3. 应用 module changes。
  4. 写 `ManifestUpdated`。
  5. 写 `Governance::Applied`（最后写入，避免“已应用但未完成”）。

### 3) 收据签名升级
- `SignatureAlgorithm` 新增：
  - `Ed25519`（节点单签）。
  - `ThresholdEd25519`（阈值多签，V1 用聚合签名声明 + 最小签名集合校验）。
- `ReceiptSignature` 新增字段：
  - `signer_node_id`（单签必填）。
  - `threshold`、`participants`（阈值签名场景）。
  - `consensus_height`、`receipts_root`（锚定字段）。
- `ReceiptSigner`：
  - 保留 `hmac_sha256` 以兼容历史测试。
  - 新增 `ed25519(...)` 与 `threshold_ed25519(...)`。

### 4) 执行错误可观测性
- `ModuleCallErrorCode` 新增：
  - `OutOfFuel`、`Interrupted`。
- Wasmtime trap 映射：
  - `Trap::OutOfFuel -> OutOfFuel`
  - `Trap::Interrupt -> Interrupted`

## 5. Risks & Roadmap
- T0：设计建档与项目拆解。
- T1：P0 工件真实性链。
- T2：P0 治理共识绑定 + 原子 apply。
- T3：P1 收据签名升级与共识锚定。
- T4：P1 执行错误可观测性补强。
- T5：回归测试、文档回写与收口。

### Technical Risks
- 历史工件若缺失合法签名会在加载阶段被拒绝，需要迁移脚本补签。
- 治理接口签名变化会影响现有调用方测试，需要一次性同步。
- 阈值签名在无完整聚合库时先走“参与者签名集合”校验，后续可替换为真实聚合签名。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-ENGINEERING-006 | 文档内既有任务条目 | `test_tier_required` | `./scripts/doc-governance-check.sh` + 引用可达性扫描 | 迁移文档命名一致性与可追溯性 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-DOC-MIG-20260303 | 逐篇阅读后人工重写为 `.prd` 命名 | 仅批量重命名 | 保证语义保真与审计可追溯。 |

## 原文约束点映射（内容保真）
- 原“目标” -> 第 1 章 Executive Summary。
- 原“范围” -> 第 2 章 User Experience & Functionality。
- 原“接口 / 数据” -> 第 4 章 Technical Specifications。
- 原“里程碑/风险” -> 第 5 章 Risks & Roadmap。

## Retained active hardening contract 与历史 provenance
本文 dated 标题、T0–T5、DEC-DOC-MIG-20260303、原文映射及 PRD-ENGINEERING-006 doc-migration row 原样保留为历史过程/迁移验证，不证明运行时 security 或多节点能力。现行/目标 hardening 条款仍为 retained active technical contract，当前实际 API 与证据边界如下；不得因日期或历史迁移完成退役安全义务。
当前 Rust API 是 World::apply_proposal(proposal_id) local-policy-gated wrapper 与 World::apply_proposal_with_finality(proposal_id,&certificate)，定位 crates/oasis7_runtime/src/world/governance.rs。旧 §4 两参数 apply_proposal(proposal_id,finality_certificate) 是历史提案 spelling，保留原提案序作为目标合同，不是当前可调用签名。
工件必填 signed identity/no unsigned、registration/load 双验证、trust map、certificate world/proposal/manifest/height/threshold/signature、module changes → ManifestUpdated → Governance::Applied 原子序、HMAC 历史/Ed25519/threshold participants/receipt anchor 和 OutOfFuel/Interrupted 全部保留；具体当前有界 staged apply 不证明所有 instance/restore/outbox 的统一 root transaction。独立接受见 [artifact](zero-trust-governance-receipt-hardening-2026-02-26.design.md#hardening-artifact-boundary)、[governance](zero-trust-governance-receipt-hardening-2026-02-26.design.md#hardening-governance-boundary)、[receipt](zero-trust-governance-receipt-hardening-2026-02-26.design.md#hardening-receipt-boundary)、[trap](zero-trust-governance-receipt-hardening-2026-02-26.design.md#hardening-trap-boundary)。
接受产品 DWE-001/004 的 artifact/execution binding 子义务，SC-1/4/5/7 的 certificate/receipt integrity 子义务；签名真实性不证明 finality BFT rounds/locks/validator transition 或 lineage winner/receipt去重，治理激活边界与历史回放仍走 root design。保持 no PKI/BFT升级/ABI业务变更，threshold signature-set validation 不宣称密码学 aggregate signature；新执行证据由 design §11.1 和 task evidence 承接，全部旧历史证据身份不重写。

<a id="hardening-acceptance-registry"></a>
### Hardening 分项专业接受
工件 registration/load双验与trust/hash拒绝、certificate绑定及atomicapply、receipt锚定/HMAC历史/threshold集合验证、OutOfFuel/Interrupted分别是独立接受关系，分别由配对设计四条boundary与验证表接收。旧migration-only验证不代签任何一项。
