---
name: requesting-repo-owned-review
description: Use when arranging or responding to review for an oasis7 change.
---

# requesting-repo-owned-review

通用规则见 [开发流程规范](../../../doc/engineering/workflow/source-of-truth.md)。

按复杂度和风险选择审阅对象。机械文档和格式可由负责人自检及自动检查；权限、凭据、共识、持久化兼容、不可逆迁移、安全和开发门禁必须由具备对应能力且未实施该部分的审阅者独立检查。

提供具体问题、版本、diff、验证和风险。记录版本、结论、阻断发现及处置即可，不要求 packet、epoch、派工回执或 ledger 等额外评审证明链；按需交接上下文不构成新的准入或完成条件。

审阅 diff 及其影响的实现、调用方和关键约束。缺陷结论应给出具体位置、可核对的触发条件或可达路径、影响和依据，区分事实与假设；代码可确认的风险可直接报告，无需以事故或运行复现为前提。疑点标明尚待核查的条件，已有必需检查仍按通用规范完成。

简化建议应指出具体冗余、重复维护点或可复用实现，并给出保留所需行为与边界的改法。代码行数、实现数量和个人偏好本身不构成阻断依据。

反馈先核对事实和影响。不影响明确验收、且未揭示实际风险的纯简化建议按非阻断处理；未完成本次明确的简化目标，或涉及正确性、安全和数据丢失等问题，仍按验收要求及通用规范处理。修复阻断项，或以可复核事实说明不适用；非阻断建议可说明理由后不改。已有取舍记录不自动免于当前约束和风险的核对。

复审关注实际变化和受影响部分；描述更新或同版本 CI 重跑不触发全量重审。CI 和评审可并行，合入前汇总实际结果。

## 区分合同偏差与规范问题

按需从两条线检查同一 diff：一是需求/合同的输入、状态、权威效果与消费者是否成立，二是仓库规范、实现结构与可维护性是否符合。两条线可以由同一审阅者完成，不增加固定角色或评审轮次。先解释实质风险，不能让风格意见掩盖 accepted/committed、unknown/failed 或 production/delivery 的语义偏差。

恢复类变更可对照 [原请求、独立 oracle 与 UI 状态案例](../systematic-debugging/references/skills-research-checks.md)；它是寻找遗漏的提示，不是第二份协议。以适用 authority 为准，安全等效实现不因写法不同而误报；发现合同本身有缺陷时回到既有设计检查流程，不在评审中偷偷新增要求。

## Related methods

- [executing-project-tasks](../executing-project-tasks/SKILL.md)
- [verification-before-completion](../verification-before-completion/SKILL.md)
- [systematic-debugging](../systematic-debugging/SKILL.md)

## Known Failure Modes

局部失败只阻塞依赖动作，其他已授权工作继续。目标或外部影响扩大时确认新增部分；远程结果不明先查询，不重复创建或伪造完成。不以 Task、Project、模型观测或 receipt 代替真实结果。
