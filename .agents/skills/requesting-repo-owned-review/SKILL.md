---
name: requesting-repo-owned-review
description: Use when arranging or responding to review for an oasis7 change.
---

# requesting-repo-owned-review

通用规则见 [开发流程规范](../../../doc/engineering/workflow/source-of-truth.md)。

按复杂度和风险选择审阅对象。机械文档和格式可由负责人自检及自动检查；权限、凭据、共识、持久化兼容、不可逆迁移、安全和开发门禁必须由具备对应能力且未实施该部分的审阅者独立检查。

提供具体问题、版本、diff、验证和风险。记录版本、结论、阻断发现及处置即可，不要求 packet、epoch、派工回执或 ledger 等额外评审证明链；按需交接上下文不构成新的准入或完成条件。

## 审阅合同与实现

先读当前需求/设计和 diff，再跟到受影响的调用方、状态变换与最终消费者，不只审改动行。按两条线检查，可由同一审阅者完成，不增加固定角色或轮次：

1. 合同是否成立：沿“适用条款 → 触发输入/状态 → 实际消费者 → 正/负例”追一条最短路径。核对权限、身份、失败/恢复及真正的业务效果；分清 accepted/committed、unknown/failed、produced/delivered。先核对该条款是当前支持范围还是未实现目标，不把目标说明当作已上线行为。
2. 实现是否可靠：检查仓库规范、已有能力复用、重复状态/逻辑、错误传播、兼容与可维护性。安全等效实现不因写法不同而判错；风格意见不能掩盖合同偏差。合同本身有缺陷时走既有[设计检查](../../../doc/engineering/workflow/source-of-truth.md#root-cause-design-check)，不在评审中偷偷新增产品要求。

缺陷结论给出具体位置、可核对的触发条件或可达路径、影响和依据，区分事实与假设。可由代码确认的风险可直接报告，无需以事故或运行复现为前提；疑点写明待核查条件，不把证据不足的疑点写成确定缺陷。回归建议说明什么行为会判红，可对照[独立预期值与恢复场景](../../../testing-manual.md#regression-oracle)，不要求另填追踪表。

## 处理反馈与复审

简化建议应指出具体冗余、重复维护点或可复用实现，并给出保留所需行为与边界的改法。代码行数、实现数量和个人偏好本身不构成阻断依据。

反馈先核对事实和影响。不影响明确验收、且未揭示实际风险的纯简化建议按非阻断处理；未完成本次明确的简化目标，或涉及正确性、安全和数据丢失等问题，仍按验收要求及通用规范处理。修复阻断项，或以可复核事实说明不适用；非阻断建议可说明理由后不改。已有取舍记录不自动免于当前约束和风险的核对。

复审关注实际变化和受影响部分；描述更新或同版本 CI 重跑不触发全量重审。CI 和评审可并行，合入前汇总实际结果。

## Related methods

- [executing-project-tasks](../executing-project-tasks/SKILL.md)
- [verification-before-completion](../verification-before-completion/SKILL.md)
- [systematic-debugging](../systematic-debugging/SKILL.md)

## Known Failure Modes

局部失败只阻塞依赖动作，其他已授权工作继续。目标或外部影响扩大时确认新增部分；远程结果不明先查询，不重复创建或伪造完成。不以 Task、Project、模型观测或 receipt 代替真实结果。
