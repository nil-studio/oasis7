# scripts PRD 文件级索引

审计轮次: 6

更新时间：2026-04-14

## 入口
- 模块 PRD：`doc/scripts/prd.md`
- 模块设计总览：`doc/scripts/design.md`
- 模块执行记录：对应 PR、实际 CI 与评审记录（Issue 按需）
- governance 归并说明：`doc/scripts/governance/README.md`
- pre-commit 专题路由：`doc/scripts/precommit/README.md`
- WASM 发布级 pipeline：`doc/world-runtime/wasm/wasm-deterministic-build-pipeline.prd.md`
- WASM 历史追溯：`doc/world-runtime/wasm/evidence.md#historical-nightly-build-std-provenance`

| 专题 PRD | 专题设计文档 | 执行记录 |
| --- | --- | --- |
| `doc/scripts/precommit/pre-commit.prd.md` | 操作契约已收口于 PRD，不再维护独立 design | 对应 PR、实际 CI 与评审记录（Issue 按需） |

## 说明
- 本索引确保模块专题文档在根入口文档树中可达。
- 通用脚本入口、参数、worktree harness、bootstrap、PR closure 与 local landing 的稳定语义已归入 `doc/scripts/{prd,design}.md`；可变任务状态与执行证据归入实际 PR/已有 Task，Issue 按需。
- 专题稳定语义按实际职责使用 PRD、design 与 evidence/runbook；按[专业设计关系规范](../engineering/doc-governance/system-design-writing-standard.design.md#product-system-design-relations)链接适用义务，不要求固定文件组合或一对一任务配对。
- 首次选择 pre-commit 的当前门禁契约或失败修复流程时，先读
  `doc/scripts/precommit/README.md`；本页保留精确文件检索。
- WASM 当前发布要求由 world-runtime canonical pipeline 承接；已吸收的 nightly/build-std 参数、CI 约束与 hash 漂移背景只在上述 evidence historical provenance 中追溯，不定义 Docker-only publishable build、receipt、single canonical token 或 cross-host evidence。
