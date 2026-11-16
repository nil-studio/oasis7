## PR review

- review 前先了解业务背景
- 知道为什么这么改，再确认改得对不对
- 按[PR 描述要求](../../doc/engineering/workflow/source-of-truth.md#pr-description)核对正文能否独立说明问题、实际变化、验证覆盖和重要剩余项，并与当前 diff 及证据一致；文档改动须说明设计或规则变化及真实实现状态。
- 涉及代码时按[任务规模规则](../../doc/engineering/workflow/source-of-truth.md#code-task-file-limit)核对每项预计及实际变更最多 30 个代码文件，包括同任务跨 PR 的累计范围；超限先重拆，不能仅凭当前 PR 不超过上限判定符合规则。
- 缺少影响审阅的信息时指出具体缺项；当前结论与历史过程混杂、状态过期时要求修正。等价且清楚的结构可以接受，不按固定标题或篇幅评分。
- 新增或修改文字的评审，以及评审意见本身，遵循[语言与写作要求](../../doc/engineering/workflow/source-of-truth.md#language-and-writing)。
