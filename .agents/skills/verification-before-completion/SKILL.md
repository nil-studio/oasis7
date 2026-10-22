---
name: verification-before-completion
description: Use when reporting oasis7 verification or completion.
---

# verification-before-completion

通用规则见 [开发流程规范](../../../doc/engineering/workflow/source-of-truth.md)。

按实际变化和受影响消费者选择真实验证，参考 `testing-manual.md`；先局部验证，再核对实际 CI 完整必要结果。影响不明时扩大覆盖。

直接检查退出码和输出，记录命令、所测版本、结果和未覆盖部分。当前 source HEAD 的必需 CI 成功结果不会仅因 main 前进而失效；按通用规范中的冲突或明确集成风险决定是否更新分支和重测。旧版本结果不能代表新 HEAD 或新的 main 合并组合。失败、取消、缺失、未知和应执行却跳过不能算通过。

问题修复的自检按[设计检查与修复](../../../doc/engineering/workflow/source-of-truth.md#root-cause-design-check)核对根因及设计判断：设计缺陷已先修正文档，实现与回归符合正确约定；纯实现 bug 能说明设计依据且已完成修复验证。没有设计缺陷不要求文档 diff，未确认的设计缺口不能用测试通过代替澄清；不另造完成证书。

报告 ready 前检查适用评审及阻断项，报告代码完成前确认 PR 实际 merged 和提交。明确的部署、多 PR 和管理义务尚未完成时列出剩余工作。

## 按消费者拆开结论

对交互/恢复改动，分别说明渲染、请求受理、原请求权威结果、业务效果与刷新后恢复的实际证据；没有覆盖的层级保持未知。cognition 的 committed 不自动证明制造等业务成功，近期窗口没有结果也不证明失败。检查与具体负例见 [场景参考](../systematic-debugging/references/skills-research-checks.md)。

核对 CI 时以当前 source HEAD 的实际 required 检查和测试选择为准；局部测试、full 套件或某个绿色 workflow 都不能替代尚未核对的独立必要检查。测试合并树时保留实际 base/event/tested tree；不因 main 前进机械重跑，也不把旧组合冒称新组合。复用现有 PR/CI 证据，不新增验证回执。

## Related methods

- [executing-project-tasks](../executing-project-tasks/SKILL.md)
- [requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md)
- [systematic-debugging](../systematic-debugging/SKILL.md)

## Known Failure Modes

局部失败只阻塞依赖动作，其他已授权工作继续。目标或外部影响扩大时确认新增部分；远程结果不明先查询，不重复创建或伪造完成。不以 Task、Project、模型观测或 receipt 代替真实结果。
