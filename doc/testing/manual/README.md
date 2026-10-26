# testing manual 文档入口

## 从这里开始

- 想先判断测试分层、门禁与通用执行顺序：读仓库级 [`testing-manual.md`](../../../testing-manual.md)。
- 想采样 Viewer Web 页面、保存截图 / state / console 证据：读 [`web-ui-agent-browser-closure-manual.manual.md`](web-ui-agent-browser-closure-manual.manual.md)。
- 想跑真实本地栈、真实 UI 输入与玩家流程矩阵：读 [`web-ui-playwright-closure-manual.manual.md`](web-ui-playwright-closure-manual.manual.md)。
- 想验证像素世界的原操作归属、授权分页/时效、探索跟随、图形恢复、可访问性、五秒首读、容量与 Web/native 同义：读 [`pixel-world-visualization-validation.manual.md`](pixel-world-visualization-validation.manual.md)。这是验证方法，不是当前通过报告；它细化而不替代外部 Runtime 的 viewer-* 组合场景。
- 想让本地入口接入 formal `public_testnet` world state，或排查 LetAI 配置、代理继承、模型选择、quota 与受控 auto-topup：读 [`local-public-testnet-letai-test-environment-2026-06-23.manual.md`](local-public-testnet-letai-test-environment-2026-06-23.manual.md)。
- 想在受治理 deployment truth 下只物化一台已擦除 validator host、保持服务 disabled/inactive 并输出 public receipt：先读 blockchain ops authority [`../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md`](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md)，再按 [`public-testnet-fresh-validator-host-bootstrap-2026-07-28.manual.md`](public-testnet-fresh-validator-host-bootstrap-2026-07-28.manual.md) 执行其 narrow subprocedure。
- 想用截图加模型视觉评审替代 routine 人工 review：读 [`model-visual-review-sop-2026-05-29.manual.md`](model-visual-review-sop-2026-05-29.manual.md)，并使用 [`../templates/model-visual-review-card-template.md`](../templates/model-visual-review-card-template.md)。

- 想在单台 ECS 上通过 CI / OSS 自动更新 Preview、保留现有世界并验证 / 恢复：读 [`single-ecs-preview-auto-update.manual.md`](single-ecs-preview-auto-update.manual.md)。

## 文档分工

| 需求 | 权威入口 | 不负责的内容 |
| --- | --- | --- |
| 仓库级测试层级、required/full 与通用操作 | `testing-manual.md` | 专项 Web UI 操作步骤 |
| Viewer Web 页面闭环与留证 | `web-ui-agent-browser-closure-manual.manual.md` | launcher 控制面产品动作、public testnet attach 证明 |
| 真实本地栈的 Playwright 玩家流程 | `web-ui-playwright-closure-manual.manual.md` | 通用 agent-browser 页面采样 |
| 像素世界的数据/体验/恢复/质量与逐端验收 | `pixel-world-visualization-validation.manual.md` | 新增通用 CI gate、模拟运行成绩、付费模型或完整发行放行 |
| 本地入口接入 formal public testnet | `local-public-testnet-letai-test-environment-2026-06-23.manual.md` | 纯 local-only LetAI playtest |
| 已擦除 validator host 的 no-start 物化 | `doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md`，再进入 `public-testnet-fresh-validator-host-bootstrap-2026-07-28.manual.md` | topology 审批、服务启用、网络 readiness 或 release claim |
| 截图的模型视觉评审 | `model-visual-review-sop-2026-05-29.manual.md` | runtime / security / release 放行结论 |

## 规格与追溯

- 已完成的系统测试手册工程化专题三件套已被 `testing-manual.md`、testing 根 PRD 与证据入口吸收并删除；`PRD-TESTING-MANUAL-001..003`、TMAN/DEC 历史映射保留在 `../prd.md`，实现过程从 Git history 与 PR 与实际验证记录追溯。
- `web-ui-agent-browser-closure-manual.prd.md` 与 PR、实际 CI 与评审记录（Issue 按需）分别保留 Web UI 闭环的需求 / 执行真值；对应 `*.manual.md` 承担实际步骤。
- `web-ui-playwright-closure-manual.design.md` 是该手册系列的 historical/shared design companion；真实操作从 `web-ui-playwright-closure-manual.manual.md` 进入。它仍受索引与审计留痕引用，不能当作无引用腐旧文件删除。

## 维护边界

- 新增或调整 operator 手册时，先更新本页的按问题分流；再在 `doc/testing/README.md`、`doc/testing/prd.index.md` 保持子树入口可达。
- 手册不得把 pure local playtest 与 formal public-testnet attach 证明混为一谈；具体边界以对应 runbook 为准。
- 历史设计 companion、PRD 和 project 只有在已被现行真值替代且 focused 引用审计确认无活跃调用时才能退役删除。

- 组织解散、不活跃保护、授权交接及恢复验收：[组织连续性系统验收计划](organization-continuity-system-acceptance.manual.md)（未执行）。
