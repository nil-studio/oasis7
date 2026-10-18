# oasis7 Runtime：治理事件与 Shadow 报告（设计分册）

审计轮次: 4

本分册为 `doc/world-runtime/prd.md` 的详细展开。2026-03-04 记录与旧示例保留为历史解释，不是现行事件的完整序列化参考。

## 当前查询入口与范围

完整变体、字段类型、默认值与 serde 表示以 [GovernanceEvent](../../../crates/oasis7_runtime/src/governance.rs)、[WorldEventBody](../../../crates/oasis7_runtime/src/world_event.rs) 及 [AuditEventKind](../../../crates/oasis7_runtime/src/audit.rs) 为准，使用时对齐消费代码的同一版本。

2026-10-13 对 `baded561af0808d9d5cb154d8acc68efe54e80db` 的静态核对表明，治理枚举除下面四种历史示例外，还包含 Queued、FinalityEpochSnapshotSet/Removed、EmergencyBrakeActivated/Released、EmergencyVetoed、IdentityPenaltyApplied/Appealed/Resolved、RestrictedStarterClaimAdminRegistryUpdated 及 ValidatorAdmissionSubmitted/Approved/Activated/Revoked。此处用于说明旧例非穷举，不另建一套完整字段权威；不声称已运行导出、回放或 serde round-trip。

治理事件使用 `type` / `data` 包装；下方 JSON 仅为旧 payload 示意，含占位值，不是可直接导入的完整 WorldEvent。可复制导出示例及其局限见 [audit-export.md](audit-export.md)。

<a id="当前实现口径2026-03-04"></a>
## 历史实现口径（2026-03-04）
- 治理事件使用 `GovernanceEvent` 枚举统一承载：`Proposed` / `ShadowReport` / `Approved` / `Applied`。
- world 事件体中治理相关入口为 `WorldEventBody::Governance(GovernanceEvent)`。
- 模块运行失败当前统一进入 `WorldEventBody::ModuleCallFailed`，未拆分 `ModuleLoadFailed` / `ModuleValidationFailed` 事件体。
- `ShadowReport` 事件当前仅记录提案标识与影子 hash，不内嵌富报告结构。

<a id="governanceevent-负载结构当前"></a>
## GovernanceEvent 负载结构（历史示例子集）

### Proposed
```json
{
  "proposal_id": "...",
  "author": "...",
  "base_manifest_hash": "...",
  "manifest": { "...": "..." },
  "patch": { "...": "..." }
}
```

### ShadowReport
```json
{
  "proposal_id": "...",
  "manifest_hash": "..."
}
```

### Approved
```json
{
  "proposal_id": "...",
  "approver": "...",
  "decision": "approve | reject"
}
```

### Applied
```json
{
  "proposal_id": "...",
  "manifest_hash": "... (optional)",
  "consensus_height": 123,
  "threshold": 5,
  "signer_node_ids": ["node-a", "node-b"]
}
```

<a id="模块失败事件口径当前"></a>
## 模块失败事件口径（2026-03-04 记录）
- 当前失败事件类型：`ModuleCallFailed`。
- 典型失败来源：
  - 沙箱执行 Trap/OutOfFuel/Interrupted。
  - 输出超限、效果上限超限、无效输出。
  - 模块加载失败（例如工件缺失）在调用阶段映射为 `ModuleCallFailed(code=SandboxUnavailable)`。
- 若后续需要独立 `ModuleLoadFailed` / `ModuleValidationFailed` 事件，应在 `WorldEventBody` 与 `AuditEventKind` 同步扩展后再回写本分册。

<a id="审计关联当前"></a>
## 审计关联（2026-03-04 记录）
- 审计维度通过 `AuditEventKind` 过滤：
  - 治理链路：`governance`
  - 模块失败：`module_call_failed`
  - 模块生命周期事件：`module_event`
- 因果链通过 `caused_by` 保留 action/effect 关联。

## 实现锚点
- `crates/oasis7_runtime/src/governance.rs`
- `crates/oasis7_runtime/src/world_event.rs`
- `crates/oasis7_runtime/src/audit.rs`
