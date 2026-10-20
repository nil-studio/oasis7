# 玩家与外部 Runtime 的持续协作

## 文档身份

- 所属产品模块：智能体、世界模拟与交互
- 上位产品 PRD：[prd.md](prd.md)
- 配对产品 design：[player-runtime-collaboration.design.md](player-runtime-collaboration.design.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- 专业域权威：[Continuous Harness](../../world-simulator/llm/continuous-agent-harness.prd.md)、[Runtime lifecycle](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md)、[Viewer](../../world-simulator/viewer/README.md)
- Last reviewed：2026-10-10

本文细化[外部 Runtime 游玩](external-agent-runtime-play.prd.md)中的目标应用、玩家指导与持续执行：Web/桌面玩家如何把目标交给 OpenClaw/Codex，如何得到回应、处理授权并在离线后继续。本文只定义产品合同，不表示消息通道、适配器、托管执行或真实协作已经实现。

## 设计适用性与生命周期闭合

- 设计判定：`paired-design`。
- 配对关系：[同名设计](player-runtime-collaboration.design.md)承接目标下发、计划汇报、问答、授权和恢复的用户交互，不复制本 PRD 的要求。
- 设计适用性理由：协作跨越多个客户端、外部 Runtime、目标版本和世界结果，需统一身份、状态、失败解释与可访问反馈。

## 1. 产品目标

<a id="player-runtime-collaboration"></a>
玩家在网页选中自己的 Agent，设定“先保证能源供应，再完成生产目标”，随后关闭网页。已启动的外部 Runtime 接收这个目标、自主推进；遇到超出现有授权的投入时提出申请。玩家从桌面客户端查看同一任务、核实成本并作出决定，Runtime 收到有效决定后继续。用户不必复制聊天记录、找到原终端或重新创建游戏 Agent。

**双方通过 oasis7 持久化的游戏任务、目标、消息与委托协作，不由网页直接操纵某个 OpenClaw/Codex 聊天进程。** 同一游戏 Agent 的世界历史、玩家有效目标、执行方绑定、消息回应和最终成果应能关联；外部 Runtime 保留自主规划能力，世界 Runtime 保留权限校验和最终结算权。

“持久化协作通道”是产品能力，不预设新的独立服务、消息中间件或第二套世界数据库。可复用现有任务、目标、授权和事件能力。这里的任务是游戏内委托，不是仓库开发流程的 Issue/Project，也不新增开发门禁或执行台账。

设计假设：异步确认和可核对的协作记录能减少跨设备重复指导。反证信号是玩家仍需复制提示词、误认已保存等于已执行，或因找不到问题与授权关系放弃任务；应通过真实使用者和实际 Runtime 样例验证，不编造改善比例。

## 2. 范围与主责边界

### 2.1 三种能力分别成立

| 能力 | 产品承诺 | 边界 |
| --- | --- | --- |
| Agent 独立运行 | 受支持执行方通过 Skill/Game API 接收任务并继续决策，不依赖观战窗口保持打开 | 必须有实际运行且获授权的执行方；安装 Skill 不等于启动常驻执行或购买托管服务。 |
| 人类可视化体验 | 玩家在 Web 和受支持桌面客户端探索世界、查看同一任务并给予高层指导 | 两者是 `viewer` 的使用表面；地图选择不授予控制权，GUI 不是 Agent 执行前置。 |
| 协作访问与验收 | 玩家发布、Runtime 接收与回应、世界执行分别有可追溯结果 | Game API 是双方受权使用的通道，`pure_api` 不要求图形；各 Runtime 和使用表面按声明范围分别验证。 |

### 2.2 唯一事实来源与身份

- 协作必须关联世界、游戏 Agent、游戏任务、有效目标/委托和当前执行方。Runtime 的内部 session、子任务或进程重启不能创建新的世界实体，也不能成为玩家任务唯一身份。
- oasis7 的可信目标/委托记录保存玩家发布内容、版本和生效范围；Runtime 私有计划、普通聊天或本地文件不能覆盖它。审批权限、Agent 所有权与累积资源边界消费[自治与责任](agent-authority-ownership-and-accountability.prd.md#agent-delegation-boundary)。
- 默认由有权玩家修改持续目标；外部 Runtime 可以提出目标调整建议。通过 Runtime 终端要求改目标，也必须使用明确的目标写入权限和同一提交路径，不能因文字来自本机而取得 owner 权限或批准自己。
- 首期每个受控 Agent 只有一个被认可的当前决策执行方；内部并行工具不等于多个独立控制者。交接必须更新绑定并使旧执行方失去新的提交资格；可读历史仍按当前权限判断，不自动把私有上下文交给下一执行方。
- 计划、摘要、问题和聊天只是带来源的协作内容。完成判定、资源消耗和世界效果依赖[世界生命周期](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md#42-recovery-rules)及玩法自己的成果规则。

### 2.3 首期与 Non-Goals

首期包含下列 14 项协作能力，OpenClaw、Codex 分别证明真实会话可使用；复用既有首局工业目标与 Web/桌面入口，不以新建完整任务平台作为前置。

不要求跨 Runtime 私有记忆无损迁移、全模型行为一致、无限期后台托管、全聊天自动理解为授权、网页直接操作用户电脑进程、完整多 Agent 调度或强制流式推送。游戏内授权不同时授予外部 Runtime 的文件系统、支付账户或其他工具权限。具体服务组件、端点、字段、队列和传输选型由专业设计确定。

## 3. 产品功能清单

清单各功能的设计/实现状态及适用 Trace 由本行链接的 REQ 状态摘要拥有；同一要求下的功能共同消费该范围，不以某个子功能完成代签其余义务。后续候选仍不是首期承诺。

<a id="collaboration-feature-catalog"></a>
清单按[功能清单维护原则](../../engineering/doc-governance/product-documentation-standard.design.md#product-feature-catalog-principles)维护，阶段不代表实现状态。下列局部功能细化外部 Runtime 主清单已有的目标、授权、持续运行和反馈承诺，不将其重复计为 14 个新的独立顶层产品；主清单编号保持稳定，开发进度与实际支持结论仍查询 PR/QA 记录。

| 功能 ID | 功能项 | 可观察结果 | 阶段 | 要求与验收 |
| --- | --- | --- | --- | --- |
| COL-F-001 | 游戏任务与执行方绑定 | 用户知道哪个 Runtime 正在为哪个世界 Agent 的哪项任务工作 | 首期协作 | [REQ-COLLAB-001](#req-collab-001) / [AC-COLLAB-001](#ac-collab-001) |
| COL-F-002 | 跨表面同一协作记录 | Web 与桌面客户端读取同一有效目标、问题、授权和结果 | 首期协作 | [REQ-COLLAB-001](#req-collab-001) / [AC-COLLAB-001](#ac-collab-001) |
| COL-F-003 | 持续目标发布与接收确认 | 保存、等待读取、已读取、采纳声明和实际用于新决策可区分 | 首期协作 | [REQ-COLLAB-002](#req-collab-002) / [AC-COLLAB-002](#ac-collab-002) |
| COL-F-004 | 目标版本与并发编辑处理 | 旧草稿、旧规划或迟到确认不覆盖当前目标 | 首期协作 | [REQ-COLLAB-002](#req-collab-002) / [AC-COLLAB-002](#ac-collab-002) |
| COL-F-005 | 计划与阶段进度汇报 | 玩家知道 Runtime 打算做什么、正在等待什么及需要谁处理 | 首期协作 | [REQ-COLLAB-003](#req-collab-003) / [AC-COLLAB-003](#ac-collab-003) |
| COL-F-006 | 汇报与权威成果核对 | 计划、估计和自报完成不会冒充世界成果 | 首期协作 | [REQ-COLLAB-003](#req-collab-003) / [AC-COLLAB-003](#ac-collab-003) |
| COL-F-007 | 任务内双向问答 | 玩家能询问原因，Runtime 能向有权玩家提出澄清问题 | 首期协作 | [REQ-COLLAB-004](#req-collab-004) / [AC-COLLAB-004](#ac-collab-004) |
| COL-F-008 | 回复关联与过时问题处理 | 回答绑定原问题，旧目标的问题不会自动改变新任务 | 首期协作 | [REQ-COLLAB-004](#req-collab-004) / [AC-COLLAB-004](#ac-collab-004) |
| COL-F-009 | 超范围决策申请 | Runtime 提出有对象、成本、风险和有效期的授权请求 | 首期协作 | [REQ-COLLAB-005](#req-collab-005) / [AC-COLLAB-005](#ac-collab-005) |
| COL-F-010 | 批准、拒绝、过期与撤销反馈 | 只有当前有效决定可以授权后续行动，拒绝或不回复不默认同意 | 首期协作 | [REQ-COLLAB-005](#req-collab-005) / [AC-COLLAB-005](#ac-collab-005) |
| COL-F-011 | 离线消息与任务恢复 | 离开后回来不丢当前目标、未答问题、未决申请与世界结果 | 首期协作 | [REQ-COLLAB-006](#req-collab-006) / [AC-COLLAB-006](#ac-collab-006) |
| COL-F-012 | 重复、迟到与事件缺口恢复 | 消息可补送但不重复改变目标、批准或世界效果 | 首期协作 | [REQ-COLLAB-006](#req-collab-006) / [AC-COLLAB-006](#ac-collab-006) |
| COL-F-013 | 执行可用性与唤醒条件 | 能区分服务可连、等待执行方、正在决策和等待世界结果 | 首期协作 | [REQ-COLLAB-007](#req-collab-007) / [AC-COLLAB-007](#ac-collab-007) |
| COL-F-014 | 停止与执行方重新接入 | 停止新的委托工作但保留待决结果；重启或交接后先核对再继续 | 首期协作 | [REQ-COLLAB-007](#req-collab-007) / [AC-COLLAB-007](#ac-collab-007) |

## 4. 用户流程、状态与失败

### 4.1 正常协作

玩家从地图选中自己的 Agent，查看有效任务及执行方，发布目标与约束。oasis7 校验身份、控制资格和编辑基准后保存新的目标版本；即使 Runtime 不在线，用户也可以得到真实的保存结果，但只能看到“等待执行方”。

已运行的执行方从 Game API 获取当前目标及增量通知，确认读取的版本，并在后续决策中使用该版本。它可汇报计划、提出问题或请求额外授权；世界动作仍经 Game API 的权威校验与结算。玩家从任一受支持表面查看进展、回答问题、作出授权决定或调整目标。离开观战界面不删除任务或停止仍有效的独立委托。

### 4.2 状态不得混为一谈

| 状态 | 可依赖的事实 | 不能推导的结论 |
| --- | --- | --- |
| 草稿 | 当前客户端正在编辑 | 尚未发布或获得任何授权 |
| 已保存/已接受 | oasis7 保存了有权发布的目标及版本 | Runtime 已经读取、执行或理解正确 |
| 待读取 / 已读取 | 当前执行方尚未确认，或确认收到指定版本 | 收到不等于采纳或产生新决策 |
| Runtime 声明已采纳 | 执行方声明更新了任务方向 | 不证明模型一定遵循，也不证明世界已改变 |
| 已用于新决策 | 可关联的新决策、等待或阻塞结果使用了指定目标版本与有效委托 | 只证明使用上下文；自然语言策略的质量仍需行为证据 |
| 世界结果 | 已知请求的权威受理、拒绝、待决或已结算状态 | pending、自报完成或动画都不是结算 |
| 被新版本替代 / 过期 / 被阻塞 | 原内容不能作为当前有效指令，历史仍可读且受权限约束 | 不能复活旧委托或追溯撤销已结算成果 |

持续目标在 oasis7 生效与外部 Runtime 采纳是两个状态面；既有 Prompt 的 accepted/applied 不能被静默重定义。可机器校验的授权和资源上限从其权威生效点约束后续请求，不等待 Runtime 的“已读”确认。

### 4.3 目标、消息与授权的规则

- Web/客户端和获准的其他写入方使用同一目标版本基准。基准过期时保留草稿并要求重读，不静默最后写入覆盖；失去控制资格时不泄露新 owner 的私有目标。消费[Prompt 并发规则](agent-conversation-and-prompt-control.prd.md#25-并发变更与过期草稿)。
- 目标变化期间已开始的推理不自动取消，但旧版本生成的新行动不能不经当前目标/委托校验就继续。已受理但未结算的世界请求按现行 Runtime 合同处理，已结算成果不回滚。
- 问答包含任务、来源、原问题、目标版本和未答/已答/已过时等可读关系。普通聊天、引述的世界文本和回复“好”不能自动改目标或批准权限；需转为明确的目标修改或授权确认操作。
- 授权申请说明所需选择、对象范围、累计预算、风险、有效期及拒绝后的路径。目标、owner 或委托变化使原申请不再适用时须失效或重新确认；执行时仍校验当前权限、剩余额度与世界前置，批准不是世界执行成功。
- 无回应不默认批准。等待时只允许继续原有效范围内独立且不规避阻塞的行动；有副作用的越权部分保持阻塞。游戏审批不替代用户机器、模型账户或第三方工具的权限确认。

### 4.4 持续执行与恢复

首期最低要求是可恢复的增量读取或有界轮询，流式推送按需增加；不要求用户电脑开放公网接收端口。Skill 说明每轮如何检查目标、问题和授权，但不承担进程保活。每个受支持 Runtime 组合必须给出用户启动并授权的实际执行/唤醒路径、失联判定边界和恢复办法；可以是运行中的 Runtime 会话或薄适配执行器，不要求另一套通用调度平台。

单次 CLI 会话结束、执行器停止或机器关闭后，不承诺仍产生新决策，也不由网页静默启动本机程序。界面应区分“最近通信正常”“执行状态未知”“等待执行方”“正在决策”和“等待世界/玩家”，并标明状态时效；一次健康检查成功不能证明 Agent 正在工作。

消息补送不承诺传输层永不重复。接收侧应按稳定关联去重，重复回复、确认、批准不能产生第二个语义效果；同一消息身份对应不同内容应拒绝并提示冲突。游标失效或事件历史有缺口时，重新获取当前目标、委托、未答问题、待决申请与权威结果，并明确缺口，不能假称“没有新事件”。

Runtime 重启或交接先校验当前执行方资格，取最新目标与未决结果再继续；过时目标保留历史但不依次重演。停止后续委托应阻止新的受限提交，不删除消息或回执，已有 pending 先核对；恢复资格需重新检查，不能以旧已读、旧批准或 session 恢复替代授权。

## 5. 产品要求与验收

<a id="req-collab-001"></a>
### REQ-COLLAB-001：协作必须关联同一游戏任务和有效执行方

Trace ID: O7-REQ-62007da5-b12a-4b8c-98bb-82ab53a35069

- 功能点：[REQ-COLLAB-001](#req-collab-001)；适用范围沿本条原条件与 [AC-COLLAB-001](#ac-collab-001)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-binding)覆盖唯一任务/执行方、旧fence隔离和私有受众；同文件需求承接/验证表反链本条AC。剩余：Web/桌面共享可恢复任务、当前执行方显示与快照/恢复的双表面消费尚需精确组合承接；不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[实现任务 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定实现源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L295)：原生TaskControl/executor fence与私有context主体绑定；[历史prepared-world真实模型记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)仅证明其固定候选的targeted切片，包含原cleanup失败。剩余Web/native协作显示、完整TaskSnapshot与跨客户端隐私，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：Web/桌面共享可恢复任务、当前执行方显示与快照/恢复的双表面消费尚需精确组合承接；实现/验证：Web/native协作显示、完整TaskSnapshot与跨客户端隐私。

- 要求：Web/桌面玩家与外部 Runtime 必须共享可恢复的世界、Agent、任务及执行绑定，显示当前合法执行方；未经授权的其他客户端、Agent 或旧执行方不能读取私有协作内容、修改目标或提交新行动。
- 理由：跨设备继续不依赖原聊天窗口，也不能因此扩大权限。
- 上位承诺：[根 SC-14](prd.md#external-runtime-play)；消费[外部 Runtime 绑定要求](external-agent-runtime-play.prd.md#req-ext-002)。
- 验收：[AC-COLLAB-001](#ac-collab-001)。

<a id="ac-collab-001"></a>
### AC-COLLAB-001：跨设备同一任务与执行方隔离

Trace refs: O7-REQ-62007da5-b12a-4b8c-98bb-82ab53a35069；承接[REQ-COLLAB-001](#req-collab-001)。

- 覆盖要求：[REQ-COLLAB-001](#req-collab-001)。
- 给定：一个合法 Agent/任务、Web 与桌面登录，以及另一身份、另一个 Agent 和已被替换的执行方作为负例。
- 当：玩家切换表面，执行方重启或交接后读取与提交。
- 则：合法用户仍看到同一任务与正确当前执行方；无权内容不泄露，旧执行方不能继续新提交，内部 session 变化不会新建游戏 Agent 或丢弃历史。

<a id="req-collab-002"></a>
### REQ-COLLAB-002：目标发布到用于决策必须有版本化确认

Trace ID: O7-REQ-edd52061-73c0-433c-9531-30d86b1db459

- 功能点：[REQ-COLLAB-002](#req-collab-002)；适用范围沿本条原条件与 [AC-COLLAB-002](#ac-collab-002)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-goal)覆盖目标版本、采纳自报与真实context/decision证据分离；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[实现任务 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定实现源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L505)：完整激活目标进入context、固定goal revision的真实模型决策；[历史prepared-world真实模型记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)仅证明其固定候选的targeted切片，包含原cleanup失败。剩余received/adoption报告和多窗口/迟到确认的完整UI，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：本条范围及主要失败边界已有专业承接；实现/验证：received/adoption报告和多窗口/迟到确认的完整UI。

- 要求：系统必须区分目标保存、执行方读取、采纳声明和实际用于新决策，并将后续提交关联当前有效目标及委托；过期编辑、旧规划和迟到确认不能覆盖新目标或绕过已生效约束。
- 理由：玩家知道指导传到了哪里，而非把保存成功当作 Agent 执行成功。
- 上位承诺：[根 SC-14](prd.md#external-runtime-play)；消费[目标应用](external-agent-runtime-play.prd.md#req-ext-006)。
- 验收：[AC-COLLAB-002](#ac-collab-002)。

<a id="ac-collab-002"></a>
### AC-COLLAB-002：离线发布、并发更新与旧版本拒绝

Trace refs: O7-REQ-edd52061-73c0-433c-9531-30d86b1db459；承接[REQ-COLLAB-002](#req-collab-002)。

- 覆盖要求：[REQ-COLLAB-002](#req-collab-002)。
- 给定：执行方离线时发布的新目标，以及另一表面的旧草稿和按旧目标启动的推理。
- 当：执行方重新接入，玩家再修改目标，旧草稿/确认/行动迟到。
- 则：保存后显示等待执行方；确认和新决策可核对各自版本，旧写入不覆盖新目标，旧行动需重新校验；授权撤销不等待已读，已有世界结果不被改写。

<a id="req-collab-003"></a>
### REQ-COLLAB-003：计划汇报与权威成果必须分别可读

Trace ID: O7-REQ-b100c55e-5c35-46dc-aaf6-e66c02b323d8

- 功能点：[REQ-COLLAB-003](#req-collab-003)；适用范围沿本条原条件与 [AC-COLLAB-003](#ac-collab-003)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-content)覆盖报告来源/更新时间/证据、真实完成判定；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：未核实
- 实现与验证依据：[已交付Skill范围](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/skills/oasis7-play/SKILL.md#L1-L25)明确prepared-world签名客户端而非两Runtime完整首局；[私有路由分派](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/world_service_private_goal.rs#L380-L431)只接goal/context/quote，[共享存储实际schema](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_client_api/src/world_service/client_state.rs#L8-L27)只保存prepare/public intent及反馈journal。[执行诊断投影](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_model_client/execution.rs#L166-L170)明确goal_completion_verified=false，但它是第一方run诊断，未绑定TaskReport的reported_at/received_at、next_actor与证据；[实际HTTP读面](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_viewer_runtime/src/game_read_api.rs#L124-L156)仍只读observation/capabilities/task-control且writes_available=false。新存储抽取未增加任务报告生产或双方恢复读取。
- 核对日期与基线：2026-10-17；状态卡按 A `0901927131b3e7c919fc6b092e77ae8895579989`，静态生产源码固定 main `e27f4d990bc4d481097c003ff75afc582fd8f65d`，22个引用/闭包源逐 Git blob 校验；另比较 `5d503f0ca79bcc68708a32672a48a2786ad51395` 迁移前源码。未运行模型、Runtime、浏览器或行为验收，未把测试源码当本轮PASS。
- 剩余范围：两Runtime各自TaskReport写入/持久身份、目标revision/来源/时间/证据核验、Web与桌面同任务读取及过时报告处理尚未闭包；native已核范围不提供完整报告，不能推导所有adapter未实现，也不以第一方诊断的诚实完成标记代签本条报告义务。

- 要求：Runtime 必须可汇报任务相关计划摘要、阶段状态、阻塞及需要谁处理，并关联适用目标和证据；玩家能区分 Runtime 报告、估计、最后更新时间与世界已确认结果，不得仅凭自报完成标记游戏目标完成。
- 理由：让玩家理解进展而不泄露模型内部思维或伪造成果。
- 上位承诺：[根 SC-14](prd.md#external-runtime-play)；消费[权威反馈](external-agent-runtime-play.prd.md#req-ext-005)。
- 验收：[AC-COLLAB-003](#ac-collab-003)。

<a id="ac-collab-003"></a>
### AC-COLLAB-003：自报成功、陈旧报告和世界结果冲突

Trace refs: O7-REQ-b100c55e-5c35-46dc-aaf6-e66c02b323d8；承接[REQ-COLLAB-003](#req-collab-003)。

- 覆盖要求：[REQ-COLLAB-003](#req-collab-003)。
- 给定：计划/阻塞报告、自报完成但缺少成果的任务，以及实际 committed 与 pending 回执。
- 当：玩家在两种表面阅读报告，并收到一份较旧报告。
- 则：来源、时效和需要谁处理可读，旧报告不覆盖当前状态；自报不能替代完成证据，报告不暴露凭据、其他任务私有内容或模型内部思维过程。

<a id="req-collab-004"></a>
### REQ-COLLAB-004：问答必须可恢复且不隐式升级为指令

Trace ID: O7-REQ-2f463269-c3c0-4da1-95f8-90011c35799a

- 功能点：[REQ-COLLAB-004](#req-collab-004)；适用范围沿本条原条件与 [AC-COLLAB-004](#ac-collab-004)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业设计](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-content)§5.3承接问答身份与旧回答不授权，§6.1和§7.2承接同切点snapshot/after-S增量与先持久化inbox再确认；逐条后续处理确认的完整语义仍需核对，维持部分覆盖。
- 实现状态：未核实
- 实现与验证依据：[私有路由分派](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/world_service_private_goal.rs#L380-L431)与[共享存储实际schema](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_client_api/src/world_service/client_state.rs#L8-L27)的真实身份是context operation，既有GoalExecutionSnapshot为控制/完整目标/授权观察，未承载question_id/message_id/in_reply_to集合；[实际HTTP读面](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_viewer_runtime/src/game_read_api.rs#L124-L156)仅消费当前task-control。迁移后的private_goal_store/client_state直接委托oasis7_client_storage，保留原prepare签名及操作恢复，而非协作inbox。[两Runtime声明profile](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/doc/world-simulator/llm/external-agent-runtime-adapter.design.md#L71-L79)各自仍需绑定声明profile到实际问答producer/consumer；普通chat、PromptControl ack与Skill自然语言汇报不充当该链。
- 核对日期与基线：2026-10-17；状态卡按 A `0901927131b3e7c919fc6b092e77ae8895579989`，静态生产源码固定 main `e27f4d990bc4d481097c003ff75afc582fd8f65d`，22个引用/闭包源逐 Git blob 校验；另比较 `5d503f0ca79bcc68708a32672a48a2786ad51395` 迁移前源码。未运行模型、Runtime、浏览器或行为验收，未把测试源码当本轮PASS。
- 剩余范围：已查清迁移不增加问答存储/确认；仍需两个Runtime各自问题/回答生产、持久inbox及接收/后续处理ack、goal revision旧消息隔离和Web/桌面读取闭包。未穷尽其他adapter，不能把当前native切片的缺项判作整条未实现。

- 要求：玩家与 Runtime 必须能围绕同一任务提出问题、回复并核对接收/后续处理；回答绑定原问题及适用目标，已过时问题或普通聊天不得自动改写目标、批准权限或产生世界动作。
- 理由：异步协作不依赖玩家与模型同时在线，也不把自然语言回复当作授权凭据。
- 上位承诺：[根 SC-14](prd.md#external-runtime-play)；消费[对话与目标区分](agent-conversation-and-prompt-control.prd.md#23-agent-prompt-与目标调整)。
- 验收：[AC-COLLAB-004](#ac-collab-004)。

<a id="ac-collab-004"></a>
### AC-COLLAB-004：跨设备回复、重复回复与过时问题

Trace refs: O7-REQ-2f463269-c3c0-4da1-95f8-90011c35799a；承接[REQ-COLLAB-004](#req-collab-004)。

- 覆盖要求：[REQ-COLLAB-004](#req-collab-004)。
- 给定：Runtime 提问后离线、玩家从另一表面回复，以及目标已变更的问题和无权回复者。
- 当：执行方恢复读取，并重复取得同一回复。
- 则：回复只进入原任务且不重复产生效果，过时问题标明需重评估；无权回复被拒绝，聊天“同意”不变成授权或持续目标修改。

<a id="req-collab-005"></a>
### REQ-COLLAB-005：关键选择必须通过可追踪的有效授权协作

Trace ID: O7-REQ-f5341eef-d8f6-42ae-b7b6-751803abd588

- 功能点：[REQ-COLLAB-005](#req-collab-005)；适用范围沿本条原条件与 [AC-COLLAB-005](#ac-collab-005)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-approval)覆盖精确申请、明确owner决定、累计授权和执行时重验；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：未核实
- 实现与验证依据：[私有路由分派](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/world_service_private_goal.rs#L380-L431)的完整分派与[共享存储实际schema](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_client_api/src/world_service/client_state.rs#L8-L27)的实际记录类型未增加ApprovalRequest/Decision；现有task控制和grant消费继续是授权边界，不能代替申请→owner明确决定的生产端。[已交付Skill范围](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/skills/oasis7-play/SKILL.md#L1-L25)要求缺grant时询问owner或停止，并不提供正式approval-request/decision接口；[两Runtime声明profile](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/doc/world-simulator/llm/external-agent-runtime-adapter.design.md#L71-L79)区分Codex本机sandbox审批、OpenClaw会话权限与游戏授权。共享operation cache重导出不产生新grant或世界写权。
- 核对日期与基线：2026-10-17；状态卡按 A `0901927131b3e7c919fc6b092e77ae8895579989`，静态生产源码固定 main `e27f4d990bc4d481097c003ff75afc582fd8f65d`，22个引用/闭包源逐 Git blob 校验；另比较 `5d503f0ca79bcc68708a32672a48a2786ad51395` 迁移前源码。未运行模型、Runtime、浏览器或行为验收，未把测试源码当本轮PASS。
- 剩余范围：两个Runtime分别需申请身份/proposal digest/goal revision→当前owner批准拒绝或调整→grant→累计额度/有效期/撤销及执行重验→双方当前状态读取的闭包；底层授权与本机工具审批均不代签游戏审批协作。当前native缺项已定位，全部external adapter尚未穷尽。

- 要求：Runtime 必须能提出符合既有授权规则的申请，玩家能够批准、拒绝或调整范围；只有当前有权主体对适用目标、对象、累计额度、风险和有效期的明确决定可用于后续行动，拒绝、超时、撤销和条件漂移不能默认为继续。
- 理由：避免逐动作遥控，同时保留重大选择与成本控制。
- 上位承诺：[根 SC-14](prd.md#external-runtime-play)；消费[累计授权与责任](agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001)。
- 验收：[AC-COLLAB-005](#ac-collab-005)。

<a id="ac-collab-005"></a>
### AC-COLLAB-005：审批生效、拒绝与失效负例

Trace refs: O7-REQ-f5341eef-d8f6-42ae-b7b6-751803abd588；承接[REQ-COLLAB-005](#req-collab-005)。

- 覆盖要求：[REQ-COLLAB-005](#req-collab-005)。
- 给定：一次超范围投入申请，另有重复批准、拒绝、超时、owner/目标变化及额度已消耗的负例。
- 当：玩家决定后 Runtime 尝试执行，包括批准后、执行前发生撤销的情形。
- 则：符合当前范围的请求才可进入世界裁决；其余阻塞或重新确认，重复批准不复制累计额度；普通聊天和游戏批准不赋予本机文件/第三方工具权限，批准不代签结算。

<a id="req-collab-006"></a>
### REQ-COLLAB-006：协作补送与恢复不得重演旧意图

Trace ID: O7-REQ-6044cb04-5fbb-4c68-81a6-fc57cfdd608a

- 功能点：[REQ-COLLAB-006](#req-collab-006)；适用范围沿本条原条件与 [AC-COLLAB-006](#ac-collab-006)，不扩大入口或阶段。
- 系统设计状态：已覆盖
- 系统设计依据：[专业承接条款](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-recovery)覆盖原操作恢复、快照分页与增量切点、旧授权不复活；同文件需求承接/验证表反链本条AC。此为目标设计覆盖，不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[实现任务 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定实现源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L322)：原prepare及原签名行动安全恢复；[历史prepared-world真实模型记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)仅证明其固定候选的targeted切片，包含原cleanup失败。剩余未答问题/待决审批的快照分页、补送与重复批准，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：本条范围及主要失败边界已有专业承接；实现/验证：未答问题/待决审批的快照分页、补送与重复批准。

- 要求：双方断线后必须能恢复当前目标、委托、未答问题、待决申请及世界结果，并识别重复、迟到、过时和历史缺口；消息补送不得导致重复批准、旧目标复活或第二次世界效果。
- 理由：持续协作需要可信的当前状态，而不是机械重放所有历史消息。
- 上位承诺：[根 SC-14](prd.md#external-runtime-play)；消费[断连恢复](external-agent-runtime-play.prd.md#req-ext-008)。
- 验收：[AC-COLLAB-006](#ac-collab-006)。

<a id="ac-collab-006"></a>
### AC-COLLAB-006：断线、游标缺口与未知提交对账

Trace refs: O7-REQ-6044cb04-5fbb-4c68-81a6-fc57cfdd608a；承接[REQ-COLLAB-006](#req-collab-006)。

- 覆盖要求：[REQ-COLLAB-006](#req-collab-006)。
- 给定：双方先后断线、消息补送、失效游标、同标识不同内容以及结果未知的提交。
- 当：恢复并继续任务。
- 则：去重、冲突和缺口明确，必要时重读可信当前快照；最新有效目标而非旧历史驱动后续决策，未知世界请求先核对，旧已读/批准不恢复权限或重复世界效果。

<a id="req-collab-007"></a>
### REQ-COLLAB-007：持续协作必须有真实执行方与明确停止边界

Trace ID: O7-REQ-8dad5caa-d064-4ac8-8c98-40a702b13685

- 功能点：[REQ-COLLAB-007](#req-collab-007)；适用范围沿本条原条件与 [AC-COLLAB-007](#ac-collab-007)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-runtime-journey)已承接同任务真实Runtime组合验收场景；同文件需求承接/验证表反链本条AC。剩余：实际driver启动/等待唤醒/恢复与协作fence的运行设计组合仍须核对；collab-runtime-journey为同任务验收设计，不代替完整运行合同。 不代表实现或发布。
- 实现状态：部分实现
- 实现与验证依据：[交付 PR #4628](https://github.com/eng-cc/oasis7/pull/4628)；[固定静态源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_game_client/client.rs#L295)核实原生当前executor fence读回/校验及消费。历史prepared-world固定候选 `b59d010556e39beaf1928eb1e95aa9aad099e180` 的两次真实目标决策、正式Skill使用及原结果回放只由[历史运行记录](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)支持；其中原cleanup失败保持不变，不能外推当前main或完整首局。剩余每个Runtime持续运行、等待/唤醒、离线更新和组合恢复，不推导端到端完成。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：实际driver启动/等待唤醒/恢复与协作fence的运行设计组合仍须核对；collab-runtime-journey为同任务验收设计，不代替完整运行合同。；实现/验证：每个Runtime持续运行、等待/唤醒、离线更新和组合恢复。

- 要求：每个受支持 Runtime 组合必须提供实际启动、接收更新、等待/唤醒与恢复路径，准确区分观战连接、执行方可用性、任务状态和世界状态；执行方停止后不得承诺新决策，停止或替换执行方必须保留世界历史并重新核验资格。
- 理由：Skill 是说明而非常驻进程，关闭 GUI 与关闭执行方是不同操作。
- 上位承诺：[根 SC-14](prd.md#external-runtime-play)；消费[独立运行](external-agent-runtime-play.prd.md#req-ext-007)。
- 验收：[AC-COLLAB-007](#ac-collab-007)。

<a id="ac-collab-007"></a>
### AC-COLLAB-007：两个 Runtime 的真实异步协作经历

Trace refs: O7-REQ-8dad5caa-d064-4ac8-8c98-40a702b13685；承接[REQ-COLLAB-007](#req-collab-007)。

- 覆盖要求：[REQ-COLLAB-007](#req-collab-007)。
- 给定：分别声明的 OpenClaw 与 Codex 执行组合及有权任务。
- 当：玩家发布目标后关闭观战，执行方继续；再停止执行方、离线更新目标、重启并恢复，期间完成一次问答及一次显式授权决定。
- 则：两个 Runtime 各自能确认新目标、回应并继续合法行动；停止时显示等待执行方而非假运行，恢复使用当前目标和授权；实际成果仍由世界证据确认。Web/桌面按声明范围覆盖，证据不足的组合不自动扩大支持。

### 5.1 叶级追踪

| REQ / AC 关系 | 专业 owner | 专业权威 | 验证证据（应提供） | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-COLLAB-001](#req-collab-001) / [AC-COLLAB-001](#ac-collab-001) | agent_engineer / runtime_engineer / viewer_engineer / qa_engineer | [绑定与授权](external-agent-runtime-play.prd.md#req-ext-002) | 跨表面同任务、错误身份隔离、执行方交接和旧提交拒绝 | test_tier_required |
| [REQ-COLLAB-002](#req-collab-002) / [AC-COLLAB-002](#ac-collab-002) | agent_engineer / runtime_engineer / viewer_engineer / qa_engineer | [目标与 continuation](../../world-simulator/llm/continuous-agent-harness.prd.md#9-goal-与-continuation-边界) | 保存/读取/采纳/使用分离、并发编辑、迟到确认与旧版本行动 | test_tier_required |
| [REQ-COLLAB-003](#req-collab-003) / [AC-COLLAB-003](#ac-collab-003) | agent_engineer / viewer_engineer / gameplay_designer / qa_engineer | [权威反馈](external-agent-runtime-play.prd.md#req-ext-005) | 报告与世界成果对账、时效、来源和私有信息负例 | test_tier_required |
| [REQ-COLLAB-004](#req-collab-004) / [AC-COLLAB-004](#ac-collab-004) | agent_engineer / viewer_engineer / qa_engineer | [Prompt 与对话](agent-conversation-and-prompt-control.prd.md#23-agent-prompt-与目标调整) | 双向问题、跨设备回答、重复/过时/无权回复不转为指令 | test_tier_required |
| [REQ-COLLAB-005](#req-collab-005) / [AC-COLLAB-005](#ac-collab-005) | agent_engineer / runtime_engineer / viewer_engineer / qa_engineer | [累计授权](agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) | 明确批准/拒绝、超时撤销、目标/owner 变化与剩余额度 | test_tier_required |
| [REQ-COLLAB-006](#req-collab-006) / [AC-COLLAB-006](#ac-collab-006) | agent_engineer / runtime_engineer / qa_engineer | [Runtime 恢复](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md#42-recovery-rules) | 断线补送、幂等、游标缺口、未知世界提交与权限重验 | test_tier_required |
| [REQ-COLLAB-007](#req-collab-007) / [AC-COLLAB-007](#ac-collab-007) | producer_system_designer / agent_engineer / viewer_engineer / qa_engineer | [实际执行与观战](external-agent-runtime-play.prd.md#req-ext-007) | 两个实际 Runtime 各自运行、问答/授权、离开、停止、更新和恢复 | test_tier_full |

### 5.2 验收范围与证据

普通 CI 优先覆盖可确定复现的绑定、版本、重复/迟到消息、授权和恢复负例；缺测试先补普通 CI，不因“协作”另设专门放行体系。真实 OpenClaw/Codex 证明实际接收与多轮行为，Web/桌面操作证明用户理解和处理流程，世界回执证明效果，分别声明未覆盖范围。

每份样例记录适用的 Runtime/版本、Skill/适配方式、游戏候选、任务/Agent、表面、授权、目标版本和验证窗口。已有世界首局、交互或授权证据可按其范围复用，不要求机械重跑全部 Runtime×表面笛卡尔积；声明某组合支持时须有该组合的适用证据，不能拼接无关联截图。文档结构检查不证明实现完成、真实体验有效或公开可用。

## 6. 权威、取舍与后续细化

[外部 Runtime PRD](external-agent-runtime-play.prd.md)拥有完整游玩目标、主功能清单和首局组合；本分册拥有其跨 Runtime 协作细节与局部清单；[Prompt 产品](agent-conversation-and-prompt-control.prd.md)拥有对话/配置、草稿及并发编辑，[自治产品](agent-authority-ownership-and-accountability.prd.md)拥有授权/责任，[世界舞台](player-readable-world-stage.prd.md)拥有空间与视觉，专业 Runtime 拥有世界提交和结果。冲突需由相关 owner 显式协调，不能用本分册覆盖既有授权强度。

首期选择持久化任务与增量读取，而非网页直连本机聊天进程：牺牲即时远程操控便利，换取设备无关、可恢复和不暴露本机端口的路径；不要求独立建设任务平台。若现有组件可履行合同则复用，不为兼容旧 Provider 回调牺牲产品目标。

未决实现选择包括承载服务、执行方绑定技术、消息保留/游标策略、轮询/唤醒方式和故障检测阈值。由 agent/runtime/viewer owner 与 QA 在具体适配实施前给出可验证边界；尚不能可靠常驻或恢复的组合只声明会话内能力，不承诺后台运行。本文不新增协议字段、精确时限或当前支持状态。
