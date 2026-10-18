# oasis7 Runtime：线上模块发布合法性闭环补齐（2026-03-08）设计

- 对应需求文档: `doc/world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

## 1. 设计定位
定义线上模块发布合法性闭环方案，让模块发布前置到可审计的合法性校验、治理约束与发布门禁。

## 2. 设计结构
- 发布校验层：检查模块来源、版本、签名与权限是否合法。
- 治理门禁层：把发布申请与治理审批、白名单和环境限制绑定。
- 上线执行层：仅允许通过合法性校验的模块进入线上发布流程。
- 审计回写层：记录发布决策、阻断原因与上线证据。

## 3. 关键接口 / 入口
- 模块发布合法性校验入口
- 治理审批/白名单
- 线上发布执行入口
- 发布审计记录

## 4. 约束与边界
- 合法性校验必须先于线上启用。
- 阻断原因需要明确、可追溯。
- 不在本专题扩展完整市场发布体系。

## 5. 设计演进计划
- 先补发布合法性规则。
- 再接治理门禁与执行阻断。
- 最后沉淀上线审计证据。

## SR2 发布信任与激活目标设计

Owner runtime_engineer；作者审读日期 2026-09-26；固定 source baseline `9c41d57b4436f71f0cf9b481043d882cfdc551ed`；独立审读未完成。原五节发布义务仍有效。本节为 concrete target design，不声称外部 worker、materializer hardening、BFT 或完整 release proof 已实现。

## 1. 问题、目标与非目标
线上 legality 需要 artifact bytes、build proof、签名 authority、epoch certificate、activation 与 execution result 逐层可验证；仅 identity_hash、provider response、CI artifact 或 localEffectReceipt 不能提升为 canonical world effect。成功是仅激活经外部治理证明的 exact manifest；不扩展市场/密码算法、不更改 current modsig domain。

## 2. 上游约束与相关角色
Release producer submits evidence；WASM owns deterministic build/identity；P2P owns epoch/stake/finality；runtime owns receiver 与原子 activation；QA owns fixed benchmark、negative fixtures 和 combined acceptance；node ops owns实际环境。CI 只有开发 regression/check，不是 production issuer。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-7a3866ed-cd66-48ea-9cc7-bb793575c957, O7-REQ-d01cf80e-1172-4963-8bc0-877dc9de8e00, O7-REQ-1440a1a4-3240-4644-8ada-bf9a34771465；[professional_acceptance: signature 精确结果接收](online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-source-and-trust) | AC-1..4/8..9 signature/snapshot/local-self-sign prohibition | [sr2-release-trust](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | located not executed / prototype非BFT |
| Trace refs: O7-REQ-b89862c6-6316-4511-afdc-1e2d1e9a5e8e, O7-REQ-a5a2d539-b2c8-42dd-83e0-a6108b5ab288, O7-REQ-c9c94faf-43bb-474c-8cf9-7ed1441e5494, O7-REQ-c1427344-798f-47f1-9dab-790fd3693cf7, O7-REQ-f25099a9-5f30-466e-939a-48ac57cbead2, O7-REQ-80d9506a-b1e5-4ac2-8d68-eb73ecfccd51；[professional_acceptance: node 精确结果接收](online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-node-release) | SC-4..7 AC-5..7/10..11/13..17 node proof and CI separation；AC-14 独立接收[当前 epoch attestation 授权](../../product/world-infrastructure/prd.md#req-infra-release-attestation-authorization)，AC-15 独立接收[三类发布 runbook 阻断分诊](../../product/world-infrastructure/prd.md#req-infra-release-release-triage)，两者不由证据可追溯性或 submit 响应代签 | [节点证据接收子范围](#sr2-release-node-evidence-receiver) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | ACK不等于 activation；external worker absence |
| Trace refs: O7-REQ-02b10123-7422-4509-9d04-ea43444890c6, O7-REQ-c2a3ca44-d9b3-4ae0-8245-d8f496f38673, O7-REQ-8d0fe4a1-7c99-42d2-8b5d-7c4990591993, O7-REQ-0ed26c5c-b1a1-4e51-a462-56bd4712e295, O7-REQ-d01cf80e-1172-4963-8bc0-877dc9de8e00, O7-REQ-c9c94faf-43bb-474c-8cf9-7ed1441e5494, O7-REQ-80d9506a-b1e5-4ac2-8d68-eb73ecfccd51；[professional_acceptance: quality 精确结果接收](online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-release-quality) | NFR-OMR-1..7 performance/convergence/errors/security；另独立承接 [AC-12 固定基准与归档产物](online-module-release-legality-closure-2026-03-08.prd.md#sr2-release-acceptance)，不以其他 NFR 通过代替 AC-12 | [sr2-release-validation](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-validation) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | benchmarks未运行，本地不是full |
| Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[professional_acceptance: activation 精确结果接收](online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-version-dependency) | activation/migration exact boundary and replay；原 [activation 字段 authority](module-lifecycle.md#sr2-activation-contract) 与迁移合同不变 | [sr2-release-trust](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust) | WASM/runtime/P2P/QA exact receiver 与 candidate readback | 当前 prepared proposal仅partial |
| Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[product_requirement: REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) | 仅 activation proof / exact governing manifest 与历史原版本 replay 依赖；经 [runtime 版本接收](../design.md#runtime-version-design) 和 [activation 字段 authority](module-lifecycle.md#sr2-activation-contract) 承接。 | [sr2-release-trust](#sr2-release-trust) | Runtime 接收 activation；P2P 提供 canonical finality；WASM 提供原版本 artifact；QA 核对同候选窗口。 | 保留产品部分覆盖/实现未核实；不替代完整 BFT、待决 intent 当前条件重裁决、linked replacement 互斥或消费者 full 组合；原四组专业义务及 SC-9/DC-5 八 cells 均独立保留。 |

本表节点发布证据引用配对 PRD 的 sr2-release-acceptance 表中 SC-4..7 / AC-5..7/10..11/13..17 子范围；原 sr2-release-node-proof 锚点的 SC-9/DC-5 组合证明仍是独立外部义务。新增 Trace refs 仅限定既有产品依赖，不改变产品或实现状态。

## 3. 当前状态、目标状态与差距
原 release PRD 描述的 SC/AC 是独立 obligations，历史 TASK 和 run 不变。Current production action controls 已有外部 finality 校验入口和 node-submit/build packaging partial locators；current builtin materializer 仍有 source fallback，不接受 identity/receipt/policy input，该缺口不能被 production action gate 覆盖。Current prototype finality 不是 BFT。目标 VersionActivation/ExternalEffectReceipt 的完整 trust/atomic replay 未被本次运行证明；SN/SR1 prepared drafts 仍 proposal-only。

## 4. 边界与结构
BuildReceipt 证明 particular build bytes/provenance；ArtifactIdentity 证明 signer 对 exact wasm/source/build_manifest 身份签名；release attestation 绑定 request/module/platform/proof bytes；governance certificate 证明 epoch snapshot 阈值准许 exact manifest；VersionActivation 证明生效 canonical boundary；execution receipt 证明实际结果。六者不能互相替代，locator/CID/path 仅检索入口。

## 5. 关键运行流程
<a id="sr2-release-node-evidence-receiver"></a>
<a id="sr2-release-trust"></a>
### 发布 proof / trust receiver

节点证据接收子范围由本节步骤 1–2、5 及后续治理接收关系共同承接，重点是 request/proof bytes、signer/platform 去重与冲突、归档和 node submit；它不将 ACK 提升为 activation，也不取代其他签名、epoch/finality 或 SC-9/DC-5 义务。
1. decode exact current release proof schema、version、size 与 required fields；未知 scheme/version 拒绝。read release request 和 retained manifest，确认 request_id/wasm_hash exact match。
2. fetch `proof_cid` archive，读取 attached files，verify canonical payload_sha256、长度与 evidence identity；必须绑定 request_id/signer_node_id/platform/build_manifest_hash/source_hash/wasm_hash/builder_image_digest/container_platform/canonicalizer_version。缺 payload、CID 不可达或 matching digest却 wrong body 均拒绝。
3. 当前 artifact signature body 是固定 `modsig:ed25519:v1` 对 exact wasm_hash/source_hash/build_manifest_hash；签名验证与 deterministic build profile 按 [build contract](../wasm/wasm-deterministic-build-pipeline.design.md#sr2-build-contract)，不能以 target H 替换 current domain。
4. 从 verified canonical epoch snapshot 读取 epoch_id/validator_set_hash/stake_root/threshold_bps/min_unique_signers/effective_height/trust_root_version；signer identity/key epoch 必须成员有效，signature 身份去重，stake checked 聚合且同时满足 bps 和独立 signer 下限。未知/重复/撤销/错 epoch/snapshot/threshold 失败。
5. attestations 按 signer_node_id+platform 去重；same key conflicting hash/cid 拒绝并保留首条审计。不同平台 keyed token 不冒充 cross-platform hash equality；同平台不可复现阻断。
6. external certificate 必须 bind proposal_id+manifest_hash+consensus_height+完整 epoch snapshot；production legacy Install/Upgrade/Rollback/ModuleReleaseApply 或本地 apply_proposal 自签路径拒绝，只允许带外部 finality 的授权入口。无 silent fallback。
7. staged manifest/registry/activation/migration/events/cache/schedule 按 [lifecycle publication](module-lifecycle.md#sr2-migration-publication)，prepared success 后单一 durable commit；certificate、activation core 与 canonical inclusion exact matching before consumer publication。
8. 原始 bytes/proof/certificate/epoch/activation/event mappings append-only retained；重复 exact attestation/receipt 返回 original disposition，冲突不重扣费、不重执行。撤销/轮换/rollback 必须新治理记录，超 grace window 旧 key 拒绝；不可达新 manifest 保持上一版并禁用切换。
External receipt 采用 [lifecycle single field authority](module-lifecycle.md#sr2-external-contract)：provider immutable succeeded|failed 与 unverified|verified_canonical|reorged|suspended 分离。Receiver 验证 scope/request/parent execution/provider/key/result/payload hashes、provider proof、canonical inclusion receipts_root/authority_snapshot_hash/finality proof；provider failure不生成 module success，raw receipt 不计 committed，local unsigned/signable EffectReceipt 不证明 production external trust。

## 6. 接口与数据合同
| 接口 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| release request/attestation | release node → consensus receiver | request_id + wasm_hash + signer/platform + proof_cid | immutable signer/platform first evidence | structured invalid-request/encoding/queue errors，submit ACK 非 commit | existing ModuleReleaseSubmitAttestation |
| artifact identity | builder signer → node loader | wasm/source/build_manifest hashes + modsig version | exact bytes before load/cache | invalid/unsigned prod rejects | current signature domain 不改 |
| finality certificate | external validators → runtime apply | proposal/manifest/height/epoch/set/stake/threshold/min_signers | snapshot boundary exact match | GovernanceFinalityInvalid | no local self-sign prod downgrade |
| target activation/external receipt | runtime/provider + P2P → snapshot/replay/consumer | VersionActivationV1 / ExternalEffectReceiptV1 exact lifecycle fields | canonical commit and durable idempotency | missing/conflicting proof blocked | target separate from current ABI |

Shared types/byte bounds/U64/ContentRef/profile 以 [interface types](../wasm/wasm-interface.md#sr2-types) 为唯一 authority；current proof payload JSON/signatures 与 target canonical CBOR H 分开验证。Target activation/external core domains分别 oasis7/version-activation/v1 与 oasis7/external-effect-receipt/v1；exclude own IDs/finality wrappers，proof必须绑定 exact core，不 recursive hash。

## 7. 状态、事务与持久化
release draft→signed→finalized→active/revoked、request proposed→attested→threshold_reached→finalized、trust draft→pending→effective→retired 原义不改。submit/approve 不等于 active；effect不由 status 文本证明。Retain request/attestation/proof/certificate/trust snapshot/activation/ModuleRelease event mappings，snapshot+tail恢复同 root/order和 old schedule；任何 prepare/migration failure 无 partial activation。Current proposal seam 的 fixed lifecycle→ManifestUpdated→Governance::Applied order 保留，complete durable migration/outbox 尚为 target。

## 8. 部署、安全与运行约束
最小权限 node 持自身 key，私钥不进仓库/log，日志只有 signer和digest。Production allow_local_fallback=false、deterministic seed signer禁用、CI write-disabled；materializer fallback gap显式单列不假装已移除。Online unreachable/missing-or-rolled-back/identity-drift 分别 builtin_release_manifest_unreachable、builtin_release_manifest_missing_or_rolled_back、builtin_release_manifest_identity_drift，值守 receiver是 testing-manual.md S11；unknown provider/proof scheme 一律 fail closed。

## 9. 质量与容量
热 cache 单模块 manifest+identity+signature p95<=200ms；100 signer 热 cache finality p95<=50ms；<=2治理 epoch收敛、trace100%、未签/伪签接受率0，均继承 PRD 阈值且独立 measured evidence。Duplicate NFR-OMR-6 按原文分别为 submit结构响应与finality latency，不静默编号。实际 scripts/oasis7-runtime-finality-baseline.sh summary.md/summary.json记录候选、配置、规模、窗口、分位数；本次未执行。

## 10. 兼容、迁移与回滚
Current release mapping append-only且request不可重写。新 target receivers 不强制 current wasm-1新字段；trust root key rotation只在治理授权 grace内兼容，结束拒旧证书。历史 artifact、proof、schedule保留给 replay；rollback必须治理撤销/新activation，节点不能手改。Materializer source fallback能力差距在后续实现前阻断 production hardening claim，但不阻断本 concrete design authoring。

## 11. 验证设计与可追溯性
<a id="sr2-release-validation"></a>
required negative fixtures/partial tests与full真实环境分别记录，不以 node-submit ACK/模板/hash字符符合代替 proof acceptance。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-7a3866ed-cd66-48ea-9cc7-bb793575c957, O7-REQ-d01cf80e-1172-4963-8bc0-877dc9de8e00, O7-REQ-1440a1a4-3240-4644-8ada-bf9a34771465；[professional_acceptance: signature 精确结果接收](online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-source-and-trust) | [sr2-release-trust](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust) | AC-1..4/8..9 signature/snapshot/local-self-sign prohibition | required module_action_loop_release_controls_tests.rs production install/upgrade/rollback rejection and external-finality acceptance；RCP-01/02 wrong request/root/signer/epoch/branch/proof + SR2-RT-06；full actual external trust bytes on same candidate；[located partial source](../../../crates/oasis7_runtime/src/tests/module_action_loop_release_controls_tests.rs) | Task trust/cert/proof archive | located not executed / prototype非BFT |
| Trace refs: O7-REQ-b89862c6-6316-4511-afdc-1e2d1e9a5e8e, O7-REQ-a5a2d539-b2c8-42dd-83e0-a6108b5ab288, O7-REQ-c9c94faf-43bb-474c-8cf9-7ed1441e5494, O7-REQ-c1427344-798f-47f1-9dab-790fd3693cf7, O7-REQ-f25099a9-5f30-466e-939a-48ac57cbead2, O7-REQ-80d9506a-b1e5-4ac2-8d68-eb73ecfccd51；[professional_acceptance: node 精确结果接收](online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-node-release) | [节点证据接收子范围](#sr2-release-node-evidence-receiver) | SC-4..7 AC-5..7/10..11/13..17 node proof and CI separation；AC-14 独立接收[当前 epoch attestation 授权](../../product/world-infrastructure/prd.md#req-infra-release-attestation-authorization)，AC-15 独立接收[三类发布 runbook 阻断分诊](../../product/world-infrastructure/prd.md#req-infra-release-release-triage)，两者不由证据可追溯性或 submit 响应代签 | scripts/package-module-release-attestation-proof.sh + scripts/module-release-node-acceptance.sh；[node submit 局部测试定义](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/module_release_attestation_submit_api_tests.rs)：parse_module_release_attestation_submit_request_normalizes_fields 检查字段归一化，build_module_release_attestation_submit_action_payload_encodes_runtime_action 检查 RuntimeAction 字段编码，module_release_attestation_submit_handler_rejects_bad_payload 断言 HTTP 400 / invalid_request，module_release_attestation_submit_handler_accepts_valid_payload 以 NoopExecutionHook 断言 HTTP 200 / action_id / submitted_at；上述定义未运行，不证明 payload 编码失败或 queue-submit 失败负例。required conflict duplicate/threshold fail/archive drift/CI check-only；full CI-off actual release-node flow仍待实际验证；ACK及字段编码不证明 proof archive/hash/body、finality 或 activation | archived payload/request/action/manifest mappings | ACK不等于 activation；external worker absence |
| Trace refs: O7-REQ-02b10123-7422-4509-9d04-ea43444890c6, O7-REQ-c2a3ca44-d9b3-4ae0-8245-d8f496f38673, O7-REQ-8d0fe4a1-7c99-42d2-8b5d-7c4990591993, O7-REQ-0ed26c5c-b1a1-4e51-a462-56bd4712e295, O7-REQ-d01cf80e-1172-4963-8bc0-877dc9de8e00, O7-REQ-c9c94faf-43bb-474c-8cf9-7ed1441e5494, O7-REQ-80d9506a-b1e5-4ac2-8d68-eb73ecfccd51；[professional_acceptance: quality 精确结果接收](online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-release-quality) | [sr2-release-validation](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-validation) | NFR-OMR-1..7 performance/convergence/errors/security；另独立承接 [AC-12 固定基准与归档产物](online-module-release-legality-closure-2026-03-08.prd.md#sr2-release-acceptance)，不以其他 NFR 通过代替 AC-12 | fixed scripts/oasis7-runtime-finality-baseline.sh 100 signers/hot cache summary + S11 faults；measure p95/epoch windows with source/integration/tested-tree readback；[located partial source](../../../testing-manual.md) | summary.md/summary.json/logs | benchmarks未运行，本地不是full |
| Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[professional_acceptance: activation 精确结果接收](online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-version-dependency) | [sr2-release-trust](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust) | activation/migration exact boundary and replay；原 [activation 字段 authority](module-lifecycle.md#sr2-activation-contract) 与迁移合同不变 | ACT-01/MIG-01..03/SR2-RT-04..08; required before/at/after, failed second migration, old bytes replay；full proof+state-sync+consumer same window；[located partial source](../../../crates/oasis7/tests/module_lifecycle.rs) | GWSC #sr2-runtime-scenarios | 当前 prepared proposal仅partial |
| Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[product_requirement: REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) | [sr2-release-trust](#sr2-release-trust) | 仅 canonical activation proof / exact governing manifest、缺证阻断与历史原版本 replay 依赖。 | [现有局部验证手册](../../../testing-manual.md)；ACT-01 / SR2-RT-04..05 与 runtime 版本目标场景尚待同候选证据补齐，手册本身不证明这些目标已实现：固定同 candidate/world/canonical window，核对 before/at/after inclusive boundary、missing/conflicting proof 与 old artifact/schedule replay；原完整 AC-DWE-004 和 test_tier_full 仍独立适用。 | 未来同候选 activation/proof、原 block manifest/artifact、receipt/replay 对账；本轮只核文档关系，测试未运行。 | 局部 release controls 不证明 activation history、完整 BFT、待决重裁决、跨版本互斥替代或消费者全窗口；不改变 partial/unverified。 |

## 12. 决策、长期风险与未决问题
保留 DEC-OMR-001..004：online签名manifest、禁identity_hash宽松回退、external finality、无CI生产依赖。目标选择分层 proof receiver，避免build合法性被误当effect finality。Runtime/P2P/ops/QA owners分别解除完整 durable implementation、actual certificate、production materializer和full-tier proof gaps；输入版本/authority/rotation变化必须重新bind，不从本次文档合入推导能力发布。
