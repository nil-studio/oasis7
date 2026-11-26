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
<!-- 涉及代码时，按[预计 PR 边界](https://github.com/nil-studio/oasis7/blob/main/doc/engineering/workflow/source-of-truth.md#planned-pr-scope)及[切片规范](https://github.com/nil-studio/oasis7/blob/main/doc/engineering/workflow/pr-slicing-and-circuit-breaker.md)将实际 diff 对回适用的 PR k/N 计划、交付物与验收归属。调整预计 N 或范围时，先更新受影响安排和原因；计划调整不扩大当前 PR 已锁定的 Allowed-Crates。30 文件仍是任务规模建议，各类型硬边界照常核对；独立简单 PR 可直接说明本项范围，多任务分别判断。 -->

- Planned-Slice: <!-- 人工计划关联；有对应计划时填 PR k/N 及原 Issue 的 checklist 条目，未建单独立 PR 填 N/A。此字段不由边界脚本校验。 -->

<!-- 按[PR 实施 checklist](https://github.com/nil-studio/oasis7/blob/main/doc/engineering/workflow/source-of-truth.md#planned-pr-checklist)回链本片范围和验收；本 PR 未合入时保持未勾选，实际合入且本片验收满足后再更新。局部交付先对齐本项实际范围并保留未勾选接续项或明确后继任务，再核对是否可勾选；调整计划保留已完成项与证据。 -->

## 验证结果（`testing-manual.md`）

<!-- 对照本次行为与切片分级验证映射说明受测版本、可复跑命令/操作与输入、预期和实际结果，以及重要未覆盖项；按需链接证据。区分局部自测、实际组合与部署/实玩/持续观测，剩余验收注明承接方或待承接。详细输出与历史过程放评论或折叠附录。 -->
- [ ] `./scripts/check-pr-boundaries.sh` passed locally
- Commands executed & results:
  - ``

## 重要风险与剩余事项（适用时）

<!-- 仅保留影响审阅、使用或交付判断的事项。按[前置事实与剩余承接](https://github.com/nil-studio/oasis7/blob/main/doc/engineering/workflow/source-of-truth.md#decomposition-evidence-and-remainder)说明当前 PR 实际完成范围、有效暂停与未验证范围；局部交付、范围收窄或原承接退出时，引用原 Issue 当前安排中的未完成验收、承接 Issue、后续计划批次或前置与下一动作。未启动批次无需预建 PR；无适用内容时删除本节。 -->

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
