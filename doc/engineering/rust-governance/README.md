# Rust 工程治理入口

本目录维护 Rust 文件结构、workspace 共性、架构依赖与协议产物的工程契约：

- [Rust 1200 行与结构切片治理](rust-1200-line-root-cause-governance-2026-03-29.prd.md)
- [Rust 工程契约需求](rust-engineering-contracts.prd.md)：Workspace 治理、架构边界和协议产物的范围与验收。
- [Rust 工程契约系统设计](rust-engineering-contracts.design.md)：配置等价、方向矩阵、生成生命周期及验证；目标设计不表示新检查已启用。
- [Rust 工程契约验证手册](rust-engineering-contracts.manual.md)：工具就绪后的逐项验收方法、真实正反例和证据范围；不记录执行完成态。

文件体量的扫描结果由 `scripts/check-rust-file-size.sh` 产生，required gate 通过
`scripts/ci-tests.sh required` 调用该脚本。文档不复制扫描清单或完成任务
ledger，避免一次性 burn-down 记录重新成为当前规则。

Workspace 与协议治理按设计中的实施范围逐步落地，当前能力与缺口见专题基线。
通用开发和合入条件仍由[开发流程规范](../workflow/source-of-truth.md)主责；
交付事实保留在 PR、实际 CI 与评审记录。

2026-02 round3、2026-03 burn-down 批次及已删除 baseline 的历史证据保留在
GitHub task、Git history 与 git history；它们不是 active task
入口，也不在本目录恢复。
