# oasis7 Runtime：测试与基架建议（设计分册）

审计轮次: 4

本分册为 `doc/world-runtime/prd.md` 的详细展开。

## 集成测试用例（草案）

- **register_happy_path**：artifact 写入 → propose → shadow pass → approve → apply → 注册表更新
- **shadow_fail_blocks_apply**：shadow fail → reject → 不产生模块事件
- **apply_fail_records_validation**：apply 阶段校验失败 → `ModuleCallFailed`（带拒绝原因）→ 注册表不更新
- **upgrade_flow**：正常升级只更新目标实例的版本绑定，保留其 ID、owner、target 和独立 state；A/B 同在 v1 时，A 升到 v2 不应冻结 v1 或迁移 B，snapshot/replay 后 B 仍可复用合法的同一 v2 完整 manifest 升级。按[实例升级与注册复用](../module/module-lifecycle.md#instance-upgrade-version-reuse)保留目标与当前版本相同、manifest 不一致、错 owner/from、冻结、不兼容和无效 finality 的拒绝用例。
- **rollback_admission_freeze**：回滚与正常升级分开验证；按[有限回滚](../module/player-published-entities.design.md#ppe-rollback-disposition)仅冻结被回滚 FROM release 的 exact `(module_id, from_module_version, from_wasm_hash)` 的新准入，不以 TO hash 或“旧版本”泛化冻结；既有实例、profile 与历史事实保留，失败不得留下部分回滚/冻结结果。
- **audit_export_contains_module_events**：审计导出包含 GovernanceEvent + Module*Failed/ShadowReport

## 测试基架建议（草案）

- 文件组织：`crates/oasis7_runtime/tests/module_lifecycle.rs`
- 共享夹具：`TestWorldBuilder`（构造 world + manifest + registry 初始态）
- 伪造工件：内存内生成 dummy wasm bytes + 计算 hash
- Shadow 注入：允许在测试中强制 shadow 失败/通过
- 断言：事件流顺序、注册表内容、审计导出记录

**TestWorldBuilder（示意 API）**
```rust
struct TestWorldBuilder {
    with_registry: bool,
    with_manifest: bool,
    caps: Vec<CapabilityGrant>,
}

impl TestWorldBuilder {
    fn new() -> Self;
    fn with_registry(mut self) -> Self;
    fn with_manifest(mut self) -> Self;
    fn with_caps(mut self, caps: Vec<CapabilityGrant>) -> Self;
    fn build(self) -> World;
}
```

**Dummy WASM 工件工具（示意）**
```rust
fn dummy_wasm_bytes(label: &str) -> Vec<u8>;  // 生成确定性字节
fn wasm_hash(bytes: &[u8]) -> String;         // 计算内容哈希（sha256/hex）
```

**Shadow 注入建议（示意）**
- 在 `WorldConfig` 或测试构造器中注入 `ShadowPolicy`（`AlwaysPass` / `AlwaysFail` / `ByModuleId(HashSet)`）。
- 测试中将 `ShadowPolicy` 设置为 `AlwaysFail` 以触发 shadow reject 分支。

**审计与可回放**
- 注册/激活/升级事件进入日志，`module_registry.json` 可由事件重建。
- `wasm_hash=H` 只定位内容 bytes；模块注册身份由 `module_id/version` 与完整 manifest 区分。按[两种身份与共享 bytes](../module/module-lifecycle.md#module-retirement-and-shared-content)，恢复须核对所选 immutable generation 的完整 registry/manifest 集及内容引用，不能用一个 H 槽覆盖共享 bytes 的不同注册记录。
- 共享 H 正例分别覆盖同模块两版本、不同模块的完整 manifest，经标准 writer 保存再由真实 reader/Observer 恢复；覆盖单 active/全 inactive 与支持的 inline/non-inline，读取顺序不得改变结果。错 manifest/hash、坏 bytes、损坏或不支持的旧格式保持拒绝；Observer 不改写源目录或借较新 cache 修补旧 checkpoint。完整矩阵及退休兼容边界见[生命周期恢复验证](../module/module-lifecycle.md#module-lifecycle-recovery-validation)。
- 上述场景是测试建议与目标合同，不表示相关实现或测试已通过；共享内容恢复与正常升级/回滚须分别验收。
