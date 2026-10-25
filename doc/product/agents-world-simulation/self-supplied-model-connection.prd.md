# 网页自带模型连接

## 文档身份

- 所属产品模块：智能体、世界模拟与交互
- 上位产品 PRD：[模块 PRD](prd.md)
- 上位专题：[Agent/provider 体验连续性](provider-agent-experience-continuity.prd.md#first-party-agent-client)
- 配对产品 design：[连接与运行体验](self-supplied-model-connection.design.md)
- 设计判定：`paired-design`。
- 生命周期：`active`
- 文档状态：目标产品合同，不是当前可用性公告。
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-25
- 专业域权威：[LLM/provider](../../world-simulator/llm/README.md)；[模型连接系统设计](../../world-simulator/llm/self-supplied-model-connection.design.md)
- 适用入口：受支持部署的主 Web `viewer`；Launcher 可复用同一配置入口，不产生新玩家访问模式。

本文拥有“玩家在网页使用自己的模型账户驱动第一方 Agent”的体验与验收。模型调用、凭证处理由 Agent 客户端侧负责；世界授权、任务、动作和结果仍由现有专业合同拥有。此处 BYOK 指玩家提供模型服务 Key，不等同于 OpenRouter 将其他厂商 Key 接入其账户的同名功能。

## 设计适用性与生命周期闭合

- 设计判定：`paired-design`。
- 配对关系：[同名产品设计](self-supplied-model-connection.design.md)承接凭证接收、检查、运行状态、停止和恢复的玩家体验。
- 设计适用性理由：跨越认证网页、本地持久凭证、浏览器执行、模型消费与世界授权，需共同说明同意、状态和失败边界。

## 1. 玩家问题与目标

### 1.1 代表性情境

玩家已有 DeepSeek 或 OpenRouter API 账户，希望给自己合法控制的游戏 Agent 指定目标。不应为了使用第一方执行方，先注册 LetAI、安装外部 Runtime、运行脚本、填写端口或修改配置文件。玩家先知道凭证交给谁、谁付费、执行在哪里，再决定是否连接和开始。

### 1.2 目标与成功含义

目标主路径是：进入受支持网页 → 选择第一方执行方 → 选择供应商、填写 Key 和确认模型 → 测试连接 → 确认有效任务/委托与运行限制 → 开始。已有合法 Agent、目标和授权可以复用，不要求重复录入；连接本身不认领 Agent、不授予世界权限、不开始持续计费运行。

成功必须同时区分模型可用、执行方可运行、游戏授权有效和世界实际结果；回答一句文本不等于 Agent 已经能玩游戏。

### 1.3 依据与假设

需求来自已有模型账户玩家的低摩擦接入场景。当前核对基线为 `nil-studio/oasis7@ed8d201059f84b8bebd04421e1fd0f56008bec11`：受控 Launcher 的网页模型连接和第一方执行链已交付，具体状态见下列功能点；普通玩家冷入口、两家真实供应商完整组合及网页端到端结果理解仍未由本轮证明。已有合法账号、SDK 设备登记、活动任务和执行端配置是当前受控入口的前置，模型 Key 不替代游戏授权。专业边界见[运行说明](../../world-simulator/llm/byok-model-consumption.runbook.md)。

## 2. 范围与 Non-Goals

首期目标为 DeepSeek 官方和 OpenRouter 的自带推理 Key、浏览器本地连接、模型兼容检查、开始/停止、换 Key/模型、失效恢复及脱敏诊断。提供经当前发行组合验证的供应商/模型预设，不承诺供应商目录里的所有模型可玩；无已验证预设时显示不可用而不是随意推荐。

目标架构改为玩家保持游戏网页打开，由浏览器直接请求所选供应商并运行第一方 Agent 循环；Key 仅持久保存在该站点 localStorage，发送时只交给选定供应商，不上传 Oasis7 后端。浏览器持有 Key，不代表世界服务授予任务权限。支持入口必须同时具备供应商 CORS、页面 CSP、浏览器执行适配与合法游戏接口；缺少任一条件先说明不可用，不以收集 Key 或服务端代理兜底。

刷新或重新打开页面可读取已保存配置，但不得自动付费检查或恢复运行。首期不承诺关页、浏览器退出、休眠或后台冻结时持续运行，也不使用 Service Worker、后台任务或服务器接管。跨设备同步 Key、自动充值、OC 兑换、通用代理平台与 OpenRouter OAuth 不在本次范围。

本次是目标合同变更：下列历史实现证据均属于 Launcher 托管临时凭证路径，不能证明浏览器直连、localStorage 或浏览器 Agent 已交付；浏览器目标路径尚未实现/验收。#4836 的目标映射和原件未知结果验收继续有效，文档修改不关闭该 Issue。

用户的供应商账户承担模型费用。绕开的是 LetAI 模型账户/充值前置，不是游戏内 Agent 认领、维护、资源、授权或执行规则；免费进入与可选服务的边界继续消费[玩家接入产品合同](../player-entry-distribution/free-entry-world-progression-and-recognition.prd.md)。

## 3. 玩家体验与决策

| 阶段 | 玩家知道什么 | 可以选择什么 | 代价/承诺 | 可观察结果 | 下一步 |
| --- | --- | --- | --- | --- | --- |
| 进入连接页 | 当前账号、Agent、执行位置、凭证接收方及部署是否支持 | 连接第一方执行方或保持原选择 | 不要求提交 Key 才能查看边界 | 不支持时说明原因且不显示可提交的密码框 | 选供应商或返回 |
| 配置 | 供应商、模型、本机保存范围、数据将发给谁 | 贴推理 Key、选择可用模型 | Key 保存在本站 localStorage，仅发往所选供应商；不索取平台管理 Key | 仅掩码，供应商地址无需手填 | 测试连接 |
| 检查 | 检查可能产生小额模型费用，但不写游戏世界 | 明确启动检查、取消或修正配置 | 有限检查调用，不自动循环消费 | 凭证、模型、工具能力分别反馈 | 就绪后确认开始 |
| 运行 | 有效目标/委托、当前执行方、模型费用来源和限制 | 开始、查看进展、停止新决策 | 后续调用使用自己的供应商额度 | 世界接受与实际成果分别显示 | 按回执继续或修复阻塞 |
| 更换/退出 | 对后续推理、保存凭证与在途行动分别有什么影响 | 换模型/Key、断开并忘记、重新连接 | 已发出的模型调用可能仍计费 | 原结果保留，新推理停止或待重新确认 | 核对原结果后恢复 |

失败、停止、预算耗尽和原行动结果未知不能被一个绿色“已连接”覆盖。错误应指向重新输入 Key、供应商账户处理、选择已支持模型、稍后重试、恢复授权或按原操作查询；不能静默改用 LetAI、其他用户或项目公共付费 Key。

详细信息层级、关闭网页与回来继续的体验见[配对产品设计](self-supplied-model-connection.design.md)。

## 4. 产品要求

<a id="req-byok-001"></a>

### REQ-BYOK-001：网页完成模型接入

Trace ID: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8

- 性质：目标要求。
- 适用条件：受支持 Web 部署中的已认证玩家。
- 要求：玩家必须能在同一模型连接入口选择 DeepSeek/OpenRouter、提交自己的推理 Key、选择已支持模型并检查连接；普通路径不依赖 LetAI 账户、脚本、本地工具或手填 API 地址。部署不支持时先解释缺口，不先收集凭证。供应商直连与浏览器执行能力独立于旧 Launcher `/v1/agent-client/*` 模型管理接口；不因这组旧接口缺失判定新路径必然不可用。
- 管理完整性：历史连接增长不能使当前有效连接无法查看、停止或断开；分页、历史保留范围和未读完必须可辨，不能把局部列表当作全部连接或把读取失败显示为空。此边界是目标要求，尚无本轮实现/容量验收证据。
- 理由：把部署和供应商细节留给系统，而不是转嫁给玩家。
- 上位承诺：[Agent 产品范围](prd.md#2-范围与玩家边界)；[第一方客户端](provider-agent-experience-continuity.prd.md#first-party-agent-client)。
- 专业权威：[DES-BYOK-001](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-001)。
- 验收：[AC-BYOK-001](#ac-byok-001)。
- 系统设计状态：已覆盖
- 系统设计依据：[DES-BYOK-001](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-001)及其验证映射承接本条目标；不代表实现或安全评审已通过。
- 实现状态：部分实现
- 实现与验证依据：[PR #4788](https://github.com/nil-studio/oasis7/pull/4788) 已交付主 Web Command Surface 中的模型连接面板、部署能力发现、供应商/模型预设、检查与显式开始；[PR #4812](https://github.com/nil-studio/oasis7/pull/4812)继续交付执行与恢复切片。当前连接入口和未配置部署拒绝接收 Key 的实现可定位到[面板源码](https://github.com/nil-studio/oasis7/blob/ed8d201059f84b8bebd04421e1fd0f56008bec11/crates/oasis7_viewer/software_safe_src/model_connection_panel.jsx)。
- 核对日期与基线：2026-10-17；需求与实现源码 `ed8d201059f84b8bebd04421e1fd0f56008bec11`；受控 Launcher/Web 入口的固定源码及既有 PR 证据审读，本轮未运行测试、模型、浏览器或部署；历史实跑只代表其原候选。
- 剩余范围：已交付连接表单与能力发现，不再列为未实现；当前仅核实受控 Launcher 接线。普通玩家冷入口到合法任务的完整组合、实际部署与两家真实供应商网页验收仍待取证。

<a id="req-byok-002"></a>

### REQ-BYOK-002：凭证去向与费用由玩家明确授权

Trace ID: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca

- 性质：目标要求。
- 适用条件：首次连接、换 Key、检查或持续推理前。
- 要求：产品必须说明实际凭证接收方、执行位置、模型数据接收方、本机持久保存边界和玩家供应商账户付费；Key 保存在当前站点 localStorage，刷新、关页后仍保留，直到玩家断开并忘记或清除站点数据；不得上传 Oasis7 后端、进入公开世界、日志、遥测、分享或导出，不跨游戏账号自动使用。保存前明确同源脚本可读、共享浏览器风险及供应商接收方，提供断开并忘记。退出账号停止运行并清除该账号本地 Key；只停止则保留 Key。检查/重试计入限制；未知费用不得显示为零，限额未获证实时不承诺供应商账户硬封顶。
- 理由：低摩擦不能通过隐瞒密钥托管和持续消费实现。
- 上位承诺：[Agent 产品范围](prd.md#2-范围与玩家边界)；[第一方客户端](provider-agent-experience-continuity.prd.md#first-party-agent-client)。
- 专业权威：[DES-BYOK-002](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-002)。
- 验收：[AC-BYOK-002](#ac-byok-002)。
- 系统设计状态：已覆盖
- 系统设计依据：[DES-BYOK-002](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-002)及其验证映射承接本条目标；不代表实现或安全评审已通过。
- 实现状态：部分实现
- 实现与验证依据：[PR #4788](https://github.com/nil-studio/oasis7/pull/4788) 与[PR #4812](https://github.com/nil-studio/oasis7/pull/4812) 已交付凭证接收方/执行位置/费用说明、会话内凭证、断开忘记、租约及预算约束；[运行说明](../../world-simulator/llm/byok-model-consumption.runbook.md)限定账号/设备边界、凭证销毁与未知用量保守计入。实现存在不代签全部凭证安全和跨账号组合验收。
- 核对日期与基线：2026-10-17；需求与实现源码 `ed8d201059f84b8bebd04421e1fd0f56008bec11`；受控 Launcher/Web 入口的固定源码及既有 PR 证据审读，本轮未运行测试、模型、浏览器或部署；历史实跑只代表其原候选。
- 剩余范围：历史局部安全评审/负向测试证据只支持原候选；当前真实部署、跨账号/多标签页、换 Key 与在途调用组合的完整覆盖尚未核实；路由与数据去向的全量产品覆盖未由本轮核实，不据局部实现声称全部完成。

<a id="req-byok-003"></a>

### REQ-BYOK-003：模型就绪不代替游戏授权或托管承诺

Trace ID: O7-REQ-84294c93-3275-4d19-8faf-389950b87464

- 性质：目标要求。
- 适用条件：玩家测试完成、准备开始及离开页面时。
- 要求：产品必须分别显示模型就绪、执行方状态、任务/Agent 授权和世界结果；开始使用玩家明确确认的第一方执行方、目标与限制。浏览器仅在页面存活、可见且当前运行许可有效时发起新推理/新提交；隐藏、断网、休眠、刷新或关闭后不得承诺持续运行，返回后先核对原件并明确恢复；世界已受理作业仍按其权威规则推进。
- 理由：玩家应知道运行的是谁、为什么能行动及离开后会发生什么。
- 上位承诺：[Agent 产品范围](prd.md#2-范围与玩家边界)；[第一方客户端](provider-agent-experience-continuity.prd.md#first-party-agent-client)。
- 专业权威：[DES-BYOK-003](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-003)。
- 验收：[AC-BYOK-003](#ac-byok-003)。
- 系统设计状态：已覆盖
- 系统设计依据：[DES-BYOK-003](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-003)及其验证映射承接本条目标；不代表实现或安全评审已通过。
- 实现状态：部分实现
- 实现与验证依据：[PR #4788](https://github.com/nil-studio/oasis7/pull/4788) 与[PR #4812](https://github.com/nil-studio/oasis7/pull/4812) 已交付模型/执行状态分离、显式开始、会话租约和服务端 fresh task fence 校验；当前[主控制面目标展示](https://github.com/nil-studio/oasis7/blob/ed8d201059f84b8bebd04421e1fd0f56008bec11/crates/oasis7_viewer/software_safe_src/viewer_app.jsx)及[实际投入/产出卡](https://github.com/nil-studio/oasis7/blob/ed8d201059f84b8bebd04421e1fd0f56008bec11/crates/oasis7_viewer/software_safe_src/industrial_starter_outcome_card.jsx)也已存在，各自实现不归为本轮新增。网页只提交 Agent 引用，由服务端读取当前签名任务，是现行授权边界；尚未证明所显示目标与实际任务目标的端到端对应，不能据此认定错误目标已执行。
- 核对日期与基线：2026-10-17；需求与实现源码 `ed8d201059f84b8bebd04421e1fd0f56008bec11`；受控 Launcher/Web 入口的固定源码及既有 PR 证据审读，本轮未运行测试、模型、浏览器或部署；历史实跑只代表其原候选。
- 剩余范围：网页所见目标与实际签名任务目标的对应、世界回执与本次运行的精确关联仍缺整合验收；已有目标与成果卡不等于任意 BYOK 未知原件的结果证明。没有证据把服务端 fresh fence 判作绕权；本次不增设客户端 task revision 合同。 后续整合与验收见 [Issue #4836](https://github.com/nil-studio/oasis7/issues/4836)。

<a id="req-byok-004"></a>

### REQ-BYOK-004：更换与断开不重放原行动

Trace ID: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34

- 性质：目标要求。
- 适用条件：更换供应商/模型/Key、停止、超时、断连、执行端重启或撤销凭证时。
- 要求：产品必须立即区分已停止新调用与尚待核对的历史结果；旧凭证不得再启动新请求，迟到模型建议不得自动进入新执行上下文。恢复时沿原操作核对世界结果，不因缺回执生成替代提交，不自动开始或扩大委托。断开本应用不等于已在供应商撤销原 Key。
- 理由：避免换模型、预算失败和重连导致重复动作或继续扣费的误解。
- 上位承诺：[Agent 产品范围](prd.md#2-范围与玩家边界)；[第一方客户端](provider-agent-experience-continuity.prd.md#first-party-agent-client)。
- 专业权威：[DES-BYOK-004](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-004)。
- 验收：[AC-BYOK-004](#ac-byok-004)。
- 系统设计状态：已覆盖
- 系统设计依据：[DES-BYOK-004](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-004)及其验证映射承接本条目标；不代表实现或安全评审已通过。
- 实现状态：部分实现
- 实现与验证依据：[PR #4812](https://github.com/nil-studio/oasis7/pull/4812) 已交付 Stop 运行代撤销、迟到结果隔离、原 operation 全槽核对、同 run 持久预算，以及严格过期 Context 以原身份 committed context_expired 收口后的新决策路径。上述能力与[运行说明披露的同 Context 必要历史恢复缺口](../../world-simulator/llm/byok-model-consumption.runbook.md)分别成立：provider conversation 仍仅存于实例，runner/provider 重建不恢复此前必要工具/会话历史。
- 核对日期与基线：2026-10-17；需求与实现源码 `ed8d201059f84b8bebd04421e1fd0f56008bec11`；受控 Launcher/Web 入口的固定源码及既有 PR 证据审读，本轮未运行测试、模型、浏览器或部署；历史实跑只代表其原候选。
- 剩余范围：同一未过期 Context 的必要工具/会话历史跨 runner/provider 生命周期仍有已知实现与验证缺口；同实例测试不能代签重建后恢复。网页未知原件的只读核对或明确入口限制/受支持安全下一步仍待整合验收，不能仅因缺查询按钮报新 bug，也不能把“刷新连接状态”或重新开始当作只读世界原件查询。Stop、持久预算、权限 fence 与过期收口已有实现，不因此降为未实现。 后续分别见[网页整合 #4836](https://github.com/nil-studio/oasis7/issues/4836)与[必要历史恢复 #4837](https://github.com/nil-studio/oasis7/issues/4837)。

<a id="req-byok-005"></a>

### REQ-BYOK-005：按供应商和模型能力分别证明可用

Trace ID: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d

- 性质：目标要求。
- 适用条件：声明支持、检查失败或运行退化时。
- 要求：DeepSeek 与 OpenRouter 必须分别验证模型协议、工具调用闭环和本产品场景；页面提供可行动的失败原因，并显式限制未覆盖模型/路由。连接测试只证明相应配置当时的技术检查，不代签完整可玩性、生产托管或发行就绪。
- 理由：防止“接口兼容”被误报为完整支持。
- 上位承诺：[Agent 产品范围](prd.md#2-范围与玩家边界)；[第一方客户端](provider-agent-experience-continuity.prd.md#first-party-agent-client)。
- 专业权威：[DES-BYOK-005](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-005)。
- 验收：[AC-BYOK-005](#ac-byok-005)。
- 系统设计状态：已覆盖
- 系统设计依据：[DES-BYOK-005](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-005)及其验证映射承接本条目标；不代表实现或安全评审已通过。
- 实现状态：部分实现
- 实现与验证依据：[PR #4788](https://github.com/nil-studio/oasis7/pull/4788) 与[PR #4812](https://github.com/nil-studio/oasis7/pull/4812) 已交付供应商/模型预设、工具协议能力检查及固定失败分类和网页安全诊断；历史受控 DeepSeek 实际调用只支持原候选/场景，不能代签 OpenRouter、全部模型路由或本轮主干的完整可玩性。
- 核对日期与基线：2026-10-17；需求与实现源码 `ed8d201059f84b8bebd04421e1fd0f56008bec11`；受控 Launcher/Web 入口的固定源码及既有 PR 证据审读，本轮未运行测试、模型、浏览器或部署；历史实跑只代表其原候选。
- 剩余范围：两家供应商及具体模型/协议/路由的独立工具闭环、真实网页和失败组合证据仍不完整；受控 DeepSeek、prepared StarterChain 或单一供应商成功不能证明完整支持或正式发行。

## 5. 验收与证据

<a id="ac-byok-001"></a>

### AC-BYOK-001：网页完成接入

Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8；覆盖 [REQ-BYOK-001](#req-byok-001)。

给定：受支持应用和已认证玩家，不配置 LetAI、不安装 CLI。

当：分别选择具备当前浏览器支持证据的供应商预设，贴推理 Key，检查并确认开始。

则：网页可完成配置与启动；缺浏览器执行、CORS/CSP 或合法游戏接口的部署不能收集 Key，也不会伪报可运行。另以超过一页的历史连接、当前有效连接、读取失败和跨账号访问验证管理完整性：分页范围可辨，当前连接仍可安全管理，失败不伪装为空；本项需独立取证，不沿用接入表单的历史通过结果。

<a id="ac-byok-002"></a>

### AC-BYOK-002：凭证、费用与用户隔离

Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca；覆盖 [REQ-BYOK-002](#req-byok-002)。

给定：两个账号、多个标签页，以及带有限制的测试凭证。

当：执行检查/重试、跨账号访问连接、断开并忘记、触发预算不足。

则：Key 按游戏账号命名空间保存在本站 localStorage，刷新后仍可读取但不自动调用；换号不使用旧账号 Key，退出/忘记删除对应 Key，其他页面停止旧代；模拟存储拒绝、配额不足和删除失败并明确提示。只有当前授权主体可使用连接；费用/未知用量与剩余许可分开；凭证不进入日志、分享、世界数据或他人请求；无公共 Key/自动充值兜底。

<a id="ac-byok-003"></a>

### AC-BYOK-003：有效执行与有界会话

Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464；覆盖 [REQ-BYOK-003](#req-byok-003)。

给定：模型检查成功，但分别构造无有效游戏委托和有有效委托两种情况。

当：尝试开始，再隐藏、冻结、关闭或刷新页面并回来。

则：无委托不能行动；有效运行通过同一游戏接口取得真实回执；失效后无新模型调用，已受理作业与模型运行状态不混淆；恢复需重新确认。

<a id="ac-byok-004"></a>

### AC-BYOK-004：切换、断开和未知结果恢复

Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34；覆盖 [REQ-BYOK-004](#req-byok-004)。

给定：模型调用或世界提交在途，且原响应可能迟到/丢失。

当：更换供应商/Key、重复停止、刷新或重开页面并重新连接。

则：旧调用不再取得新执行资格；已发生世界效果保留且不重放；未知结果继续按原操作查询；本应用断开与供应商 Key 撤销分开说明。

<a id="ac-byok-005"></a>

### AC-BYOK-005：独立支持与故障解释

Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d；覆盖 [REQ-BYOK-005](#req-byok-005)。

给定：固定版本、模型、协议、路由、浏览器和游戏场景。

当：两家分别执行观察→工具→合法行动→回执，并注入无效 Key、余额不足、限流、错误模型、畸形工具结果和流式中断。

则：每个结果有结构化错误与安全下一步；未覆盖组合保持不可用/实验性，文本连通和一个供应商成功不能代签另一个。

### 5.1 追踪

| 产品要求 REQ / 验收 AC | 上位承诺 | 专业 owner | 专业域权威 | 验证证据与边界 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| [REQ-BYOK-001](#req-byok-001) / [AC-BYOK-001](#ac-byok-001) | [第一方客户端](provider-agent-experience-continuity.prd.md#first-party-agent-client) | `viewer_engineer / agent_engineer` | [DES-BYOK-001](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-001) | [BYOK-01](../../testing/manual/self-supplied-model-connection.manual.md#byok-01)：计划验证，不声明已运行；真实调用另需授权 | `test_tier_full` |
| [REQ-BYOK-002](#req-byok-002) / [AC-BYOK-002](#ac-byok-002) | [第一方客户端](provider-agent-experience-continuity.prd.md#first-party-agent-client) | `agent_engineer / qa_engineer` | [DES-BYOK-002](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-002) | [BYOK-02 / BYOK-03](../../testing/manual/self-supplied-model-connection.manual.md#byok-02)：计划验证，不声明已运行；真实调用另需授权 | `test_tier_required` |
| [REQ-BYOK-003](#req-byok-003) / [AC-BYOK-003](#ac-byok-003) | [第一方客户端](provider-agent-experience-continuity.prd.md#first-party-agent-client) | `agent_engineer / runtime_engineer` | [DES-BYOK-003](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-003) | [BYOK-04 / BYOK-05](../../testing/manual/self-supplied-model-connection.manual.md#byok-04)：计划验证，不声明已运行；真实调用另需授权 | `test_tier_full` |
| [REQ-BYOK-004](#req-byok-004) / [AC-BYOK-004](#ac-byok-004) | [第一方客户端](provider-agent-experience-continuity.prd.md#first-party-agent-client) | `agent_engineer / runtime_engineer` | [DES-BYOK-004](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-004) | [BYOK-06](../../testing/manual/self-supplied-model-connection.manual.md#byok-06)：计划验证，不声明已运行；真实调用另需授权 | `test_tier_full` |
| [REQ-BYOK-005](#req-byok-005) / [AC-BYOK-005](#ac-byok-005) | [第一方客户端](provider-agent-experience-continuity.prd.md#first-party-agent-client) | `agent_engineer / qa_engineer` | [DES-BYOK-005](../../world-simulator/llm/self-supplied-model-connection.design.md#des-byok-005) | [BYOK-07 / BYOK-08](../../testing/manual/self-supplied-model-connection.manual.md#byok-07)：计划验证，不声明已运行；真实调用另需授权 | `test_tier_full` |

### 5.2 效果与证据范围

每份验收绑定实际源码/构建、供应商、模型、协议、路由策略、执行部署、浏览器、场景和窗口；真实调用需显式授权使用测试 Key 与费用预算。禁止把原始 Key、真实私有目标或供应商完整错误体写进证据。mock 合同验证、真实供应商验证和真实游戏结果分别记录；本文件不预填 PASS，也不修改公开发行状态。

### 5.3 浏览器目标路径的新增验收边界

沿用 REQ-BYOK-001～005 与对应 Trace ID，不建立第二套世界权限。AC-BYOK-001/005 须在真实外部浏览器按供应商、模型、协议分别验证 CSP、CORS 预检和实际响应、工具往返；脚本附带 Origin 成功不能代签浏览器。AC-BYOK-002 须检查 localStorage 的保存、拒绝/损坏/删除失败、账号隔离、无 Key 上传世界或应用后端及安全诊断。AC-BYOK-003/004 须覆盖多标签页唯一执行、目标/owner/executor fence 改变、隐藏与恢复、关页未送达事件、刷新后原件扫描、预算连续及迟到结果隔离。

对响应丢失但已生效、Pending、Rejected、HistoryUnavailable 和部分结算分别证明准确归属与安全下一步；只读核对不调用模型、不重发动作、不换 nonce、不创建替代 Context、不重置预算。能力缺失时明确本页限制并阻塞新效果，不能用连接刷新、模型成功、successful_effects 或最新快照作原件结算证明。

## 6. 权威与相邻专题

体验主责在本产品模块；[玩家接入与发行](../player-entry-distribution/prd.md)拥有登录、模式与受支持应用入口，Launcher 只复用入口和投影。仓库工程指挥面板不是面向玩家的凭证后台。第一方 Harness 与共享 Game Client 的依赖边界由[统一客户端设计](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-001)拥有，模型供应商不成为新的游戏执行方身份。

已有切换不重放及执行方透明度不重复立法：Trace refs: O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5（[既有切换要求](provider-agent-experience-continuity.prd.md#req-agent-parity-002)）；O7-REQ-d452396d-291d-41a5-a912-6d615b58c29e（[既有执行方要求](provider-agent-experience-continuity.prd.md#req-agent-parity-004)）。本专题只增加模型连接、凭证生命周期与网页组合条件。

## 7. 设计取舍与未决问题

采用浏览器直接请求供应商并运行第一方 Agent 循环，Key 仅持久保存在本站 localStorage；旧可信服务端会话处理和 Key 不落盘的选择已被本目标取代。刷新后保留配置可减少重复输入，但同源脚本可读取 Key，共享设备和站点脚本安全由玩家与部署方共同承担，页面须在保存前明确告知。模型请求只发往已验证的供应商预设，游戏授权仍走既有签名与任务合同；CSP、供应商 CORS 或浏览器能力不足时禁用，不自动退回服务端处理。无人值守托管、跨设备保存与 OAuth 仍留待后续独立设计。

待取得的事实是具体部署安全评审、两家所选模型兼容性及真实玩家体验，而不是让实现者自由决定凭证去向、失败兜底或世界授权例外。
