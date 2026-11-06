---
name: executing-project-tasks
description: Use when implementing and finishing an authorized oasis7 change.
---

# executing-project-tasks

通用规则见 [开发流程规范](../../../doc/engineering/workflow/source-of-truth.md)。

明确问题、范围、验收和用户限制。未拆分的简单修改写入 PR；拆出的开发任务按[原生 sub-issue 规则](../../../doc/engineering/workflow/source-of-truth.md#code-task-sub-issues)建立或复用父子 Issue，任务内部步骤和用例可用清单。同一授权范围内的实现取舍由负责人直接决定，必要时简要说明理由。

具有多项交付结果的跨模块任务、多人或跨会话持续任务，先按[任务拆分与并行开发](../../../doc/engineering/workflow/source-of-truth.md#parallel-task-development)和 [parallel-development](../parallel-development/SKILL.md)梳理剩余工作、依赖、写入负责人及整体验收。条件具备时由多个独立开发会话并行实施；强耦合部分按实际依赖串行。默认交付小而完整的 PR，公共前置具备条件后及时评审并在授权内合入，让依赖方尽早复用；任务数与 PR 数分别判断。派发前回读原生父子关系及具体子 Issue 的认领、状态和暂停；复用本项已有委派记录，父项认领不代替子项领取。

开始具体实现前按[可验证的代码任务](../../../doc/engineering/workflow/source-of-truth.md#verifiable-code-task)确认本轮执行项与交付边界，并按[验收准备](../../../doc/engineering/workflow/source-of-truth.md#code-task-acceptance-readiness)核对输入、运行入口和结果观察。只有父目标或阶段标题时先补齐可执行范围；缺少最小夹具或接线时先明确其建设与验证，环境或权限缺口保留具体解除条件。开发中出现独立新结果或前提变化时更新受影响拆分，不把剩余父目标持续吸收到当前任务。

新增或实质修改的跨层要求按[要求追踪规范](../../../doc/engineering/doc-governance/cross-layer-requirements-traceability.design.md)处理：先查原文声明，沿用同一要求的 Trace ID，在适用设计和当前 PR 等已有任务说明中写 Trace refs，并链接具体条款、说明本次范围与验证。追踪可直接引用 PR，不另领要求 ID；是否建立父子 Issue 按任务拆分规则执行。未形成或不适用的层级如实说明。只更新本次涉及的关系，不全仓回填，不把 ID 或找到关联记录当作实现、验证或合入证明。

接手已有代码或文档 Issue 开始实施前，按[日常开发](../../../doc/engineering/workflow/source-of-truth.md#2-日常开发)检查已有认领和暂停，在原 Issue 发布开工 comment 并回读确认；已有执行或状态不明时先核对交接，避免重复认领。采用代码队列时，按[领取与状态维护](../../../doc/engineering/workflow/source-of-truth.md#coding-agent-task-claim)更新实现标签并处理写入失败。续做同一范围及已有 TPM 记录按通用规范复用，不重复留言。

修改前按开发规范检查 `git worktree list --porcelain`、工作区状态和使用情况，优先复用固定开发路径；聊天附件只用于识别附件，不能据此认定没有可复用工作区。需要新增时执行 `scripts/new-task-worktree.sh`，遵循仓库根目录配置，不以 App 创建、worktree fork 或 handoff 代替。用户明确要求其他入口时遵从用户要求。Cargo 默认使用各 worktree 的本地 `target/`，无需准备链接；日常反馈使用 wrapper，正式验收保留 canonical 命令。已有 target 不擅自删除。

当前开发会话在独立分支连续分析、实现、自测、跟进 PR/CI 并修复本项问题；不默认转派给 subagent。并行任务按[独立开发会话](../../../doc/engineering/workflow/source-of-truth.md#independent-development-sessions)分别领取，保持写入边界并保留他人改动。

先读相关实现，沿调用和数据流确认受影响范围。最小完整修改以本次验收和适用契约为界，包含必要的消费者、测试、配置和文档；允许根因修复跨模块进行，不为减少文件数或行数省略必要修改。

新增能力前查找仓库已有实现、标准库和已有依赖。在行为、错误处理及状态约束相容时优先复用，不因代码表面相似合并不同职责。新增抽象、依赖、配置或包装层应服务本次验收或已确认的职责与边界，避免为未确认的未来用途预留机制；实现数量少不能单独作为删除接口或抽象的依据。

选择满足验收和适用契约、职责清楚且易于理解和维护的实现。简化时保留所需的边界校验、错误处理和恢复行为；无关清理不自动纳入本次修改。

排障修复在根因确认后先按[设计检查与修复](../../../doc/engineering/workflow/source-of-truth.md#root-cause-design-check)核对产品与系统设计：设计有缺陷先补受影响文档再实现，纯实现 bug 直接修复验证。只改有缺陷或受联动影响的层级，同一 PR 连续完成，不把设计修订变成额外审批或独立前置 PR。

按[验证与评审](../../../doc/engineering/workflow/source-of-truth.md#3-验证与评审)选择覆盖本项行为和风险的测试，执行局部验证、修复并复测，随本项交付可复跑结果。可稳定自动复现的缺陷优先复用能检出问题的回归，覆盖不足再增加或修正，先观察失败再修复。机械修改不强制增加与实现重复的测试；父项继续负责实际组合和剩余环境验收。同步相关技术和使用文档。

使用验证和评审方法。同一交付范围继续更新原 PR。创建和更新时按[PR 描述要求](../../../doc/engineering/workflow/source-of-truth.md#pr-description)读取[默认模板](../../../.github/pull_request_template.md)，填写实际问题、改动结果、验证和适用风险；简单变更可用等价简短结构。

通过 CLI 提交时，先把完整正文写入文件，再用 `gh pr create --title '<English title>' --body-file <path>`；通过 API 提交时显式提供同样的正文。发布后回读确认内容完整。实质改动后及请求评审前对照当前 diff 更新摘要，详细执行过程按规范归位，收尾时修正过期的当前状态。

合入前检查当前 HEAD、required checks、评审及阻断项、冲突、权限和 hold；正常保护合入绑定预期 HEAD。PR 实际 merged 才完成代码交付；已明确的部署或整体验收继续执行。

清理独立进行，保留 dirty、未推送、使用中和身份不明资源。

## Related methods

- [verification-before-completion](../verification-before-completion/SKILL.md)
- [parallel-development](../parallel-development/SKILL.md)
- [requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md)
- [systematic-debugging](../systematic-debugging/SKILL.md)

## Known Failure Modes

局部失败只阻塞依赖动作，其他已授权工作继续。目标或外部影响扩大时确认新增部分；远程结果不明先查询，不重复创建或伪造完成。不以 Task、Project、模型观测或 receipt 代替真实结果。

PR 标题和 commit message 使用英文。
