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

## 按消费者给出验证结论

先用[公共行为与独立预期值](../../../testing-manual.md#regression-oracle)核对断言是否能检出原问题，再区分下列证据覆盖到哪一层；只列本次涉及的层，不新增统一回执。

| 实际证据 | 可以说明 | 不能代替 |
| --- | --- | --- |
| fixture / component / mock 通过 | 固定输入与受测接缝的行为 | 未经过的真实接线、持久化、授权或 Runtime |
| 真实界面截图和可见状态 | 指定版本/状态/视口的呈现 | 键盘、屏幕阅读器、native 控件或业务结果 |
| transport ACK / 受理状态 | 对应层收到或接受请求 | canonical committed result 或业务成功 |
| 原请求权威回执 | 身份与合同校验通过后的原操作结果 | 下游生产、交付或玩家成果，除非对应权威结果也已核对 |
| 恢复回读 | 原历史/结果在当前读权限下可恢复 | 恢复后可写、可服务或后续成功推进 |

例如 cognition committed 而业务 rejected，应分别报告候选已提交与业务未成功；近期窗口缺项不能报告失败。恢复后可服务的结论还要满足适用[正向继续合同](../../../testing-manual.md#request-recovery-regression)。未覆盖的层级明确写未验证，不能用模糊的“全通过”合并。

核对当前 source HEAD 的实际必要检查和测试选择，不只看 workflow 名称、绿色汇总或 `full` 字样；应执行却 skipped 仍未通过。测试合并树时说明真实 base/event/tested tree；复用 PR/CI 原证据，不把同版本重跑或 main 前进变成新的全量验证义务。

## Related methods

- [executing-project-tasks](../executing-project-tasks/SKILL.md)
- [requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md)
- [systematic-debugging](../systematic-debugging/SKILL.md)

## Known Failure Modes

局部失败只阻塞依赖动作，其他已授权工作继续。目标或外部影响扩大时确认新增部分；远程结果不明先查询，不重复创建或伪造完成。不以 Task、Project、模型观测或 receipt 代替真实结果。
