---
applyTo: "**"
---

## PR review

- review 前先了解业务背景
- 知道为什么这么改，再确认改得对不对
- 按[PR 描述要求](../../doc/engineering/workflow/source-of-truth.md#pr-description)核对正文能否独立说明问题、实际变化、验证覆盖和重要剩余项，并与当前 diff 及证据一致；文档改动须说明设计或规则变化及真实实现状态。
- 涉及代码时按[小而完整的 PR](../../doc/engineering/workflow/source-of-truth.md#small-complete-pr)与[任务规模建议](../../doc/engineering/workflow/source-of-truth.md#code-task-file-limit)结合预计及实际累计范围双向核对粒度：尽量不超过 30 个代码文件，超过时核对拆分收益与保留完整改动的理由；同时检查是否将无外部下游等待的同域迁移、边界修复或同类配置拆成过多微 PR，或在同一子任务并行挂出多个兄弟开放 PR 挤占 CI 队列，不仅凭文件或 PR 数量提出机械阻断。
- 缺少影响审阅的信息时指出具体缺项；当前结论与历史过程混杂、状态过期时要求修正。等价且清楚的结构可以接受，不按固定标题或篇幅评分。
- 新增或修改文字的评审，以及评审意见本身，遵循[语言与写作要求](../../doc/engineering/workflow/source-of-truth.md#language-and-writing)。
