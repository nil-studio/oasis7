# headless-runtime 文档索引（原 nonviewer）

审计轮次: 10

## 说明
- 模块目录已从旧名称 `nonviewer` 重命名为 `headless-runtime`。
- 已完成的历史 `nonviewer-*` hardening 三件套不再保留为活跃文档；稳定鉴权、长稳与归档边界已收敛到本模块 `prd.md` / `design.md` / PR、实际 CI 与评审记录（Issue 按需），历史实施可从 Git history 与这些执行记录追溯。
- 不再保留 `doc/headless-runtime/archive/` 归档目录。
- 长稳内存、CAS 冷归档与 replay/GC 的 runtime 实现合同另见 `doc/world-runtime/runtime/runtime-storage-footprint-governance.prd.md`；旧专题仅通过固定提交 `25fbcd7cf590c5e1b1248119d407cc7c34e8e634`、PR 与实际验证记录追溯，不恢复旧 split-crate authority。

## 入口
- PRD: `doc/headless-runtime/prd.md`
- 设计总览: `doc/headless-runtime/design.md`
- 当前任务与执行证据: 对应 PR、实际 CI 与评审记录；Issue/Project 按需
- 文件级索引: `doc/headless-runtime/prd.index.md`

## 从这里开始
- 想先确认 headless-runtime 当前职责、生命周期边界与发布接口：先读 `doc/headless-runtime/prd.md`。
- 想确认模块是否还有活跃执行项及最近一次完成的范围：读对应 PR/已有 Task、实际 CI 与评审证据。
- 想理解旧 `nonviewer` 命名及已退役专题的追溯边界：读上面的“说明”。
- 想查生命周期 / 鉴权一致性自检入口：先读 `doc/headless-runtime/checklists/lifecycle-auth-consistency-checklist.md`。
- 想查长稳归档、事故追溯或 release gate 对接模板：进入 `doc/headless-runtime/templates/`。

## 模块职责
- 维护无界面运行链路的生命周期、鉴权与长稳追溯口径。
- 在模块根 authority 中维护鉴权、防重放、长稳内存边界与冷归档合同。
- 承接与 testing / core 的 headless 证据链和发布门禁对接口径。

## 主题文档
- `checklists/`：生命周期 / 鉴权一致性检查清单。
- `templates/`：长稳归档、事故追溯与 release gate 对接模板。

## 根目录收口
- 模块根目录主入口保留：`README.md`、`prd.md`、`design.md`、`prd.index.md`；过程记录由实际 PR/已有 Task 承接。
- 其余专题文档按主题下沉到 `checklists/`、`templates/`。
- 2026-03-11 模块状态 closure / handoff root 文档已退役删除；当前状态与下一任务入口以对应 PR/已有 Task 的实际记录为准，Issue/Project 按需。

## 维护约定
- 无界面运行链路行为变更时，更新受影响的长期要求、设计与验证方法；执行结果留在实际 PR/已有 Task，分工见[开发流程规范](../engineering/workflow/source-of-truth.md#information-record-placement)。
- 新文档使用 `headless-runtime-*` 前缀；已吸收的历史 `nonviewer-*` slug 仅通过 Git history、PR 与实际验证记录追溯。
- 新增专题后，需同步回写 `doc/headless-runtime/prd.index.md` 与本目录索引。
- README 负责解释命名迁移与模块级入口顺序，不替代 `checklists/`、`templates/` 或 `prd.index.md` 的详细内容。
