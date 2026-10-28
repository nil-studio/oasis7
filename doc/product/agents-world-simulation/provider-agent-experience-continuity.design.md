# Agent/provider 体验连续性：执行方选择与共同游戏边界

## 文档身份

- 所属产品模块：智能体、世界模拟与交互
- 上位产品 PRD：[模块 PRD](prd.md)
- 配对产品 PRD：[Agent/provider 体验连续性](provider-agent-experience-continuity.prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-16
- 专业域权威：[Provider parity](../../world-simulator/llm/provider-agent-experience-parity.prd.md)
- 适用范围：专业域声明支持的官方内置/外部执行方、平台和玩家入口。本文是目标体验，不是当前支持或发布报告。
- 技术主责：[统一游戏客户端边界](../../world-simulator/llm/agent-game-client-boundary.design.md)；原在途意图技术语义仍由 [Provider Switch](../../world-simulator/llm/provider-switch-intent-continuity.design.md)拥有。

## 1. 玩家情境、目标与设计依据

玩家已有一个 Agent，先使用游戏提供的执行方完成工厂建设，之后希望交给熟悉的外部 Runtime 继续生产。玩家不应被要求理解内部模块拆分，也不应因换软件而重新认领 Agent、失去原成果或重复执行结果未知的请求。

设计将“谁来思考和提出行动”与“游戏允许什么、实际发生什么”分开。执行方选择影响策略、模型成本与运行依赖，不改变世界规则。产品要求由配对 PRD 声明一次；本页只说明信息、选择和体验反馈，不定义接口、签名、数据库或状态机。

Trace refs: O7-REQ-ad3ea59c-596c-4c50-8da8-fe47608c6f01（[REQ-AGENT-PARITY-003](provider-agent-experience-continuity.prd.md#req-agent-parity-003)）；O7-REQ-d452396d-291d-41a5-a912-6d615b58c29e（[REQ-AGENT-PARITY-004](provider-agent-experience-continuity.prd.md#req-agent-parity-004)）。

既有连续性要求继续适用。Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6（[范围与支持](provider-agent-experience-continuity.prd.md#req-agent-parity-001)）；O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5（[切换不重放](provider-agent-experience-continuity.prd.md#req-agent-parity-002)）。

## 2. 范围、选项与取舍

| 玩家选择 | 能获得什么 | 成本、条件与不能推导的结果 |
| --- | --- | --- |
| 官方内置执行方 | 游戏提供的目标理解与行动组织方式，通过现有启动体验使用 | 仍需实际模型配置、预算和游戏授权；官方不等于免费、额外权限或云托管 |
| 已支持的外部 Runtime | 保留熟悉的会话、工具与私有策略 | 依赖实际 Runtime 在线、版本兼容和授权；模型端点连通不等于该 Runtime 已接入 |
| 暂停后续委托 | 停止继续产生获授权的新行动，保留世界已提交事实 | 在途行动按其原权威状态处理；生产/转运等已受理作业不因暂停自动取消 |
| 先等待/核对再切换 | 避免把未知请求当成失败重做 | 可能等待原结果或权限恢复；不能承诺立即无损切换所有组合 |

不默认选中内置执行方，不新增第三种玩家访问模式。未支持的执行方组合须说明缺口，不展示为可用选项。私有记忆不承诺自动迁移；可恢复的是授权允许读取的世界任务、目标、行动和结果。

## 3. 正常流程与信息层级

### 3.1 选择与开始

先展示当前世界、玩家合法控制的 Agent、实际执行方及运行位置，再说明可用能力、模型费用/预算来源和委托范围。玩家可以保持、改选或暂不开始，不把“使用官方”提示成推荐越权通道。

连接配置完成、执行方已启动、委托有效、正在思考和世界已接受行动分别表达。就绪检查失败时保留玩家目标和当前成果，说明哪一项未满足及修复入口，不自动改用其他模型或 Agent。

源码拆分后的必要组件由正式启动路径组织。玩家不需要靠内部脚本名和端口猜测哪个程序缺失；手动诊断可以保留，但不是普通开始的默认步骤。

选择官方执行方后的网页自带模型入口由[自带模型连接产品要求](self-supplied-model-connection.prd.md)及其[配对体验设计](self-supplied-model-connection.design.md)承接。它细化模型选择、凭证接收方、会话与费用说明，不改变本页执行方选择、委托和在途结果的主责。该入口仍是目标设计，不表示当前网页已支持 DeepSeek/OpenRouter 或默认云托管。

### 3.2 运行与阅读结果

主信息是“当前目标、实际执行方、已发生的成果、阻塞与下一步”。连接/进程详情放在诊断层；不展示私有思考流来冒充进度。

模型提出建设、系统受理建设、工厂实际建成分别呈现；排产受理与首批产物完成也分别呈现。玩家可按原请求查看权威结果，不因为两个执行方产生相似文字就合并其成果。

关闭观战界面后，独立执行方只有在仍在线、具备预算和有效委托时才继续。回来时分别读取执行方状态与世界作业结果，不由浏览器连接推断 Agent 在运行。世界作业即使没有模型参与仍可继续，按玩法与 Runtime 的实际规则展示。

### 3.3 停止、切换与回来继续

切换前先说明影响的是后续新决策，以及哪些请求仍未知、已经受理或完成。玩家可等待核对、停止后续委托或使用明确支持的撤回能力；没有该能力就不承诺“取消全部”。

切换后仍是同一游戏 Agent 和合法任务；新执行方读取当前有效目标与权威结果。旧执行方迟到的建议不自动转交执行，未知请求也不作为新操作重试。恢复读取被拒绝时明确保留未知，不能让“重新连接”按钮暗中执行第二遍。

## 4. 异常、恢复与禁止误读

以下是玩家信息分类，不新增运行时 wire 状态。

| 情境 | 玩家知道什么 | 安全下一步 | 不得暗示 |
| --- | --- | --- | --- |
| 模型/Runtime 未连接或预算不足 | 哪个依赖不可用、已有成果仍有效 | 修复实际配置/预算或显式改选 | 已自动换模型、仍在思考 |
| 行动结果未知 | 原请求已送出但后果尚未核对 | 按原请求查询；权限不足时等待恢复 | 失败、取消、可直接重做 |
| 目标或执行委托变化 | 旧决策不再有新的执行许可 | 查看原结果后让有效执行方重新理解 | 切换追溯撤销所有世界效果 |
| 世界观察过期或报价变化 | 旧信息不能证明现在可执行 | 保留原结果，取得新观察后重排 | 报价预留材料、旧建议仍保价 |
| 执行方退出但作业继续 | 无新推理；世界已受理工作仍有进展 | 恢复获授权执行方或仅观察 | Viewer 在线等于 Agent 在线 |
| 不支持的能力/平台/组合 | 未覆盖范围和替代选择 | 保持受限或选择已验证组合 | 原路径自动兜底、所有能力天然等价 |

## 5. 可访问性与隐私

状态和下一步以文字表达，不仅依赖颜色、动画或连接图标。窄屏先保留目标、执行方、结果状态和主操作；展开诊断不应挤掉这些信息。键盘可完成选择、查看成本和停止确认；异步刷新不抢焦点，不丢未提交的目标编辑。

普通玩家表面不暴露模型密钥、owner/executor 私钥、完整私有目标给无权主体或私有推理流。显示预算事实时区分已报告费用、剩余许可和未知用量，未知不能写成零。网页与桌面分别消费相同产品语义，但各自取得适用交互证据。

## 6. 验收、证据与未决问题

| 产品要求 | 体验验证 | 专业验证与范围限制 |
| --- | --- | --- |
| [REQ-AGENT-PARITY-001](provider-agent-experience-continuity.prd.md#req-agent-parity-001) / [AC-AGENT-PARITY-001](provider-agent-experience-continuity.prd.md#ac-agent-parity-001) | 不支持组合保持窄承诺，玩家理解限制 | 具体 provider 支持仍由 parity authority；本页不发布新支持矩阵 |
| [REQ-AGENT-PARITY-002](provider-agent-experience-continuity.prd.md#req-agent-parity-002) / [AC-AGENT-PARITY-002](provider-agent-experience-continuity.prd.md#ac-agent-parity-002) | 切换前后的候选、未知、受理和完成清楚区分 | 原请求与世界后果由 Runtime/Provider Switch 验证，界面截图不代签 |
| [REQ-AGENT-PARITY-003](provider-agent-experience-continuity.prd.md#req-agent-parity-003) / [AC-AGENT-PARITY-003](provider-agent-experience-continuity.prd.md#ac-agent-parity-003) | 相同授权能力的官方/外部执行不呈现不同游戏规则 | 系统 AGC-01..05、07..09；真实模型与玩家体验另取证 |
| [REQ-AGENT-PARITY-004](provider-agent-experience-continuity.prd.md#req-agent-parity-004) / [AC-AGENT-PARITY-004](provider-agent-experience-continuity.prd.md#ac-agent-parity-004) | 完成选择、开始、离开、回来、预算失败、停止与恢复 | 系统 AGC-06、09..11，按实际平台与入口；不证明云托管 |

设计假设：正式启动路径能隐藏模块拆分的部署细节而不隐藏运行责任。反证信号是用户必须自行猜测多个进程、停止状态被误读或不能辨认费用来源；由产品 owner 与 launcher/viewer owner 调整交互，不能以技术拆分为理由删去信息。

未知项是每个实际发布组合的安装与运行证据，不是留给实现者自行决定的权限例外。当前设计未执行玩家、模型、平台或部署验收；后续实际实现与验证见 [#4726](https://github.com/eng-cc/oasis7/issues/4726)，具体测试步骤仍在专业设计与手册。
