# world-runtime governance 文档导航

本目录收敛 runtime 治理事件、审计导出与已完成的收据安全专题；本页是该簇唯一的首读入口。

## 按问题进入

- 想确认 runtime 当前治理、审计与回放边界：先读 [`../prd.md`](../prd.md)；实现契约以 `crates/oasis7_runtime/src/` 与其测试为准。
- 想确认 runtime 工作与验收记录：读对应 PR、实际 CI/测试与评审记录；Issue / Project 按需跟踪。
- 想查当前完整治理事件与字段：直接读 [GovernanceEvent](../../../crates/oasis7_runtime/src/governance.rs) 和 [WorldEventBody](../../../crates/oasis7_runtime/src/world_event.rs)；[`governance-events.md`](governance-events.md) 保留 2026-03-04 历史解释与示例子集，不是现行完整枚举。
- 想按需了解审计导出的筛选与 JSON 文件输出：读 [`audit-export.md`](audit-export.md)，完整类型与序列化名称回到 [AuditFilter / AuditEventKind](../../../crates/oasis7_runtime/src/audit.rs)；该分册不是默认运行时或发布入口，示例不证明导出已运行。
- 想追溯已完成的多节点治理、收据签名与最终性绑定专题：从 [`zero-trust-governance-receipt-hardening-2026-02-26.prd.md`](zero-trust-governance-receipt-hardening-2026-02-26.prd.md) 进入同名 PRD / design，并按 PR 与实际验证记录 / Git history 追溯实施过程。

## 阅读面与保留边界

- 当前模块规格、交付事实与实现行为分别由 `../prd.md`、Git/PR/实际 CI/评审记录和源码/测试承担；本目录不复制这些主入口的长表或发布判断。
- `zero-trust-governance-receipt-hardening-2026-02-26.*` 已完成，保留为专题决策与审计追溯，不作为新的 active task 入口。
- 本轮未删除 `audit-export.md` 或 `governance-events.md`：前者仍对应 `World::save_audit_log` 和 runtime 审计测试，后者仍有治理事件与失败解释的历史价值。二者没有可证明的现行文档替代物，不能仅因文件日期较早而删除。

## 维护规则

- 新的治理专题先落入本目录；若形成活跃 PRD / design authority 对，再同步 `../prd.index.md`。
- 只有在现行模块入口不再引用、并有明确替代真值与 repository-wide caller scan 证据时，才可删除本目录的历史材料。
