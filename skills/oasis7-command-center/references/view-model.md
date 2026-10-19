# 视图刷新与规范回顾方法

按需读取本分册完成来源采集、关联、刷新和简报。四视图及状态语义以[产品设计](../../../doc/engineering/command-center/command-center.prd.md)为准；数据身份、存储和一致性以[系统设计](../../../doc/engineering/command-center/command-center.design.md)为准。

持久项目摘要与架构索引按[持续更新流程](../../../doc/engineering/command-center/command-center.manual.md)维护；[项目状态](../../../doc/engineering/command-center/README.md)提供目标、进展、近期变化和人工关注，[架构索引](../../../doc/engineering/command-center/architecture/README.md)提供部署等关键边界。先核对它们的来源基线与时间，再核对有关原始来源并更新派生摘要，避免用摘要作为自身依据。

## 采集与刷新

1. 按[项目入口地图](project-map.md)确定本次仓库、工作树、提交、环境和时间范围。当前阶段从 core 的指定章节读取，长期方向沿产品四域下钻，避免加载无关全文。
2. 为每个重要结论保留可得的来源位置、源版本与观测时间。按 PRD [REQ-CC-002](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-002)区分事实、汇报、归纳及待核实关联；不将旧缓存标为刚刚读取。
3. 按[全局要求追踪规范](../../../doc/engineering/doc-governance/cross-layer-requirements-traceability.design.md#global-requirement-trace)复用来源中已声明的全局 Trace ID，并保留本地条款 ID、真实 path/fragment、PR/Issue URL 和会话身份。把只有 PR 的工作纳入结果；一个 PR 服务多个目标时复用同一工作来源，不复制任务。没有 Trace ID 的存量来源仍沿明确链接读取，不补造关系或要求全量回填；关联不明确时先给建议。
4. 按来源实际变化更新受影响的投影，并保留缺失和读取范围。文件刚被读取不代表其中的部署、PR 或验收事实刚核验；各来源成功时间分别保留。遇到分页中断、权限不足或离线时，继续处理可用区域，不把失败改为空结果，也不让迟到旧结果覆盖更新事实。
5. 计算进度前回到 PRD [REQ-CC-003](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-003)核对验收集合与适用证据；没有可核实分母时说明已知与未知部分。

Trace refs: O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6；本节承接关联消费边界。当前通过完整 ID 全文检索和人工追链核对产品或专业要求、适用设计及 Task/PR，包含已关闭 Issue 与已合入 PR。索引时延、权限或分页缺口须明确披露；共享 ID 不证明完整关联或验收通过。[只读追踪 CLI](../../../scripts/requirement-trace.README.md)已提供有界查询与可重建索引；它不等于文档树图形目标接口，也不自动判定设计覆盖或实现完成。消费[产品功能状态摘要](../../../doc/engineering/doc-governance/product-documentation-standard.design.md#feature-design-implementation-status)时，回核准确设计条款和 Task/PR/验证证据的基线、环境、验收及剩余范围，不将旧摘要当作当前事实。

## 回顾与记录决定

1. 根据事项找到当前主责条款，再沿显式后继与引用查看理由和历史。工程流程、产品承诺、技术合同和公开状态分别按职责解释，不制造全局统一优先级。
2. 遇到上传旧 `world-rule.md`、旧 `agent_world_*` 或历史 `.pm` 内容，沿当前导航找接收条款。无法确认继承关系时保留原始身份与影响，不自动迁入当前规则。
3. 用户明确要求修改决定时，优先编辑主责条款或已有决策记录，保留问题、决定、理由、范围和替代关系。仅需草稿时保存草稿；用户已决定而规范尚未同步时按 PRD [REQ-CC-004](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-004)解释。
4. 仅在存在真实阅读基线且能核实差异时总结“自上次查看的变化”，不推测用户读过什么。更新索引按正式 PRD 保留手工关联、阅读位置及个人决定记录。

## 生成四视图结果或简报

首页及同一工作的组织规则读取 [REQ-CC-013](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-013)、[REQ-CC-014](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-014)。先核对摘要、源引用和实际能力，再生成重点、成果、真实待处理与返回工作入口；无明确优先级保留并列，关系失效保留原依据。静态简报、文档可检索与完整客户端闭环分别说明。

- 先写当前阶段、近期交付和长期目标的关联，再说明目前进展、明确时间窗口内最近做了什么、未完成条件和下一步；每项重要结论保留来源入口。
- 顶层架构按问题导航，分别解释设计约定、已有实现和实际部署；人工关注区分需要决定、待补事实和知悉变化，已有决定保留来源与理由。
- 执行信息按 PRD [REQ-CC-005](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-005)关联工作目的与明确来源，分别解释 PR、外部会话定位和目标状态；检查详情在原 PR 的 GitHub 页面核实；外部工具负责执行状态。
- 按 PRD [REQ-CC-006](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-006)组织真正需要用户处理的问题，避免把已授权普通工作重新放进审批队列。
- 如只生成静态简报或页面，标出生成时间及数据覆盖；自动更新需要实际读取或订阅，不把 Skill 安装本身称为实时监控。

提交前按相关 `AC-CC` 检查来源能否打开、历史是否可辨、缺失是否解释清楚。只刷新简报不需要创建任务、Issue、分支、远程评论或新的模型运行。
