<!-- 按 [PR 描述要求](https://github.com/nil-studio/oasis7/blob/main/doc/engineering/workflow/source-of-truth.md#pr-description)与[切片及熔断规范](https://github.com/nil-studio/oasis7/blob/main/doc/engineering/workflow/pr-slicing-and-circuit-breaker.md)填写实际内容。简单变更可合并相邻栏目；删除无关的可选栏目。 -->

## Boundary Metadata (Machine-Checked)

- Slice-Type: <!-- Dark | Adapter | Cutover | Bugfix | Docs | Chore -->
- Allowed-Crates: <!-- comma-separated crate names locked at Draft PR creation, e.g. oasis7_simulator, oasis7_world_state (use 'none' for docs/scripts) -->
- Active-Path-Impact: <!-- None (Unwired / Additive Only) OR list exact entrypoints/routes changed -->
- Related-Issue: <!-- #issue_number -->

## 问题与目的（Scope Contract & Summary）

<!-- 简短说明原来的问题或能力缺口、影响及本次目标。按需关联 Issue。 -->
- **In-Scope:**
- **Out-of-Scope (Non-Goals):**

## 本次改动与结果

<!-- 按行为、能力或规则说明具体变化及改后结果。文档 PR 写清确定了什么设计或方法、实现处于什么状态；文件清单不能代替说明。 -->
<!-- 涉及代码时，按[小而完整的 PR](https://github.com/nil-studio/oasis7/blob/main/doc/engineering/workflow/source-of-truth.md#small-complete-pr)、[预计 PR 边界](https://github.com/nil-studio/oasis7/blob/main/doc/engineering/workflow/source-of-truth.md#planned-pr-scope)、[任务规模建议](https://github.com/nil-studio/oasis7/blob/main/doc/engineering/workflow/source-of-truth.md#code-task-file-limit)及[切片与熔断规范](https://github.com/nil-studio/oasis7/blob/main/doc/engineering/workflow/pr-slicing-and-circuit-breaker.md)结合 diff 和关联批次说明任务范围：默认对齐关联代码 Issue 预定的 PR 边界与 `Slice-Type` / `Allowed-Crates` 白名单。多任务分别判断，纯文档变更填写 `Allowed-Crates: none`。 -->

## 验证结果（`testing-manual.md`）

<!-- 对照本次行为与切片分级验证映射说明受测版本、可复跑命令/操作与输入、预期和实际结果，以及重要未覆盖项；按需链接证据。区分局部自测、实际组合与部署/实玩/持续观测，剩余验收注明承接方或待承接。详细输出与历史过程放评论或折叠附录。 -->
- [ ] `./scripts/check-pr-boundaries.sh` passed locally
- Commands executed & results:
  - ``

## 重要风险与剩余事项（适用时）

<!-- 仅保留影响审阅、使用或交付判断的事项，包括有效暂停和必要的未验证范围；无适用内容时删除本节。 -->

## Requirement trace (when applicable)

<!-- 新增或实质修改跨层要求时，按 [追踪规范](https://github.com/nil-studio/oasis7/blob/main/doc/engineering/doc-governance/cross-layer-requirements-traceability.design.md)引用已有要求 ID；新要求只在主责条款声明。链接要求原文与适用设计，说明本 PR 覆盖范围；正文已有的验证和剩余项直接引用。层级尚未形成或不适用时简述原因。本 PR 可直接作为 Task，无需另建 Issue 或任务 ID；不涉及此类要求时删除本节。 -->

- Trace refs, requirement declaration and applicable design links:
- Scope covered by this PR, validation and remaining work:

## Reward Review Intake

<!-- 不申请早期贡献奖励评审时删除本节；只有申请时才将 Request reward review 填为 yes。保留标题和字段名供导入工具识别；Reward Account 是唯一必填的奖励收款字段，不使用原始 public_key 作为申领人或展示名。 -->

- Request reward review:
- Reward Account:
- Evidence / context link:
- Notes:
