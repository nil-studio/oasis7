---
name: executing-project-tasks
description: Use when implementing and finishing an authorized oasis7 change.
---

# executing-project-tasks

通用规则见 [开发流程规范](../../../doc/engineering/workflow/source-of-truth.md)。

明确问题、范围、验收和用户限制。简单修改写入 PR，复杂任务使用已有设计或按需使用的 Issue checklist。同一授权范围内的实现取舍由负责人直接决定，必要时简要说明理由。

新增或实质修改的跨层要求按[要求追踪规范](../../../doc/engineering/doc-governance/cross-layer-requirements-traceability.design.md)处理：先查原文声明，沿用同一要求的 Trace ID，在适用设计和当前 PR 等已有任务说明中写 Trace refs，并链接具体条款、说明本次范围与验证。Task 可直接是 PR，不另领要求 ID，也不强制建 Issue；未形成或不适用的层级如实说明。仅对本次涉及的关系更新，不全仓回填，不把 ID 或找到关联记录当作实现、验证或合入证明。

修改前按开发规范检查 `git worktree list --porcelain`、工作区状态和使用情况，优先复用固定开发路径；聊天附件只用于识别附件，不能据此认定没有可复用工作区。需要新增时执行 `scripts/new-task-worktree.sh`，遵循仓库根目录配置，不以 App 创建、worktree fork 或 handoff 代替。用户明确要求其他入口时遵从用户要求。Cargo 默认使用各 worktree 的本地 `target/`，无需准备链接；日常反馈使用 wrapper，正式验收保留 canonical 命令。已有 target 不擅自删除。

在独立分支连续分析、实现、局部验证和修复。subagent 按需使用，说明必要上下文、写入范围和期望结果，避免重叠编辑。保留他人改动。

先读相关实现，沿调用和数据流确认受影响范围。最小完整修改以本次验收和适用契约为界，包含必要的消费者、测试、配置和文档；允许根因修复跨模块进行，不以文件数或行数省略必要修改。

新增能力前查找仓库已有实现、标准库和已有依赖。在行为、错误处理及状态约束相容时优先复用，不因代码表面相似合并不同职责。新增抽象、依赖、配置或包装层应服务本次验收或已确认的职责与边界，避免为未确认的未来用途预留机制；实现数量少不能单独作为删除接口或抽象的依据。

在满足验收和适用契约的方案中，选择职责清楚、容易理解和维护的实现。简化保留所需的边界校验、错误处理和恢复行为；无关清理不自动纳入本次修改。

排障修复在根因确认后先按[设计检查与修复](../../../doc/engineering/workflow/source-of-truth.md#root-cause-design-check)核对产品与系统设计：设计有缺陷先补受影响文档再实现，纯实现 bug 直接修复验证。只改有缺陷或受联动影响的层级，同一 PR 连续完成，不把设计修订变成额外审批或独立前置 PR。

可稳定自动复现的行为缺陷，优先复用能检出该缺陷的现有回归测试，覆盖不足再增加或修正，先观察失败再修复。验证覆盖受影响行为和关键风险，不以测试数量判断充分；机械修改不强制增加与实现重复的测试。同步相关技术和使用文档。

使用验证和评审方法。同一目的继续更新原 PR，说明问题、改动、验证和风险。合入前检查当前 HEAD、required checks、评审及阻断项、冲突、权限和 hold；正常保护合入绑定预期 HEAD。实际 merged 才完成代码交付，明确部署或整体验收继续执行。

清理独立进行，保留 dirty、未推送、使用中和身份不明资源。

## Related methods

- [verification-before-completion](../verification-before-completion/SKILL.md)
- [requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md)
- [systematic-debugging](../systematic-debugging/SKILL.md)

## Known Failure Modes

局部失败只阻塞依赖动作，其他已授权工作继续。目标或外部影响扩大时确认新增部分；远程结果不明先查询，不重复创建或伪造完成。不以 Task、Project、模型观测或 receipt 代替真实结果。

PR 标题和 commit message 使用英文。
