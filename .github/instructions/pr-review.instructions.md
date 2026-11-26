---
applyTo: "**"
---

## PR review

- review 前先了解业务背景
- 知道为什么这么改，再确认改得对不对
- 按[PR 描述要求](../../doc/engineering/workflow/source-of-truth.md#pr-description)核对正文能否独立说明问题、实际变化、验证覆盖和重要剩余项，并与当前 diff 及证据一致；文档改动须说明设计或规则变化及真实实现状态。
- 涉及代码时按[小而完整的 PR](../../doc/engineering/workflow/source-of-truth.md#small-complete-pr)、[预计 PR 边界](../../doc/engineering/workflow/source-of-truth.md#planned-pr-scope)、[切片与熔断规范](../../doc/engineering/workflow/pr-slicing-and-circuit-breaker.md)与[任务规模建议](../../doc/engineering/workflow/source-of-truth.md#code-task-file-limit)核对实际范围及适用计划：有子任务切片安排时，人工核对 `Planned-Slice: k/N`、交付物与验收归属；未拆分简单 PR 可无关联 Issue。Crate 白名单、相邻环、行数与活跃路径按硬边界及类型适用规则检查，Chore 不用于新业务/宿主实现或跨层切流。检查无说明偏离计划、遗漏必要联动、无实际收益的微 PR 和同子任务无解阻理由的兄弟开放 PR；说明工程原因并更新计划的合理归并或增拆可接受，30 文件与预计 N 不作数量门禁。
- 有切片计划时按[PR 实施 checklist](../../doc/engineering/workflow/source-of-truth.md#planned-pr-checklist)核对当前 PR 对应条目、实际验收与剩余承接；未合入或本片验收未满足不得勾选，局部交付和未合入关闭不算原完整范围已交付。
- 缺少影响审阅的信息时指出具体缺项；当前结论与历史过程混杂、状态过期时要求修正。等价且清楚的结构可以接受，不按固定标题或篇幅评分。
- 新增或修改文字的评审，以及评审意见本身，遵循[语言与写作要求](../../doc/engineering/workflow/source-of-truth.md#language-and-writing)。
