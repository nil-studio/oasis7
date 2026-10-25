# 网页自带模型连接：产品设计

## 文档身份

- 所属产品模块：智能体、世界模拟与交互
- 上位产品 PRD：[模块 PRD](prd.md)
- 配对产品 PRD：[网页自带模型连接](self-supplied-model-connection.prd.md)
- 生命周期：`active`
- 文档状态：目标体验，不声明已经上线。
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-25
- 专业域权威：[模型连接系统设计](../../world-simulator/llm/self-supplied-model-connection.design.md)；[统一客户端](../../world-simulator/llm/agent-game-client-boundary.design.md)
- 关联体验：[执行方选择与连续性](provider-agent-experience-continuity.design.md)

## 1. 设计命题

让已有模型 API 账户的玩家在游戏网页完成连接，不先成为部署工程师。第一层只呈现供应商、Key、模型和检查；凭证接收方、浏览器执行位置、本机保存和费用说明不能藏进高级诊断。选择“第一方执行方”与选择“模型供应商”是两个不同决定，不能把 DeepSeek 文本接口标成一个已接入的外部 Agent Runtime。

目标是玩家保持网页打开，由页面直接请求供应商并运行 Agent。Key 保存到本站 localStorage，Oasis7 后端不接收 Key；保存不等于运行授权。CORS、CSP、浏览器执行或合法游戏接口缺失时先说明限制。本设计替代原托管临时凭证目标，历史实现尚未迁移，不能据此宣称上线。

## 2. 代表性片段

玩家在自己的 Agent 页面选择第一方执行方，看到“模型连接”，选择 DeepSeek，粘贴专用推理 Key。模型选项来自本部署已支持配置；页面说明检查会有有限调用费用。检查通过后显示“模型可用，尚未开始”，保留既有目标和授权，玩家确认本次运行限制再开始。

之后玩家选择 OpenRouter，不需要重建游戏 Agent。界面先说明切换只影响后续推理，并保留原行动的未知/已接受/已完成状态。新连接检查完成前，原连接不会被新草稿无声替换；正式切换时停止旧执行资格，原世界结果仍可追读。

## 3. 机制与体验关系

<a id="experience-connect"></a>

### 3.1 连接入口与字段

Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8；承接 [REQ-BYOK-001](self-supplied-model-connection.prd.md#req-byok-001)。

入口落在主 Web 的“我的 Agent → 执行方 → 模型连接”，账号设置可提供同一组件的管理入口；Launcher 只打开/复用该入口，不维护第二份模型配置。静态官网只展示受支持应用链接；具有浏览器直连与合法游戏接口的 Web 应用不依赖 Launcher 模型托管接口。缺少必要能力时不显示可提交的 Key 表单。正式位置可随现有布局调整，但始终绑定当前玩家、目标 Agent 与实际执行方。

| 信息/控件 | 默认行为 | 玩家可见约束 |
| --- | --- | --- |
| 供应商 | 玩家明确选择 DeepSeek 官方或 OpenRouter | 不是改游戏模式，不自动改执行方或升级授权 |
| API Key | 密码输入，不自动读取剪贴板、不回显已提交值 | 只接受推理用途；不索取供应商管理 Key、世界 signer 或 LetAI 平台凭证 |
| 模型 | 本部署当前验证的预设；显示完整模型身份 | 允许改选已支持模型；未覆盖项标注不可用，不默选“目录第一个” |
| 执行/费用说明 | Key 输入前展示供应商接收方、浏览器执行、本地持久保存和付费账户来源 | 说明本站同源脚本可读取 localStorage；Key 不上传 Oasis7 后端 |
| 测试连接 | 显式操作，按钮防重复提交 | 提示有限调用费用；只做非世界写入检查 |
| 开始 | 检查后仍单独确认；复用已有目标/委托信息 | 只运行确认的 Agent、任务和限制；无权限则解释阻塞 |

<a id="experience-secrets"></a>

### 3.2 凭证和费用信息

Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca；承接 [REQ-BYOK-002](self-supplied-model-connection.prd.md#req-byok-002)。

默认文案明确“Key 保存在此浏览器的本站数据中，刷新或关页后仍保留；仅发送给所选供应商”。保存前提示同源脚本、XSS 和共享浏览器风险；不宣称加密保险箱或仅内存保存。提交成功后清空原始输入，不在页面源码、可复制诊断、分享链接或普通运行详情中展示 Key。供应商账号余额、应用估算费用、已报告用量和允许继续运行的预算不是同一数值；缺失值显示未知。

OpenRouter 的模型名称不等于底层提供商。连接摘要展示实际允许的路由范围与数据去向约束；配置变更可能改变成本或数据接收方时重新确认，不能以自动回退绕过原选择。产品不声称能控制同一 Key 在其他应用中的消费。

## 4. 循环与成长

<a id="experience-run"></a>

Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464；承接 [REQ-BYOK-003](self-supplied-model-connection.prd.md#req-byok-003)。

连接后回到既有“目标 → 观察 → 提议 → 世界裁决 → 结果 → 调整”循环，不新建第二份聊天任务或独立玩法。页面分别显示模型状态、第一方执行方状态、游戏授权以及已发生成果；推理文本和 token 增长不冒充工业进展。

页面可见且玩家已确认时才开始/继续新的推理和世界提交。隐藏、断网、休眠恢复或执行资格变化后暂停；返回先核对原件，再由玩家明确恢复，不因 Key 仍在就自动启动。关页不依赖 unload 送达来停止远端循环，因为不存在远端模型循环；已发供应商请求可能仍计费，世界已受理作业照常推进。多标签页同一 Agent 只允许一个活动执行页，其他页面可查看与停止，不能静默接管。

## 5. 行为与后果

<a id="experience-switch"></a>

Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34；承接 [REQ-BYOK-004](self-supplied-model-connection.prd.md#req-byok-004)。

“停止新决策”保留 localStorage Key，重新开始仍需确认；“断开并忘记”停止当前运行并删除该账号保存的 Key，通知其他同源页面停止旧代。删除失败明确显示“已停止，但本机 Key 尚未删除”，提供清除站点数据指引；退出账号也停止并清除该账号 Key。两者都不表示取消所有世界作业、撤销已发送的模型请求费用或删除供应商处的 Key。需要上游撤销时指向供应商账户操作，而不伪报本应用已经完成。

修改模型/供应商先形成草稿；新配置检查失败保留已确认连接及其真实运行状态，玩家也可先停止再配置。应用新配置时展示实际切换结果，旧模型迟到建议不取得新授权。多个页面编辑冲突时提示配置已变化并要求刷新，不用最后到达的请求静默覆盖。

## 6. 资源与平衡影响

模型费用由玩家自己的供应商账户承担；本次连接不兑换 OC，也不改变 Agent 认领维护、材料报价、任务委托和世界执行成本。模型更贵、速度更快或推理更强不能授予更多世界权限。运行限制不足时停止新的推理，不假装通过公共 Key 或免费策略继续完成任务。

## 7. 失败、阻塞与恢复

<a id="experience-failures"></a>

Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d；承接 [REQ-BYOK-005](self-supplied-model-connection.prd.md#req-byok-005)。

| 情况 | 首要说明 | 主动作 | 保留什么 |
| --- | --- | --- | --- |
| Key 无效/已撤销 | 模型认证失败，不等于游戏账号失效 | 重新输入或去供应商检查 | 世界成果、原操作身份 |
| 余额/本次预算不足 | 是供应商额度问题还是本应用运行限制 | 供应商处理或明确调整本次限制 | 未知费用和在途结果，不自动续费 |
| 模型/工具不兼容 | 该具体模型组合未就绪 | 改选已验证配置 | 玩家目标，不伪装为普通聊天成功 |
| 限流/服务不可用 | 重试受本次预算和期限限制 | 显式重试或停止 | 原世界请求，不复制行动 |
| 会话过期/执行端退出 | 不再产生新推理，但世界可能继续 | 重新认证/连接并核对原结果 | 已提交和未知结果 |
| 模型就绪但无有效游戏委托 | 不能代表当前 Agent 行动 | 进入合法授权/任务入口 | Key 状态与授权状态分开 |
| 断开已确认，原结果仍未知 | 新调用已停止，历史后果待核对 | 按原操作查看 | 未知不能改成取消或失败 |

状态优先级：先明确安全/授权阻塞与“已停止新推理”，同时保留原行动结果；随后显示预算、供应商退化与连接诊断。颜色、动画或成功 toast 不得覆盖这两个正交事实。

## 8. 表现与可访问性

键盘可完成供应商/模型选择、Key 输入、检查和停止；字段有可读标签，错误与字段关联且不重复朗读 Key。窄屏优先保留当前 Agent、凭证去向、付费说明和主动作；检查完成不抢焦点，普通异步刷新不清除未提交草稿。来自供应商的错误文本按不可信文本呈现，不注入 HTML，也不直接展示可能包含请求内容的原始错误体。

## 9. 取舍与验证

| 体验要求 | 配对验收 | 反证信号 |
| --- | --- | --- |
| [REQ-BYOK-001](self-supplied-model-connection.prd.md#req-byok-001) | [AC-BYOK-001](self-supplied-model-connection.prd.md#ac-byok-001) | 仍需要脚本、LetAI 注册或猜端口 |
| [REQ-BYOK-002](self-supplied-model-connection.prd.md#req-byok-002) | [AC-BYOK-002](self-supplied-model-connection.prd.md#ac-byok-002) | 玩家误以为 Key 不发给供应商或被安全隔离于同源脚本，或误以为估算是硬封顶 |
| [REQ-BYOK-003](self-supplied-model-connection.prd.md#req-byok-003) | [AC-BYOK-003](self-supplied-model-connection.prd.md#ac-byok-003) | 连接成功被误读为已授权、关页仍永久运行 |
| [REQ-BYOK-004](self-supplied-model-connection.prd.md#req-byok-004) | [AC-BYOK-004](self-supplied-model-connection.prd.md#ac-byok-004) | 切换/重连重复动作，断开被误读为上游 Key 已撤销 |
| [REQ-BYOK-005](self-supplied-model-connection.prd.md#req-byok-005) | [AC-BYOK-005](self-supplied-model-connection.prd.md#ac-byok-005) | 文本可回答被当成游戏工具已兼容 |

首期接受 localStorage 持久保存的同源脚本可读风险，明确告知并限制脚本来源；不做服务器密钥托管。存储不可用时不假装保存成功，也不悄悄改为上传后端。具体模型组合与部署证据是待核实事实；本设计不预设真实调用成功、默认启用或发布。专业合同与故障验证见[系统设计](../../world-simulator/llm/self-supplied-model-connection.design.md#verification-mapping)。
