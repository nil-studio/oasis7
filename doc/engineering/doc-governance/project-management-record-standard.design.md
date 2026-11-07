# 项目管理记录方法

- 状态：active
- 适用范围：Issue、按需使用的 Project、复杂变更说明与组合验收
- 当前入口：[文档治理](README.md)
- 通用规则：[开发流程规范](../workflow/source-of-truth.md)

项目管理记录帮助读者理解目标、范围、依赖和结果。未拆分的简单修改使用 PR 描述；拆出的独立开发任务按[原生 sub-issue 规则](../workflow/source-of-truth.md#code-task-sub-issues)使用 GitHub 父子 Issue，Project 按需展示进度。

## 内容

说明问题、预期结果、允许范围、负责人、必要依赖、验收方式和重要限制。记录已完成及剩余义务，区分代码交付与明确要求的部署、迁移或组合验收。协调 Issue 在所需义务实际完成后关闭。

用 Git 提交、PR、真实 CI 运行和评审记录定位实际版本及结果。失败只阻塞依赖动作，Project 同步失败不改变已合入事实。历史 Issue、本地资料和用户记忆保留为可读材料，不生成身份、packet、receipt 或终态证明链。

## 父子任务与前置依赖

Trace refs: O7-REQ-f1388c2b-6f64-4ebd-886e-0b741bfe176c；本节说明[原生代码子任务](../workflow/source-of-truth.md#code-task-sub-issues)的记录分工。

父 Issue 记录总体范围、原生子项关系、依赖和整体验收；子 Issue 记录本项范围、负责人/会话、独立认领和实现状态、关联 PR、验收结果及剩余义务。创建或复用、挂接与派发前回读按主责规范执行。父项 active 不代替子项领取，子 PR 不自动关闭父项；子项沿用父目标适用的授权和暂停。

父子关系表示目标归属，前置依赖另记所需产物、输入版本和解除条件。父正文中的子任务清单和链接只作导航，不能代替子 Issue 或 GitHub 原生关系；任务内部机械步骤和测试用例可保留清单。

## 记录位置与持续更新

信息分类统一见[开发流程规范：长期文档与过程记录的归属](../workflow/source-of-truth.md#information-record-placement)。目标、阻塞、逐次尝试、候选/CI/评审与剩余安排写在对应 Task/PR；已有 Issue 承接其真实范围，拆出的开发任务使用子 Issue，未拆分的简单修改可直接使用 PR，不为保存同一记录重复建单。

正文概括范围和当前情况，时间线保存变化、失败与原始证据定位。原始材料按其证据保留要求保存，不能只依赖可能过期的 CI 链接；未核到唯一接收方时保留固定源，不倒入任意新 Task。长期合同与允许的派生摘要按各自职责维护，不复制整份任务台账。

## 跨层要求关联

Trace refs: O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6；本节承接[全局要求追踪](cross-layer-requirements-traceability.design.md#global-requirement-trace)的适用写作/引用约定。

涉及稳定要求的任务，按[跨层追踪规范](cross-layer-requirements-traceability.design.md)在原有说明中写 `Trace refs: <完整 ID>`，逐项链接产品或专业要求声明处与适用的系统设计条款，并说明本次实现/验证范围。产品、设计和 Task 共用要求的 ID；Task 本身不另领全局 ID。一个任务可以承接多项要求，一项要求也可以由多个任务交付。

要求追踪可直接引用当前 PR，不因追踪另建 Issue 或任务标识；拆出的独立开发任务仍按原生 sub-issue 规则使用子 Issue，Project 按需使用。纯工程工作链接专业要求并说明产品层不适用；简单编辑不为补齐三层而编造需求或设计。新增引用时保留已有局部 REQ/AC、真实 path#fragment 和 Git/PR/CI 证据，不生成第二份状态台账。

当前可用[只读追踪工具](../../../scripts/requirement-trace.README.md)聚合其支持范围内的关系，也可按完整 ID 搜索并人工追链；工具不判定实现状态或验收完成。搜索覆盖、未形成或不适用的层级和未完成义务应如实说明，找到相关 PR 不等于要求已经满足。

产品功能点的[设计/实现状态摘要](product-documentation-standard.design.md#feature-design-implementation-status)回链本 Task 的实际范围和证据；Task 不复制产品要求正文。交付或验收依据改变时同步重核受影响摘要；部分交付列清剩余义务，已验证结论写明固定基线、环境、入口和验收范围。PR 合入或 Issue 关闭只说明对应记录状态，不能替代功能验收；详细执行过程仍留在这里。

## 可选说明模板

模板填在本项子 Issue、其他已使用的 Issue 或当前 PR，按实际范围选用；不将每次实填结果新增为仓库 Markdown 文件。

- 问题与目标：
- 范围及重要限制：
- 父目标与原生父子关系（拆出的开发任务适用）：
- Trace refs、要求声明及适用设计链接（涉及跨层要求时）：
- 必要依赖与验收：
- 实际版本、结果及链接：
- 剩余工作和风险：

长期产品要求保留在产品文档，专业技术合同保留在对应系统设计，实现和测试承载运行事实。使用 [系统设计写作方法](system-design-writing-standard.design.md) 维护技术材料。
