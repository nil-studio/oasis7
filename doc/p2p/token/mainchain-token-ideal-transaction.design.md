# oasis7 主链理想交易合同（设计）

- 对应需求文档: `doc/p2p/token/mainchain-token-ideal-transaction.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

## Design authority

This stable design absorbs the dated 2026-06-08 ideal-transaction design. Its purpose is a single signed metadata envelope shared by runtime, node-local re-verification, explorer, launcher and transfer-auth surfaces. It does not change consensus, custody, wallet, or fee-economics mechanisms.

## Phase 1 field and validation model

| Field group | Design rule |
| --- | --- |
| Chain identity | `chain_id` and `network_id` are live-runtime sourced, signed, and never launcher-local fallbacks. |
| Context | `tx_version=2` and `tx_type=asset_transfer` are required whenever v2 context is present. |
| Expiry | `valid_until_unix_ms` is signed and runtime rejects non-positive or expired values. |
| Asset/application | `asset_id`, `memo`, and `application_payload_hash` are signed and displayable. |
| Quote | `max_fee` is non-zero when present and requires `fee_asset_id`; both are metadata only, with no debit. |
| Client audit | `client_request_id` is signed audit/idempotency metadata only, not consensus dedupe. |

The signing payload and node-local signature rebuild use the exact same implemented fields. A submission whose live chain identity is unavailable fails before signing; structural validation rejects invalid v2 context and incomplete quote metadata.

## Surface contract

- Runtime action: `Action::TransferMainToken` carries the implemented metadata.
- Node: locally rebuilds/reverifies the same signed payload; no node-local bypass.
- Explorer/launcher: display submitted metadata for audit without requiring raw JSON reconstruction.
- Fees: `max_fee` and `fee_asset_id` are labelled quote metadata, never evidence of fee charging.

## Phase 2+ design boundary

No field may be added merely as an optional string. `fee_payer` requires separate payer authorization and debit; `sponsor` requires authorization, allowance/limits, revocation, abuse controls and audit; `priority_fee` requires mempool and execution ordering policy; real fee debit requires affordability, settlement, treasury/burn policy and explorer events. These require a separately approved fee/auth execution model.

## Rollout and residual risk

Phase 1 implementation status is limited to the signed-metadata/API scope. It does not close UI quote-label or actual-rendering acceptance: native/wasm and Chinese/English consumers still need surface-specific evidence under [itx-surface](#itx-surface). The residual risk is semantic, not an operational shortcut: presenting fee quote fields as charged fees, or future fields as supported sponsor/priority execution, is false. This design cannot be used as mainnet readiness, external-custody, or token-economics authority.

## 追踪范围与未验收风险

本次只追踪配对 PRD 已有的冻结专业合同，不重开经济或安全决定；完成范围按上文限定为 metadata/API，不将未验收 UI 算入闭环。源码字节重核固定 main `b32d4782b4d3dbcafa7c2cd0864fb16e8016bbc0`（与原静态审读源一致）；静态定义与已有测试断言仅是验证定位，本次未执行 Rust、UI、签名、真实交易、费用、部署或恢复测试。共享 runtime、publication、persistence、launcher 与 CI 在途变更不作为已验收证据；原文“已实现/complete”不因本次追踪新增而获得新验证。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [PRD-P2P-ITX-001](mainchain-token-ideal-transaction.prd.md#itx-001) | Phase 1 metadata 字段进入签名 payload，node-local 重建同形 | [Phase 1 metadata 字段进入签名 payload，node-local 重建同形](#itx-signed-metadata) | Runtime signed payload 与 Node local re-verification 主责；`consensus_action_payload.rs::main_token_transfer_from_runtime_action` / `node_runtime_core.rs::build_local_main_token_action_signing_action` | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=Phase 1 metadata 字段进入签名 payload，node-local 重建同形，不覆盖端到端产品验收 |
| [PRD-P2P-ITX-001](mainchain-token-ideal-transaction.prd.md#itx-001) | v2 context、expiry 与 quote metadata 的结构校验 | [v2 context、expiry 与 quote metadata 的结构校验](#itx-validation) | Chain-runtime transfer submit parsing / preflight 主责；`transfer_submit_api.rs::parse_transfer_submit_request` / `preflight_validate_transfer_request` | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=v2 context、expiry 与 quote metadata 的结构校验，不覆盖端到端产品验收 |
| [PRD-P2P-ITX-001](mainchain-token-ideal-transaction.prd.md#itx-001) | 提交 metadata 的 audit 展示及 quote 标识 | [提交 metadata 的 audit 展示及 quote 标识](#itx-surface) | Chain-runtime explorer metadata 投影及 launcher 展示主责；`transfer_submit_api.rs` tracker 与 `transfer_submit_api_explorer_tests.rs` JSON 契约；UI 呈现独立验收 | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=提交 metadata 的 audit 展示及 quote 标识，不覆盖端到端产品验收 |
| [PRD-P2P-ITX-002](mainchain-token-ideal-transaction.prd.md#itx-002) | live chain/network identity 与签名前 fail-fast | [live chain/network identity 与签名前 fail-fast](#itx-live-identity) | Client launcher snapshot ingestion / chain status / interactive transfer 主责；`main_tests.rs` snapshot 用例、`main_chain_status.rs` 失联失效、`transfer_window.rs::submit_transfer` guard 分别核验 | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=live chain/network identity 与签名前 fail-fast，不覆盖端到端产品验收 |
| [PRD-P2P-ITX-003](mainchain-token-ideal-transaction.prd.md#itx-003) | Phase 2+ 独立 fee/auth 执行设计审批边界 | [Phase 2+ 独立 fee/auth 执行设计审批边界](#itx-phase2-gate) | 配对 PRD `PRD-P2P-ITX-003` 的独立 fee/auth execution 设计主责；本文件不授权未来 payer/sponsor/priority 执行 | 产品层不适用；reason=本关系仅追踪既有专业内部合同，不承接玩家价值、治理资格或世界权利；scope=Phase 2+ 独立 fee/auth 执行设计审批边界，不覆盖端到端产品验收 |

## 冻结设计条款与验证边界

<a id="itx-signed-metadata"></a>
### Phase 1 metadata 字段进入签名 payload，node-local 重建同形

承接 [PRD-P2P-ITX-001](mainchain-token-ideal-transaction.prd.md#itx-001)。签名字段一致性；不将 client_request_id 提升为 consensus dedupe。验证限界：`build_transfer_submit_action_payload_carries_v2_context_fields` 断言填入的 metadata 并验证 auth；`consensus_action_payload.rs::legacy_runtime_wrapper_preserves_contextual_v2_vector` 另有固定向量，不证明每字段变异拒绝矩阵。

<a id="itx-validation"></a>
### v2 context、expiry 与 quote metadata 的结构校验

承接 [PRD-P2P-ITX-001](mainchain-token-ideal-transaction.prd.md#itx-001)。tx_version=2、tx_type=asset_transfer；非正/过期 expiry、max_fee=0、缺 fee_asset_id 的拒绝边界。验证限界：`preflight_transfer_rejects_expired_v2_request` 断言 expired；`parse_transfer_submit_request_rejects_non_main_token_asset_id` 断言非法 asset；其余结构边界须单独核对源码/补证。

<a id="itx-surface"></a>
### 提交 metadata 的 audit 展示及 quote 标识

承接 [PRD-P2P-ITX-001](mainchain-token-ideal-transaction.prd.md#itx-001)。复用原 Surface contract；max_fee/fee_asset_id 仅报价，无自动扣款或结算。验证限界：现有 explorer p0/p1 测试断言 JSON metadata；UI label/quote wording/真实渲染仍未由此验收。

<a id="itx-live-identity"></a>
### live chain/network identity 与签名前 fail-fast

承接 [PRD-P2P-ITX-002](mainchain-token-ideal-transaction.prd.md#itx-002)。原字段模型明确 identity 不可 launcher-local fallback；不可用即拒绝签名。验证限界：`main_tests.rs::apply_web_snapshot_marks_control_plane_snapshot_received` 仅断言 snapshot identity 入 app；`main_chain_status.rs` 内联 tests 断失联清除及 stale poll；`transfer_window.rs::submit_transfer` 缺 identity 的签名前 guard 仅静态核查，直接缺失值提交测试尚未定位，仍为 planned 待补验证，不视为通过。

<a id="itx-phase2-gate"></a>
### Phase 2+ 独立 fee/auth 执行设计审批边界

承接 [PRD-P2P-ITX-003](mainchain-token-ideal-transaction.prd.md#itx-003)。保留 payer authorization/debit、sponsor allowance/revocation/abuse/audit、priority policy 与真实费用结算的独立前置设计。验证限界：人工核对 Explicit non-goals and future gate；未批准、未执行未来 payer/sponsor/priority 或结算行为。

### 11.1 验证映射表

| 上游 requirement / professional acceptance | 本设计条款 | 验证义务 | 验证 source / 方法 | 本次结果与证据边界 | 未覆盖范围 |
| --- | --- | --- | --- | --- | --- |
| [PRD-P2P-ITX-001](mainchain-token-ideal-transaction.prd.md#itx-001) | [Phase 1 metadata 字段进入签名 payload，node-local 重建同形](#itx-signed-metadata) | Phase 1 metadata 字段进入签名 payload，node-local 重建同形 | [静态核对定位](../../../crates/oasis7_chain_runtime/src/host/transfer_submit_api_tests.rs)；`build_transfer_submit_action_payload_carries_v2_context_fields` 断言填入的 metadata 并验证 auth；`consensus_action_payload.rs::legacy_runtime_wrapper_preserves_contextual_v2_vector` 另有固定向量，不证明每字段变异拒绝矩阵 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
| [PRD-P2P-ITX-001](mainchain-token-ideal-transaction.prd.md#itx-001) | [v2 context、expiry 与 quote metadata 的结构校验](#itx-validation) | v2 context、expiry 与 quote metadata 的结构校验 | [静态核对定位](../../../crates/oasis7_chain_runtime/src/host/transfer_submit_api_tests.rs)；`preflight_transfer_rejects_expired_v2_request` 断言 expired；`parse_transfer_submit_request_rejects_non_main_token_asset_id` 断言非法 asset；其余结构边界须单独核对源码/补证 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
| [PRD-P2P-ITX-001](mainchain-token-ideal-transaction.prd.md#itx-001) | [提交 metadata 的 audit 展示及 quote 标识](#itx-surface) | 提交 metadata 的 audit 展示及 quote 标识 | [静态核对定位](../../../crates/oasis7_chain_runtime/src/host/transfer_submit_api_explorer_tests.rs)；`explorer_p0_blocks_txs_tx_search_queries_return_expected_payloads` 与 `explorer_p1_endpoints_return_expected_payloads` 断言 JSON metadata；UI label/quote wording/真实渲染仍未由此验收 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
| [PRD-P2P-ITX-002](mainchain-token-ideal-transaction.prd.md#itx-002) | [live chain/network identity 与签名前 fail-fast](#itx-live-identity) | live chain/network identity 与签名前 fail-fast | [静态核对定位](../../../crates/oasis7_client_launcher/src/main_tests.rs)；`main_tests.rs::apply_web_snapshot_marks_control_plane_snapshot_received` 仅断言 snapshot identity 入 app；`main_chain_status.rs` 内联 tests 断失联清除及 stale poll；`transfer_window.rs::submit_transfer` 缺 identity 的签名前 guard 仅静态核查，直接缺失值提交测试尚未定位，仍为 planned 待补验证，不视为通过 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
| [PRD-P2P-ITX-003](mainchain-token-ideal-transaction.prd.md#itx-003) | [Phase 2+ 独立 fee/auth 执行设计审批边界](#itx-phase2-gate) | Phase 2+ 独立 fee/auth 执行设计审批边界 | N/A；reason=本条只冻结尚未批准的 Phase 2+ 范围，当前不存在本条承诺的 payer/sponsor/priority 执行能力可验收；scope=仅未来 fee/auth execution 审批前置边界，不豁免 Phase 1 metadata 验证；owner_role=fee/auth 执行设计主责；evidence_ref=[既有 Phase 2+ gate](mainchain-token-ideal-transaction.prd.md#explicit-non-goals-and-future-gate)；re-evaluate=单独 fee/auth 执行模型获批或提出新增执行支持时重新建立实际测试映射 | 只读静态复核；未执行行为回归或验收，结果须绑定实际候选与场景 | 不证明当前在途实现、完整恢复、mainnet readiness、外部 custody 或产品端到端闭环 |
