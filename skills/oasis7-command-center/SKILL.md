---
name: oasis7-command-center
description: 为 eng-cc/oasis7 构建、修改或刷新项目开发指挥看板，关联短期与长期目标、当前有效规范及决策、Git/PR 交付事实与原 PR、外部工具定位和待人处理的问题。在用户要求 oasis7 项目总览、指挥面板、持续更新项目状态、目标进度、近期变化、架构与部署索引、规范回顾或通过看板管理开发任务时使用。不把游戏内 Agent/Viewer 当成开发会话，也不作为普通代码修改的默认流程。
---

# Oasis7 项目指挥看板

按真实项目来源帮助用户判断目标、进展、决定和当前行动。产品行为与验收读取[正式产品设计](../../doc/engineering/command-center/command-center.prd.md)，技术选型与实现合同读取[正式系统设计](../../doc/engineering/command-center/command-center.design.md)。本 Skill 只保留使用和执行方法，不另立产品合同。

日常入口按正式 PRD 的 [首页判断](../../doc/engineering/command-center/command-center.prd.md#REQ-CC-013) 和 [工作接续](../../doc/engineering/command-center/command-center.prd.md#REQ-CC-014)组织；完整基础首版和来源/桌面切片分别说明。仅调整设计时保留当前实现差距，不顺带运行模型、迁移会话或改变已有暂停范围。

遵循当前仓库的 `doc/engineering/workflow/source-of-truth.md`，不增加任务准入、审批、生命周期、回执或交付条件。若从独立 Skill 位置调用，按已确认的 oasis7 Git 根目录解析上述文档。

## 先按请求确定动作

| 用户请求 | 交付与写入范围 |
| --- | --- |
| 构建、定制、修复看板 | 按正式 PRD/design 实现并验证所需切片及数据接入 |
| 查看、刷新、汇报进度 | 读取事实，更新已有派生视图或给出简报；不创建任务、发评论、改业务状态 |
| 持续维护项目状态或架构索引 | 按[持续更新流程](../../doc/engineering/command-center/command-center.manual.md)更新受影响文档；在已有授权内走普通分支/PR，不把只读查看变成远端写入 |
| 记录、修改一项决策 | 在已有授权范围内更新对应主责文档，再刷新索引与关联 |
| 从看板接续开发工作 | 打开原 PR/工作树或明确外部会话定位；实际任务在原工具执行 |
| 只要设计方案或 Skill | 交付要求的文档或 Skill，不顺带实施应用 |

已有授权持续有效。只问影响具体决定的缺失信息，先完成独立可做的工作。查看状态不意味着授权启动 Codex、外部模型任务、合入或部署。

## 定位当前来源

1. 读取[项目入口地图](references/project-map.md)，确认目标为 `eng-cc/oasis7`。本地从 Git 根目录和工作树识别仓库；远程固定本轮已解析的提交，记录版本与查询时间。
2. 读取当前 `AGENTS.md`、开发流程和根 `README.md`，再用 `rg` 定位相关产品、专业域或测试条款，避免全仓全文加载。
3. 先读[项目状态](../../doc/engineering/command-center/README.md)的核验范围和[架构索引](../../doc/engineering/command-center/architecture/README.md)，再从 `doc/core/prd.md` 的“当前阶段交付目标与全局 P0”核对近期目标，从产品入口核对长期方向；摘要不是权威，不用历史里程碑替代当前路线图。
4. 按事项主责和显式后继关系核对规范；不能只按修改时间、审计轮次、文件名或模型记忆排序。遇到冲突说明来源及影响，不阻塞无关工作。
5. 从 Git、PR、实际 CI 和评审核实工程交付（检查详情在 GitHub 核对）；按需关联 Issue/Project，不要求只有 PR 的工作补建 Issue。分页不完整、离线或权限不足时保留可用结果并说明范围。
6. 将上传旧稿、历史 `.pm` 与当前规范区分。旧 `world-rule.md`、`agent_world_*` 命名通过当前导航找接收条款；不恢复退役任务账本。游戏世界实体与开发会话分别识别。

## 构建或修改看板

1. 对照正式 PRD 的相关 `REQ-CC`/`AC-CC` 确定本次用户结果，按[客户端实施方法](references/desktop-client.md)查找当前实现与适用入口；不要把目标设计描述成已完成软件。
2. 按正式系统设计采用已选技术基线和代码边界，核对当前依赖与用户约束。需要改变行为或架构时更新主责文档，不在 Skill 内保留第二套决定。
3. 先贯通真实来源、派生数据和页面，再逐步接入所需能力。演示数据只进入标明的演示入口；未接入区域按正式 PRD 显示范围。
4. 按[视图刷新方法](references/view-model.md)核对目标、规范及交付关系；需要原会话定位时读取[Codex 外部接续方法](references/external-handoff.md)。刷新不启动模型任务。
5. 复用适合的组件和已有工具方法。UI 设计 Skill 适合且可用时按需采用；不要为看板改造世界运行时、公共 `site/` 或玩家 Viewer。

实施、调试、验证和评审按需使用 `.agents/skills/README.md`，不串固定角色链。一个清楚目的尽量在一个 PR 原子完成；只读查询不创建分支、Issue 或远程记录。

## 刷新、决策与执行

- 维护持久文档时先读[持续更新流程](../../doc/engineering/command-center/command-center.manual.md)，沿日常工作收尾、目标/设计变化及项目回顾更新受影响项；没有变化不制造提交，不为每次合入递归创建状态 PR。
- 按所选范围重新读取来源，只更新实际变化的派生内容；保留摘要内部事实核验时间，不能用本次文件读取时间冒充目标、部署或验收刚更新。只要简报时直接汇报。
- 同步长期方向、短期目标、已核实进展、近期有意义变化及真正需要人关注的事项；部署索引分别写设计、实现和实际环境核验，不把历史机器清单当成当前拓扑。
- 按“当前目标、变化与交付、阻塞/待决定、下一步、来源及未覆盖范围”汇报；模型归纳标明性质并保留事实入口。
- 记录个人决定只保存内容、理由、范围、正式文档关联与草稿/确认/替代历史，保留原确认时间；正式条款修改按原授权在原工具执行。不要恢复双轨同步、证据修订、全范围checkbox、决定关注或待处理同步面板。
- 仅在用户要求记录且确无承载位置时，按当前文档规范创建最小决策文档；不另建通用决策登记系统。
- 执行动作前核对仓库、环境、运行目标与状态，复用有效授权；结果不确定先查已有结果，避免重复启动或写入。

## 验证与交付

按改动选择正式 PRD 的相关验收及系统设计中的对应验证。界面变更结合当前 `testing-manual.md`；重点覆盖误导进度、来源缺失、多会话串绑及重复执行等实际风险，不为纯文本 Skill 跑完整应用或游戏栈。

交付说明实际内容、入口、来源覆盖、验证结果与未接入能力。分别表述 Skill 已保存、页面已实现、代码已合入和目标已验收。完成适用验证后停止扩展测试。

## 按需参考

- [正式产品设计](../../doc/engineering/command-center/command-center.prd.md)：用户价值、四视图、状态语义、首版边界和验收。
- [正式系统设计](../../doc/engineering/command-center/command-center.design.md)：技术选型、组件、存储、协议、生命周期与发布。
- [项目状态](../../doc/engineering/command-center/README.md)与[架构索引](../../doc/engineering/command-center/architecture/README.md)：日常阅读入口与已核验范围。
- [持续更新流程](../../doc/engineering/command-center/command-center.manual.md)：更新时机、责任、字段、来源核对、并行与失败处理。
- [项目入口地图](references/project-map.md)：定位当前主责来源、迁移关系和已有方法。
- [客户端实施方法](references/desktop-client.md)：把已确认设计转为实际客户端改动。
- [视图刷新方法](references/view-model.md)：采集、关联、回顾决定及生成简报。
- [Codex 外部接续方法](references/external-handoff.md)：保存准确定位，打开原会话或复制入口；不连接或控制运行时。
