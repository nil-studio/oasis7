---
name: executing-project-tasks
description: Use when implementing and finishing an authorized oasis7 change.
---

# executing-project-tasks

通用规则见 [开发流程规范](../../../doc/engineering/workflow/source-of-truth.md)。

明确问题、范围、验收和用户限制。未拆分的简单修改写入 PR；拆出的开发任务按[原生 sub-issue 规则](../../../doc/engineering/workflow/source-of-truth.md#code-task-sub-issues)建立或复用父子 Issue，任务内部步骤和用例可用清单。同一授权范围内的实现取舍由负责人直接决定，必要时简要说明理由。

具有多项交付结果的跨模块任务、多人或跨会话持续任务，先按[项目拆解](../../../doc/engineering/workflow/source-of-truth.md#project-decomposition)和 [parallel-development](../parallel-development/SKILL.md)组织可用模块增量及关键依赖，只把当前和近期批次细化为有实际拆分收益的执行任务；后续目标与整体验收继续可见。共享接口先由真实消费方定向检查，条件具备再并行填充实现；同一业务的过细步骤与触达同一 CI 受影响范围的同类迁移、边界修复、配置治理先盘点后成批连续完成。按[预计 PR 边界](../../../doc/engineering/workflow/source-of-truth.md#planned-pr-scope)为当前及近期拟派发范围核对目标分支和实际依赖，写明计划覆盖范围、预计 PR 数量（`N`）及有序切片（`PR k/N`、`Slice-Type`、`Allowed-Crates`、预计非锁行数/文件数、交付物与消费者、所承接验收、依赖和 `Non-Goals`）。`N` 是可调整的当前计划估算，不是父目标完成比例；后续未细化范围保留目标、前置和解除动作。符合切片硬边界且无拆分收益时预计 1 PR 收口，包含多个独立结果时按实际收益复核子任务边界，不按固定 PR 数量自动拆单。派发前回读原生父子关系及子项认领、状态和暂停，复用已有记录。

开始具体实现前按[可验证的代码任务](../../../doc/engineering/workflow/source-of-truth.md#verifiable-code-task)确认本轮执行项与当前计划。接手的存量/在途 Issue 若缺当前执行范围的剩余预计 PR 数量与逐片边界，先核对目标分支及实际输入，在原 Issue 补齐，后续未细化范围不阻塞当前独立就绪工作。适用[锁边界流程](../../../doc/engineering/workflow/pr-slicing-and-circuit-breaker.md)的切片，仅为当前动工的一片建立零改动契约 Draft，锁定 `Slice-Type`、`Allowed-Crates` 与 `Non-Goals` 后再写实现；用 `Planned-Slice: k/N` 人工关联有效计划，不新增机器校验。普通 PR 的创建时机按[评审与合入](../../../doc/engineering/workflow/source-of-truth.md#42-评审与合入)执行，未建单的独立简单 PR 可填 `Planned-Slice: N/A`。类型与行数限制按切片规范的适用条件及 Docs、Chore、Bugfix 等例外执行，不将新业务/宿主实现或跨层切流伪装为 Chore。按[验收准备](../../../doc/engineering/workflow/source-of-truth.md#code-task-acceptance-readiness)核对输入、运行入口和结果观察；缺少夹具、接线、环境或权限时明确建设与解除动作。

沿当前计划组织交付。发现遗漏消费者、真实耦合或规模变化使原计划不再成立时，先暂停受影响的扩展，按[范围变化的处置](../../../doc/engineering/workflow/source-of-truth.md#task-scope-change)在原 Issue/PR 说明原因，更新计划及剩余验收承接。更新计划不自动扩大当前 Draft 已锁定的 `Allowed-Crates` 或原授权；新增范围按原锁边界规则安排，达到分册第 4 节熔断条件时转走只读来源拆骨，其他独立就绪范围继续。保留历史交付和未完义务，不能靠调整 `N` 或白名单制造完成。

按[代码任务规模建议](../../../doc/engineering/workflow/source-of-truth.md#code-task-file-limit)让本任务尽量不超过 30 个代码文件；范围包含必要测试、脚本、配置及生成代码，同任务各 PR 累计观察。接手时估算，实施与评审时复核实际范围；超过时优先评估拆分，完整联动或测试确有必要则保留范围继续实施，并在原 Issue/PR 简述原因，不按文件或 PR 数量自动停工或额外申请豁免；切片硬边界仍须满足。

新增或实质修改的跨层要求按[要求追踪规范](../../../doc/engineering/doc-governance/cross-layer-requirements-traceability.design.md)处理：先查原文声明，沿用同一要求的 Trace ID，在适用设计和当前 PR 等已有任务说明中写 Trace refs，并链接具体条款、说明本次范围与验证。追踪可直接引用 PR，不另领要求 ID；是否建立父子 Issue 按任务拆分规则执行。未形成或不适用的层级如实说明。只更新本次涉及的关系，不全仓回填，不把 ID 或找到关联记录当作实现、验证或合入证明。

接手已有代码或文档 Issue 开始实施前，按[日常开发](../../../doc/engineering/workflow/source-of-truth.md#2-日常开发)检查已有认领和暂停，在原 Issue 发布开工 comment 并回读确认；已有执行或状态不明时先核对交接，避免重复认领。直接独立会话（无 TPM 协调时）触碰高扇出底层 crate、`Cargo.lock` 或共享工作流入口，以及准备新开 PR 前，先查看当前在途开放 PR（如 `gh pr list`）与队列情况：同子任务默认只保持一个在途开放 PR，遇到同热点范围在途改动或队列明显拥堵时，优先协调归并或在本地完成完整批次准备后再提交，避免并发撞车反复变基。采用代码队列时，按[领取与状态维护](../../../doc/engineering/workflow/source-of-truth.md#coding-agent-task-claim)更新实现标签并处理写入失败。续做同一范围及已有 TPM 记录按通用规范复用，不重复留言。

修改前按开发规范检查 `git worktree list --porcelain`、工作区状态和使用情况，优先复用固定开发路径；聊天附件只用于识别附件，不能据此认定没有可复用工作区。需要新增时执行 `scripts/new-task-worktree.sh`，遵循仓库根目录配置，不以 App 创建、worktree fork 或 handoff 代替。用户明确要求其他入口时遵从用户要求。Cargo 默认使用各 worktree 的本地 `target/`，无需准备链接；日常反馈使用 wrapper，正式验收保留 canonical 命令。已有 target 不擅自删除。

当前开发会话在独立分支连续分析、实现、自测、跟进 PR/CI 并修复本项问题；不默认转派给 subagent。建议规模内且符合单切片硬边界的同一任务默认在单分支内用多个清楚的本地 commit 连续推进实现步骤、同模块迁移与已知边界分支，按改动类型完成适用本地自测（如 Rust 开发使用 `./scripts/cargo-dev.sh`、显式检查入口 `scripts/check-local.py`、前端/文档执行对应检查）并通过 `./scripts/check-pr-boundaries.sh --slice <Type> --allowed-crates <csv>` 预检收敛后再成批推送，沿上述创建时机更新同一 PR；达到[分册第 4 节熔断条件](../../../doc/engineering/workflow/pr-slicing-and-circuit-breaker.md#4-自动熔断阈值与只读拆骨止损协议salvage-protocol)时，立即停止原地修补并执行只读标本拆骨协议。并行任务按[独立开发会话](../../../doc/engineering/workflow/source-of-truth.md#independent-development-sessions)分别领取，保持写入边界并保留他人改动。

先读相关实现，沿调用和数据流确认受影响范围。最小完整修改以本次验收和适用契约为界，包含必要的消费者、测试、配置和文档；允许根因修复跨模块进行，不为减少文件数或行数省略必要修改。

新增能力前查找仓库已有实现、标准库和已有依赖。在行为、错误处理及状态约束相容时优先复用，不因代码表面相似合并不同职责。新增抽象、依赖、配置或包装层应服务本次验收或已确认的职责与边界，避免为未确认的未来用途预留机制；实现数量少不能单独作为删除接口或抽象的依据。

选择满足验收和适用契约、职责清楚且易于理解和维护的实现。简化时保留所需的边界校验、错误处理和恢复行为；无关清理不自动纳入本次修改。

排障修复在根因确认后先按[设计检查与修复](../../../doc/engineering/workflow/source-of-truth.md#root-cause-design-check)核对适用要求与系统设计：设计有缺陷先补受影响文档再实现，纯实现 bug 直接修复验证。只改有缺陷或受联动影响的层级，同一 PR 连续完成，不把设计修订变成额外审批或独立前置 PR；同模块内的同源边界缺陷先盘点齐全再同批修复验证，避免逐点单开一串单文件 PR。

按[验证与评审](../../../doc/engineering/workflow/source-of-truth.md#3-验证与评审)选择覆盖本项行为和风险的测试，随开发 PR 完成适用单元测试及不依赖外部条件的集成测试，修复并复测，交付可复跑结果。仅剩复杂测试时，按[测试分工与转换规则](../../../doc/engineering/workflow/source-of-truth.md#issue-lifecycle)在本项所需开发 PR 均合入后交接原 Issue；无此剩余且全部原义务满足时直接结项。可稳定自动复现的缺陷优先复用能检出问题的回归，覆盖不足再增加或修正，先观察失败再修复。机械修改不强制增加与实现重复的测试；父项继续负责实际组合和剩余环境验收。同步相关技术和使用文档。

使用验证和评审方法。同一交付范围继续更新原 PR，并优先在本地把已知关联问题一并修完自测后再推送，避免短时间高频零散 push 反复启动并取消远程 CI。创建和更新时按[PR 描述要求](../../../doc/engineering/workflow/source-of-truth.md#pr-description)读取[默认模板](../../../.github/pull_request_template.md)，填写实际问题、改动结果、验证和适用风险；简单变更可用等价简短结构。

接手已有拆分、实质调整范围及交付阶段成果时，按[前置事实与剩余验收承接](../../../doc/engineering/workflow/source-of-truth.md#decomposition-evidence-and-remainder)核对目标分支、输入版本和真实能力。请求评审前把实际 diff 对回 Issue 验收和计划批次；若仅完成局部范围或原承接 PR 已退出，更新原 Issue 的已交付范围、剩余验收承接、后续批次或解除动作，并让当前 PR 的剩余事项引用该安排。按[PR 实施清单](../../../doc/engineering/workflow/source-of-truth.md#planned-pr-checklist)在同一 Issue 正文跟进已划定范围，创建实际 PR 后补链接；实现中、评审中或未合入保持未勾选，仅实际合入且本片验收完成后勾选。局部合入先对齐实际范围并保留未完验收接续项或后继任务，再勾选已交付部分。改计划保留已完成历史，未合入关闭或替代继续跟踪有效剩余范围；经有效决定取消的范围移到历史说明，不计作交付或继续列为待办。未启动批次保留计划，适用锁边界流程时仅创建当前动工切片的 Draft；不把历史预留范围或来源分支测试当作当前交付，父项组合与环境验收仍单独保留。

通过 CLI 提交时，先把完整正文写入文件，再用 `gh pr create --title '<English title>' --body-file <path>`；通过 API 提交时显式提供同样的正文。发布后回读确认内容完整。实质改动后及请求评审前对照当前 diff 更新摘要，详细执行过程按规范归位，收尾时修正过期的当前状态。

按本次约定的交付终点收尾；仅交付可评审 PR 时如实注明尚未合入。合入前检查当前 HEAD、required checks、评审及阻断项、冲突、权限和 hold；正常保护合入绑定预期 HEAD。PR 实际 merged 才完成代码交付；已明确的部署或整体验收继续执行。

清理独立进行，保留 dirty、未推送、使用中和身份不明资源。

## Related methods

- [verification-before-completion](../verification-before-completion/SKILL.md)
- [parallel-development](../parallel-development/SKILL.md)
- [requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md)
- [systematic-debugging](../systematic-debugging/SKILL.md)

## Known Failure Modes

局部失败只阻塞依赖动作，其他已授权工作继续。目标或外部影响扩大时确认新增部分；远程结果不明先查询，不重复创建或伪造完成。不以 Task、Project、模型观测或 receipt 代替真实结果。

PR 标题和 commit message 使用英文。
