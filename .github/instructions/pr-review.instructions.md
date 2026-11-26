---
applyTo: "**"
---

## PR review

- review 前先了解业务背景
- 知道为什么这么改，再确认改得对不对
- 按[PR 描述要求](../../doc/engineering/workflow/source-of-truth.md#pr-description)核对正文能否独立说明问题、实际变化、验证覆盖和重要剩余项，并与当前 diff 及证据一致；文档改动须说明设计或规则变化及真实实现状态。
- 涉及代码时按[小而完整的 PR](../../doc/engineering/workflow/source-of-truth.md#small-complete-pr)、[预计 PR 边界](../../doc/engineering/workflow/source-of-truth.md#planned-pr-scope)、[切片与熔断规范](../../doc/engineering/workflow/pr-slicing-and-circuit-breaker.md)与[任务规模建议](../../doc/engineering/workflow/source-of-truth.md#code-task-file-limit)对照关联 Issue 预定的 `PR k/N` 切片表双向核对粒度：核查 `Slice-Type`、`Allowed-Crates`、`Planned-Slice` 与预定切片是否一致，拦截无计划拆微 PR、跨片打包、越出白名单 Crate、在同一子任务并行挂出多个兄弟开放 PR 挤占 CI 队列，或将新业务实现与跨层切流伪装为 `[Slice: Chore]` 规避 `Dark -> Adapter -> Cutover`；已先在原 Issue 更新切片计划并说明工程原因的合理调整不按数量机械阻断。
- 缺少影响审阅的信息时指出具体缺项；当前结论与历史过程混杂、状态过期时要求修正。等价且清楚的结构可以接受，不按固定标题或篇幅评分。
- 新增或修改文字的评审，以及评审意见本身，遵循[语言与写作要求](../../doc/engineering/workflow/source-of-truth.md#language-and-writing)。
