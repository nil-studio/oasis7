# 客户端实施方法

按需读取本分册，把正式设计转换为客户端改动。产品形态、四视图和用户承诺以[产品设计](../../../doc/engineering/command-center/command-center.prd.md)为准；技术选型、组件与发布以[系统设计](../../../doc/engineering/command-center/command-center.design.md)为准。本分册不另维护功能清单或架构决定。

## 先定位本次改动

| 需要解决的问题 | 先读主责条款 |
| --- | --- |
| 启动、打开项目与首版接入范围 | PRD [REQ-CC-001](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-001)、[REQ-CC-012](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-012) |
| 目标、规范、执行和待处理页面 | PRD [REQ-CC-003](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-003) 至 [REQ-CC-006](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-006) |
| 外部会话定位、旧数据保留与退出 | PRD [REQ-CC-007](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-007) 至 [REQ-CC-009](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-009)，再读系统设计对应机制 |
| 中文交互、个人数据与隐私 | PRD [REQ-CC-010](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-010)、[REQ-CC-011](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-011) |

## 从目标设计推进到实现

1. 检查当前客户端目录、实际入口、manifest、连接器和测试，区分已经实现、待实现和没有核实的能力。计划目录不能被当成源码已存在的证据。
2. 在系统设计的代码边界内选择本次可用切片，复用适合的组件与仓库缓存策略。来源基础可局部交付，完整基础首版须覆盖 [日常工作闭环](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-013) 和 [已有工作接续](../../../doc/engineering/command-center/command-center.prd.md#REQ-CC-014)，不能把原文页或连接成功当作完整用户任务通过。只有涉及 Viewer 或 `site/` 本身的用户需求才修改这些入口；工程客户端继续拥有独立上下文。
3. 先让一个真实来源完成读取、定位与展示，再扩展页面和连接。浏览器预览采用明确的 fixture/replay；实际桌面权限与连接在真实 App 中核对，不能用预览成功代替桌面成功。
4. 按当前适用主责文档选择构建、UI 和桌面验证命令。Codex 外部接续按[接续方法](external-handoff.md)核对原定位与严格导航边界，不因演示需要启动外部推理。
5. 需要改变既定用户承诺时修改产品条款；机制或依赖取舍变化更新系统设计及受影响的映射。依赖升级是否要求重选由实际变化判断，不重复泛化调研。

## 验证与说明

使用正式 PRD 的对应 `AC-CC` 和系统设计的验证映射，覆盖本次变化造成的具体风险。涉及窗口、退出、恢复或更新时，以真实运行所有权和生命周期验证，不能只看窗口是否消失。

说明实际可启动入口、连接覆盖、数据基线及未完成部分。纯设计或 Skill 交付说明文档结果；实现交付给出实际验证与使用入口，不将“选型已经决定”表述为“客户端已经完成”。
