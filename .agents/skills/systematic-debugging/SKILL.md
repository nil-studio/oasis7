---
name: systematic-debugging
description: Use when diagnosing an oasis7 bug, failed test, or regression.
---

# systematic-debugging

通用规则见 [开发流程规范](../../../doc/engineering/workflow/source-of-truth.md)。

取得可重复失败和准确错误，区分产品、测试、环境及入口问题。缩小输入与失败边界，读相关实现和近期 diff，提出可证伪假设。

用最小实验验证关键假设，定位根因后先按[设计检查与修复](../../../doc/engineering/workflow/source-of-truth.md#root-cause-design-check)核对受影响的产品设计文档和系统设计文档，再决定修复路径。检查产品要求、体验和验收是否遗漏或冲突，以及系统职责、接口、状态和失败/恢复合同是否足以支持正确行为。

有设计缺陷时，先修订受影响的产品/系统文档，再按正确约定补回归、修复实现并验证；设计与实现同时有问题也按此顺序。设计正确而实现偏离时，直接修复验证，不为流程改文档。缺文档或约定含糊不能直接认定纯代码 bug；查明设计依据，必要取舍超出授权时只暂停依赖动作。只修有问题的层级，同一 PR 连续完成，不要求文档先单独合入。

不用重启、跳过测试或吞错误掩盖根因，也不改写正确设计或放宽测试迁就错误实现。对可自动复现行为先写或修正回归测试，观察失败再修复；测试保护真实行为，避免镜像实现。验证受影响消费者、兼容和失败路径，说明环境限制。在当前 PR 或已有 Task 简述根因、设计判断及依据、修复与验证结果，不新建排障表单或流程任务。

复杂问题按需请求领域协作，给出复现、上下文、写入范围和期望结果；不依赖流程身份。重复失败时修正假设，不堆叠猜测补丁。

## 按症状选择最短反馈环

序列化/恢复问题先比较固定 fixture 或原始事件回放；跨实现或宿主差异考虑差分；边界组合考虑性质测试；明确的历史回归考虑 bisect。选择能对原症状判红且成本最低的一条，不要求把这些方法全跑一遍。先保留原失败，不能用新世界、新请求或重启后的正常状态替代它。

预期值取自适用合同、独立手算样例或固定权威原件，不调用被测投影/关联函数再生成 expected。对请求恢复同时检查原身份、真实业务效果与失败路径；日志用于定位，不代签权威结果。具体按需案例见 [反馈环与恢复检查](references/skills-research-checks.md)。

## Related methods

- [executing-project-tasks](../executing-project-tasks/SKILL.md)
- [verification-before-completion](../verification-before-completion/SKILL.md)
- [requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md)

## Known Failure Modes

局部失败只阻塞依赖动作，其他已授权工作继续。目标或外部影响扩大时确认新增部分；远程结果不明先查询，不重复创建或伪造完成。不以 Task、Project、模型观测或 receipt 代替真实结果。
