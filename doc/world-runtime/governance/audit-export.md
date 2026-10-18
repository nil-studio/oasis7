# oasis7 Runtime：审计导出规范（设计分册）

审计轮次: 4

本分册为 `doc/world-runtime/prd.md` 的详细展开。2026-03-04 实现口径保留为当时记录；它不代表本次已执行导出或确认当前部署。

当前接口与筛选的源码入口为 [World::save_audit_log](../../../crates/oasis7_runtime/src/world/audit.rs) 与 [AuditFilter / AuditEventKind](../../../crates/oasis7_runtime/src/audit.rs)。下方事件名按 `baded561af0808d9d5cb154d8acc68efe54e80db`（2026-10-13）枚举静态核对；后续新增/变更应以消费代码同版本的枚举与 serde 注解为完整权威，不能把此列表当作跨版本不变协议。

## 当前实现口径（2026-03-04）
- 导出入口：`World::save_audit_log(path, filter)`。
- 导出内容：`filter` 命中的 `WorldEvent` JSON 数组。
- 当前实现不支持 `limit/cursor` 分页导出，也不支持多文件 chunk 导出。

## 导出接口（当前）

```rust
pub fn save_audit_log(
    &self,
    path: impl AsRef<Path>,
    filter: &AuditFilter,
) -> Result<(), WorldError>
```

- `path`：输出 JSON 文件路径。
- `filter`：审计过滤条件。

## AuditFilter 字段（当前）
- `kinds`: 可选事件类型列表（`AuditEventKind`）。
- `from_time` / `to_time`: 时间区间。
- `from_event_id` / `to_event_id`: 事件 ID 区间。
- `caused_by`: `action` / `effect`。

## AuditEventKind（当前）

序列化名称遵循 `#[serde(rename_all = "snake_case")]`。本列表仅是上述固定版本的名称参考；字段与治理 payload 仍回到源码，不以字段对照代签解析、筛选或导出测试。
- `domain`
- `effect_queued`
- `receipt_appended`
- `capability_authorization`
- `policy_decision`
- `rule_decision`
- `action_overridden`
- `governance`
- `module_event`
- `module_call_failed`
- `module_emitted`
- `module_state_updated`
- `module_runtime_charged`
- `product_validation_delivery_cursor_updated`
- `snapshot_created`
- `manifest_updated`
- `rollback_applied`

## 导出示例（当前语义）
```json
[
  {
    "id": 1,
    "time": 100,
    "caused_by": null,
    "body": {
      "kind": "Governance",
      "payload": {
        "type": "Proposed",
        "data": {
          "proposal_id": 1,
          "author": "agent:alpha",
          "base_manifest_hash": "example-base-manifest-hash",
          "manifest": {
            "version": 1,
            "content": {}
          },
          "patch": null
        }
      }
    }
  }
]
```

本例对应 `Vec<WorldEvent>` 的 JSON 形状：`ProposalId` 为 `u64`，`GovernanceEvent::Proposed` 必须包含完整 `Manifest { version, content }`，无 patch 时显式写 `null`。`base_manifest_hash` 是示例字符串，不是某次真实导出或有效治理执行的证据。维护示例时应按上述 Rust 类型执行离线解析/序列化回读；JSON 语法检查或字段对照不能代签该 round-trip 已运行。

## 待补齐能力（未实现）
- 分页导出（`limit/cursor`）。
- 导出元信息包装（`export_id`、`start/end_event_id`）。
- 多文件分片导出。

以上能力若落地，应先更新 `runtime/world/audit.rs` 与测试，再回写本分册。
