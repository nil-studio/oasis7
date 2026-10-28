# Agent 对话与 Prompt 控制产品设计

## 文档身份

- 结构裁剪：理由：体验设计按信息和交互顺序组织，验证回到产品映射；范围：既有Prompt交互与恢复，不转成专业系统合同；风险：applied不可扩大为持久化或同步；映射：[目标与设计命题](#1-设计原则)、[代表性流程](#3-关键交互)、[验证追踪](#7-产品要求与验收映射)、[决策与风险边界](#6-设计边界)

- 配对产品 PRD：[`doc/product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md`](agent-conversation-and-prompt-control.prd.md)
- 上位产品 PRD：[`doc/product/agents-world-simulation/prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-09-13
- 专业域权威：[`world-simulator` 专业 PRD](../../world-simulator/prd.md)、[`Viewer 手册`](../../world-simulator/viewer/viewer-manual.manual.md)

本文定义跨 surface 的产品交互设计，不冻结历史 EGUI 布局或实现字段。当前可用入口、鉴权、协议和操作步骤以 [`Viewer 手册`](../../world-simulator/viewer/viewer-manual.manual.md) 与 [`world-simulator` 专业 PRD](../../world-simulator/prd.md) 为准。

## 1. 设计原则

1. 先确认对象，再表达意图：Agent 身份和玩家是否有权操作必须先于输入与应用动作。
2. 对话与配置分层：低风险的一次消息不能与持续影响后续行为的 Prompt/目标调整共用模糊提交语义。
3. 草稿不冒充结果：选择预设、填充、编辑、发送、accepted 与 applied 是不同阶段。
4. 默认值可解释：玩家能看懂当前生效值、默认值、override 与本地草稿之间的关系。
5. 能力按证据收窄：没有 preview、rollback、持久化或完整回执时，不显示或暗示对应保证。

## 2. 信息架构

正式 surface 按以下逻辑层级组织，但不要求固定在某个面板：

1. 当前 Agent：名称/标识、控制资格或 blocked 原因。
2. 交互类型：对话或 Prompt/目标调整。
3. 当前内容来源：默认、当前生效、override、本地草稿或预设。
4. 编辑与主动作：填充、发送、恢复默认、预览、应用或回滚，仅展示当前 authority 支持的动作。
5. 结果与恢复：accepted/applied/rejected/blocked、影响范围、原因和下一步。

预设是草稿辅助层，不与权威 profile 并列成第二事实源。Prompt/目标调整属于高影响层，应比普通对话提供更明确的目标、差异和结果说明。

## 3. 关键交互

### 3.1 预设填充

- 选择预设后先进入可编辑草稿；玩家仍需显式发送。
- 新增、编辑或删除预设只改变当前专业 authority 所声明的预设存储范围。
- 若预设不持久化，应在离开或刷新前说明，而不是让玩家丢失预设后才知道。

### 3.2 加载与编辑 Agent Prompt

- 加载时绑定当前 Agent，并标明值的来源。
- 切换 Agent 时，未提交草稿必须被保留、确认丢弃或明确重载，不能静默应用到另一个 Agent。
- 恢复默认应表达为删除/清除 override 或等价专业语义；清空输入不能在无说明时同时代表“空值”和“恢复默认”。

### 3.3 外部变更与过期草稿

- 编辑开始时记录目标 Agent、当前生效值和当前控制/授权范围作为比较基准；它只用于发现变化，不是提交凭证。
- 先校验目标 Agent 与当前控制/授权，再判断当前值是否变化；控制/授权丢失时使用 `blocked/control-lost`，它优先于值变化产生的 `stale` / `needs refresh`。
- 若 Agent 转让、撤销或玩家控制资格发生变化，草稿只能保持未提交并显示不泄露权威内容的安全原因；不得显示最新生效值或差异，也不得提供该草稿的提交、重放或转移路径。玩家可以放弃草稿或进入另一个明确授权的恢复/重新选择流程。
- 只有控制/授权仍有效、且另一 surface 或有权主体在编辑期间改变当前值时，草稿才进入 `stale` / `needs refresh` 或等价语义。此时保留本地草稿并展示最新生效值、变化原因和可用的查看差异、刷新/重载、重新编辑或放弃动作；禁止静默合并、覆盖最新值或自动重试。
- 重新校验通过后，玩家的重新编辑才形成新的独立提交。若提交与其他变更竞态，surface 只呈现 authority 的真实结果，不能先显示 applied 再回滚，也不能由客户端重复提交。

### 3.4 提交与反馈

- 普通对话使用直接发送反馈；Prompt/目标调整使用更强的目标确认和结果状态。
- 仅当专业 authority 返回实际应用结果时才能显示 applied。只有 acceptance 时，继续显示 pending/accepted 或等价状态。
- applied 只表示当前 authority 已确认的本次应用范围；除非另有对应 authority 证据，入口不得把它外推为永久保存、跨会话持久化或跨入口同步，并应让玩家看见该范围边界。
- 对于非 `control-lost` 的 rejected/blocked，必须以玩家可理解的摘要保留原因，并提供重新鉴权、刷新当前值、修正输入、选择受支持模式或稍后重试等真实下一步；`control-lost` 只能沿 3.3 的 authority-safe 路径处理。当前 canonical Web surface 将高级 Prompt 设置默认收起，并在展开后提供 preview/apply/rollback 与最近反馈；这不自动要求其他 surface 具有同等控件。

## 4. 响应式与输入设计

- 内容按“Agent → 类型 → 草稿 → 主动作 → 结果”保持稳定顺序；窄屏可通过分组、渐进披露或内部滚动压缩空间。
- 主动作和结果不得被滚动区永久遮蔽；高影响应用动作与普通发送在位置、文案或确认反馈上应可辨认。
- 输入区必须避免 IME、快捷键、换行、焦点和发送动作互相误触。具体快捷键及已验证的输入法行为由当前 surface authority 决定并在手册中说明；未定义快捷键时，正常文本编辑与显式发送动作仍须可用。
- 响应式布局只需保持目标 Agent、草稿、主动作和结果可达，不冻结“最右侧面板”“记录在上、输入在下”或双面板宽度等历史 EGUI 结构。
- 诊断细节按需展开，不能盖过目标 Agent、当前值、主要动作和结果。

- 中文组合态保持为编辑过程，完成后才形成草稿文本；浏览器事件时序差异不得造成重复文字或发送。焦点切换区分编辑对象和世界控制，输入区操作不触发世界快捷键/拾取。
- 发送保留 pending、可见失败原因和显式恢复，页面保持可操作且不得自动重放；正文与反馈保留中英文可读和文本复制。此为设计验收目标，完整 IME 与无卡死由专业证据确认。

## 5. 状态模型

| 阶段 | 玩家可见含义 | 不得外推 |
| --- | --- | --- |
| `draft` | 内容只在当前编辑面 | 已发送、已保存 |
| `filled` | 预设已复制到草稿 | 已发送、已应用 |
| `blocked/control-lost` | 目标或控制/授权已失效；草稿仍是未提交的本地内容，只显示 authority-safe 原因 | 最新权威值/差异、提交、重放、转移或已应用 |
| `stale/needs-refresh` | 控制/授权仍有效，但加载基准已被外部值取代；草稿仍是本地候选 | 已合并、已覆盖最新值、已应用 |
| `submitted/accepted` | 请求已被入口或 authority 接受 | runtime 已应用、已持久化 |
| `applied` | 当前 authority 确认变更生效 | 永久保存、跨入口完全一致 |
| `rejected/blocked` | 非 `control-lost` 的拒绝或阻塞；未应用，并给出原因/下一步 | 静默 fallback 或假成功 |

具体状态名和协议枚举可以不同，但玩家语义必须保持等价。

## 6. 设计边界

- 历史“最右 Chat Panel”“Prompt Presets 折叠区”只是设计来源，不是当前布局规范。
- 历史 Enter 发送、中文 IME bridge、左侧历史栏清理和 EGUI 焦点信号只作为迁移来源，不构成当前 Web 的快捷键、输入法或聚焦保证。
- 本设计不决定 profile schema、默认常量、patch 算法、服务端事件来源或强认证方式。
- 本设计不要求恢复已退役 Prompt Ops，也不要求所有 surface 暴露 Agent Prompt 修改能力。
- 技术实现、当前支持范围和验证命令继续由 `world-simulator` 专业文档与证据承载。

## 7. 产品要求与验收映射

本 design 不另造一套产品要求；它为配对 PRD 的 `REQ-PROMPT-*` 提供玩家经历和状态解释。配对 PRD 的 `AC-PROMPT-*` 是验收场景入口，以下映射确保原有设计义务有稳定下钻路径：

| 设计内容 | 配对 PRD 叶子要求 | 验收场景 |
| --- | --- | --- |
| Trace refs: O7-REQ-14ddd9ba-2d94-48e8-b2e4-0dbae26ddc36；对象确认、控制资格、对话/配置分层 | [REQ-PROMPT-001](agent-conversation-and-prompt-control.prd.md#req-prompt-001) / [REQ-PROMPT-002](agent-conversation-and-prompt-control.prd.md#req-prompt-002) | [AC-PROMPT-001](agent-conversation-and-prompt-control.prd.md#ac-prompt-001) / [AC-PROMPT-002](agent-conversation-and-prompt-control.prd.md#ac-prompt-002) |
| Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293；预设、草稿、默认、override、清除默认 | [REQ-PROMPT-002](agent-conversation-and-prompt-control.prd.md#req-prompt-002) / [REQ-PROMPT-003](agent-conversation-and-prompt-control.prd.md#req-prompt-003) | [AC-PROMPT-002](agent-conversation-and-prompt-control.prd.md#ac-prompt-002) / [AC-PROMPT-003](agent-conversation-and-prompt-control.prd.md#ac-prompt-003) |
| Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111；accepted/applied 的分界、applied 范围、失败原因与下一步 | [REQ-PROMPT-004](agent-conversation-and-prompt-control.prd.md#req-prompt-004) / [REQ-PROMPT-010](agent-conversation-and-prompt-control.prd.md#req-prompt-010) / [REQ-PROMPT-011](agent-conversation-and-prompt-control.prd.md#req-prompt-011) | [AC-PROMPT-004](agent-conversation-and-prompt-control.prd.md#ac-prompt-004) / [AC-PROMPT-010](agent-conversation-and-prompt-control.prd.md#ac-prompt-010) / [AC-PROMPT-011](agent-conversation-and-prompt-control.prd.md#ac-prompt-011) |
| Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd；过期草稿、控制权丢失优先和信息可见性 | [REQ-PROMPT-007](agent-conversation-and-prompt-control.prd.md#req-prompt-007) / [REQ-PROMPT-008](agent-conversation-and-prompt-control.prd.md#req-prompt-008) | [AC-PROMPT-007](agent-conversation-and-prompt-control.prd.md#ac-prompt-007) / [AC-PROMPT-008](agent-conversation-and-prompt-control.prd.md#ac-prompt-008) |
| Trace refs: O7-REQ-92124092-6021-49e4-8def-33038e20b637；提交时重新校验、竞态与单一结果 | [REQ-PROMPT-009](agent-conversation-and-prompt-control.prd.md#req-prompt-009) | [AC-PROMPT-009](agent-conversation-and-prompt-control.prd.md#ac-prompt-009) |
| Trace refs: O7-REQ-b6140b6c-ca2c-4734-84ce-7963f9385fff；响应式、IME、焦点和历史布局边界 | [REQ-PROMPT-005](agent-conversation-and-prompt-control.prd.md#req-prompt-005) / [REQ-PROMPT-006](agent-conversation-and-prompt-control.prd.md#req-prompt-006) | [AC-PROMPT-005](agent-conversation-and-prompt-control.prd.md#ac-prompt-005) / [AC-PROMPT-006](agent-conversation-and-prompt-control.prd.md#ac-prompt-006) / [AC-7](agent-conversation-and-prompt-control.prd.md#ac-7) |

这些链接只表达产品语义和验收追踪；profile schema、协议枚举、patch、鉴权和组件实现仍由本设计开头所列专业 authority 拥有。
## PRD REQ/AC fragment mapping

This design delegates product requirements and acceptance semantics to its paired PRD. Every row below is a navigable fragment mapping; this design does not create a second requirement authority.

| PRD requirement | PRD acceptance |
| --- | --- |
| Trace refs: O7-REQ-14ddd9ba-2d94-48e8-b2e4-0dbae26ddc36；[REQ-PROMPT-001](agent-conversation-and-prompt-control.prd.md#req-prompt-001) | [AC-PROMPT-001](agent-conversation-and-prompt-control.prd.md#ac-prompt-001) |
| Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293；[REQ-PROMPT-002](agent-conversation-and-prompt-control.prd.md#req-prompt-002) | [AC-PROMPT-002](agent-conversation-and-prompt-control.prd.md#ac-prompt-002) |
| Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d；[REQ-PROMPT-003](agent-conversation-and-prompt-control.prd.md#req-prompt-003) | [AC-PROMPT-003](agent-conversation-and-prompt-control.prd.md#ac-prompt-003) |
| Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111；[REQ-PROMPT-004](agent-conversation-and-prompt-control.prd.md#req-prompt-004) | [AC-PROMPT-004](agent-conversation-and-prompt-control.prd.md#ac-prompt-004) |
| Trace refs: O7-REQ-b6140b6c-ca2c-4734-84ce-7963f9385fff；[REQ-PROMPT-005](agent-conversation-and-prompt-control.prd.md#req-prompt-005) | [AC-PROMPT-005](agent-conversation-and-prompt-control.prd.md#ac-prompt-005) |
| Trace refs: O7-REQ-6f348a2d-cb87-4838-8447-21c8eebf6d07；[REQ-PROMPT-006](agent-conversation-and-prompt-control.prd.md#req-prompt-006) | [AC-PROMPT-006](agent-conversation-and-prompt-control.prd.md#ac-prompt-006) |
| Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd；[REQ-PROMPT-007](agent-conversation-and-prompt-control.prd.md#req-prompt-007) | [AC-PROMPT-007](agent-conversation-and-prompt-control.prd.md#ac-prompt-007) |
| Trace refs: O7-REQ-a9797fdb-39f7-48fb-9202-9700d9d86159；[REQ-PROMPT-008](agent-conversation-and-prompt-control.prd.md#req-prompt-008) | [AC-PROMPT-008](agent-conversation-and-prompt-control.prd.md#ac-prompt-008) |
| Trace refs: O7-REQ-92124092-6021-49e4-8def-33038e20b637；[REQ-PROMPT-009](agent-conversation-and-prompt-control.prd.md#req-prompt-009) | [AC-PROMPT-009](agent-conversation-and-prompt-control.prd.md#ac-prompt-009) |
| Trace refs: O7-REQ-3bbcade1-3f73-4899-b324-f857fd27b245；[REQ-PROMPT-010](agent-conversation-and-prompt-control.prd.md#req-prompt-010) | [AC-PROMPT-010](agent-conversation-and-prompt-control.prd.md#ac-prompt-010) |
| Trace refs: O7-REQ-36cc9e9d-eaa7-436f-b16b-6f6af089b04c；[REQ-PROMPT-011](agent-conversation-and-prompt-control.prd.md#req-prompt-011) | [AC-PROMPT-011](agent-conversation-and-prompt-control.prd.md#ac-prompt-011) |
|  | [AC-1](agent-conversation-and-prompt-control.prd.md#ac-1) |
|  | [AC-2](agent-conversation-and-prompt-control.prd.md#ac-2) |
|  | [AC-3](agent-conversation-and-prompt-control.prd.md#ac-3) |
|  | [AC-4](agent-conversation-and-prompt-control.prd.md#ac-4) |
|  | [AC-5](agent-conversation-and-prompt-control.prd.md#ac-5) |
|  | [AC-6](agent-conversation-and-prompt-control.prd.md#ac-6) |
|  | [AC-7](agent-conversation-and-prompt-control.prd.md#ac-7) |
|  | [AC-8](agent-conversation-and-prompt-control.prd.md#ac-8) |
