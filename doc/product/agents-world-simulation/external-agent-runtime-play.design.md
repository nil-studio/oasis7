# 外部 Agent Runtime 接入与持续游玩产品设计

## 文档身份

- 配对产品 PRD：[external-agent-runtime-play.prd.md](external-agent-runtime-play.prd.md)
- 上位产品 PRD：[prd.md](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- 专业域权威：[Decision Provider](../../world-simulator/llm/decision-provider-contract.prd.md)、[Continuous Agent Harness](../../world-simulator/llm/continuous-agent-harness.prd.md)、[双轨执行](../../world-simulator/llm/provider-agent-dual-mode.prd.md)、[Runtime lifecycle](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md)
- Last reviewed：2026-10-10

本设计组织配对 PRD 的用户经历、信息层级和恢复解释。首期**推荐**外部 Runtime 通过官方 Skill 学会并主动调用标准 Game API；Provider Bridge 只是可以弃用的既有实验适配，并非新的产品接口权威。所有行为义务和首期范围由配对 PRD 拥有；以下状态是体验表达，不新增协议枚举、CLI 参数、组件布局或实现状态机，也不表示当前已支持 OpenClaw/Codex。

## 1. 设计命题

目标体验是让玩家把已有 Runtime 委托给游戏 Agent，同时能通过网页和受支持桌面客户端**真实浏览同一个大世界、定位 Agent 和工厂、查看事件与权威结果**，再放心离开观战并在回来后理解进展、损失、阻塞和下一步。玩家主要决定目标、策略约束、委托范围与是否继续投入；Runtime 自主决定授权内的具体行动。

接入阶段优先回答“连接的是谁、替哪个 Agent 做什么、需要什么条件和开销”；开始后优先回答“目标有何进展、世界确认了什么、下一步为何这样做”。版本、适配器、连接诊断和详细调用记录按需展开，不能挤占游戏目标与后果。

设计假设是：熟悉的 Runtime 使用方式、连续的目标反馈和明确的恢复选项能降低接入与持续游玩的理解成本。反证信号包括：用户必须读原始日志才知道结果、重复输入逐步命令、把对话回复当成已应用目标，或因一次断连重新创建任务和重复操作。真实使用者观察验证这些假设；结构检查不证明吸引力或留存。

## 2. 代表性使用经历

### 2.1 首次接入、自主入场与委托

玩家从可信发布入口为 OpenClaw 或 Codex 获取官方 oasis7 Skill，核对版本兼容和网络 Game API 地址与权限范围，而不是先研究旧桥接工具。入口明确区分“目标 Runtime”“已验证组合”“不适用版本或缺失能力”，并给出连接目标、凭据归属、运行前置和成本范围。仅检测到程序存在或模型能回答时，仍保留尚未完成的连接/会话步骤。

连接就绪后先区分稳定参与主体、游戏角色和当前执行实例，再展示该主体可控制的 Agent 与既有义务。自身角色允许有权主体选择目标；他人角色仅使用玩家授权的目标和范围。如果没有角色，按现行规则进入正常注册/初始绑定、经济认领或观察路径，不把所有首次角色取得都显示为付费 claim。认领与维护条件消费[所有权主责](../world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#1-产品承诺)，连接软件本身不授予实体或资源。

启动前给出一个完整的委托摘要：目标世界与 Agent、高层目标、允许范围、预算的可执行部分、持续运行条件。日常行动由 Runtime 在该范围内自行选择；严重后果和额外授权消费[自治与委托主责](agent-authority-ownership-and-accountability.prd.md#2-授权与自治边界)，避免把正常游玩变成连续的逐动作批准。

<a id="autonomous-entry-experience"></a>
#### 无预置角色的机器入场

Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；承接 [REQ-EXT-018](external-agent-runtime-play.prd.md#req-ext-018) / [AC-EXT-018](external-agent-runtime-play.prd.md#ac-ext-018)。

Runtime 从可信 Skill/公开发现入口识别世界和能力，完成适用的注册或登录、世界准入，按现行规则取得自己的 Agent。体验分别显示账号已认证、世界已准入、角色已绑定、canonical owner/必要 starter 已满足、task/目标已提交、独立 executor 已就绪；前一步成功不隐含后一步通过。自动编排可以隐藏机械步骤，但不能省略任务或伪造任何权威结果。

需要人类触摸凭据、接受协议、批准费用或完成站点必要核验时，返回需要谁完成什么以及原操作的继续入口；不要求人类逐动作遥控，也不绕过确认。未知结果优先找回原身份和请求，不重新注册或另领初始资产。无准入资格或未发布的入口应显示明确阻塞，不引导用户暴露 loopback 私有服务。

同一 Runtime 的自身 Agent 与受托 Agent 以资源上下文标明“当前 owner / 受托执行、目标来源、有效范围”。切换上下文不切全局游戏 mode，不合并预算、私有目标或审批权。自有主体通过独立受保护 owner 控制路径设定自身目标；他人委托里仍只能建议或申请，不能自己批准。撤销一份委托后保留其他独立关系及已提交结果，不把一次撤销误报为整台 Runtime 或全部角色关闭。

### 2.2 生产、反馈与新的选择

代表性任务消费[当前首局链](../world-rules-core-gameplay/first-session-and-continuation.prd.md)：Runtime 从观察中识别目标、资源和合法能力，形成计划并推进生产。计划可以变化，也可能因资源、授权或观察过期而被拒绝；产品将变化解释为“发生了什么、为什么需要调整、有哪些有效下一步”。

世界结算后，将匹配的成果、资源变化和主因果带回任务上下文。下一轮 Runtime 以真实结果继续，玩家同时能看到对应成果。生产接受、排程、普通库存变化与首产物完成按首局主责区分；这里不增加新的完成口径。

第一次遇到生产阻塞时，玩家可以理解等待、补足条件、缩小目标或重排之间的取舍。界面只展示当时权威合同支持的选择；无法执行的选项说明限制，不用反复重试代替决策。任务取得首产物后继续展示下一项目标与未完成义务，不自动宣布稳定生产或交付完成。

### 2.3 目标调整、离开与回访

玩家可以保持委托、调整高层目标或停止后续委托。目标调整沿用[Prompt 主责](agent-conversation-and-prompt-control.prd.md#23-agent-prompt-与目标调整)：草稿、已接受和已应用分别呈现。只有应用后的新决策使用有效新目标；已有待决行动独立显示，不能借调整目标暗示其已经取消。

离开 Viewer 时，执行方仍在线且委托有效，任务可以继续。回访先恢复当前目标、最新可信成果、主要阻塞和安全下一步，再提供离开期间的详细记录。关闭 Runtime 时如实说明其执行停止；世界继续推进，已受理行动仍按原合同产生结果。

### 2.4 以 Skill + Game API 主动游玩的真实体验

玩家向已有 Runtime 表达“使用 oasis7 Skill，连接我获授权的世界 Agent，开始首项工业目标”。Skill 先说明自己的来源和适配范围，Runtime 使用已有的工具调用能力验证游戏 API 版本、认证/委托、当前目标和能力目录；任何需授权的步骤由有权玩家确认。Skill 不是能修改世界的实体，不复制模型密钥，也不依赖向 Prompt 贴长期 token。

当 Runtime 得到合法授权后，自己进行“观察 → 确定下一动作 → 向 Game API 提交 → 读取 pending/committed/rejected/failed 或增量事件 → 更新自身计划”的循环。用户不需要知道 `provider_loopback_http` 的启动参数，也不需要为了玩游戏跑一套模型 Provider 回调服务。世界并不替 Runtime 运行它的私有多 Agent 工作流；外部推理停止时，世界继续但不会凭空有新外部认知。

在首次访问中，默认只呈现“连接目标/有效身份、受控 Agent、当前目标、关键世界反馈、阻塞/恢复”。只有用户需要排错时才展开 Skill 版本、Game API 协议状态、网络/认证错误和诊断；不把 HTTPS 连上、Skill 文档已下载或 MCP 已识别某个工具当作已进入可玩的 Agent 会话。

### 2.5 大世界 Web/客户端可视化体验

大世界是理解外部 Runtime 自主行动的**玩家主要观察面**。玩家从世界总览进入区域/Fragment，平移缩放、搜索定位自己的 Agent、工厂或路线，选中查看有来源时效的对象状态、工业活动、世界事件、当前目标及权威回执，再从合法 Agent 的上下文打开高层 Prompt/目标/委托入口。地图点击本身不直接移动、采集或建造。

| 场景 | 交互结果 | 必须保留 |
| --- | --- | --- |
| 世界/区域/Fragment | 平移、缩放、下钻、返回总览或当前目标 | 观察范围、真实位置与抽象关系位置不同 |
| Agent/工厂/事件 | 搜索、定位、选中检查，适用时跟随/退出 | 任务、blocker、来源时效、ambient 活动与 committed receipt 分离 |
| 高层指导 | 从合法 Agent 详情修改 Prompt/目标/委托 | accepted/applied、pending/committed 与控制资格独立 |
| 断线或 Renderer 错误 | 可访问文本的最新可信结果、刷新或安全返回 | 不把空舞台或旧缓存冒充实时世界 |

正式桌面客户端可以内嵌 Web 舞台，但必须在客户端内部操作世界；仅跳转系统浏览器不算满足客户端体验。世界浏览、空间数据和 P1-A/B/C 前置由[世界舞台单一主责](player-readable-world-stage.prd.md#world-exploration-surfaces)及其配对设计定义。本设计只是外部 Runtime 游戏闭环的组合，不声明旧 2D overview 或 native viewer 已交付。

### 2.6 玩家与外部 Runtime 的双向持续协作

Web/桌面玩家不直接控制 OpenClaw/Codex 的聊天进程，而是将有权发布的目标、约束、问题和决定保存到同一游戏任务。当前受权执行方经 Game API 读取版本化目标与增量变化，分别确认读取、声明采纳并将有效版本用于新决策；计划与阻塞报告、双向问答、授权申请和世界结果返回同一任务上下文。跨设备回访不要求复制原聊天记录或重新创建 Agent。

目标已保存、执行方已读取、采纳声明、实际用于决策和世界已执行是不同事实。普通聊天不能自动改目标或批准权限；累计预算、撤销和过期按世界权威规则生效，不等待执行方确认。离线发布显示等待执行方，增量缺口/重复/迟到消息按当前目标与权限恢复，而非重演旧指令。

[玩家与外部 Runtime 持续协作 PRD](player-runtime-collaboration.prd.md#player-runtime-collaboration)拥有该通路的局部功能清单和验收，[配对协作设计](player-runtime-collaboration.design.md)承接状态、问答与授权交互。其要求细化本体验既有的目标应用、授权、反馈和恢复，不新建一套世界权威或开发任务台账。实际启动/接收/等待/唤醒能力须由各受支持执行组合提供；Skill 不是常驻进程，关闭执行方后不能承诺后台推理。

## 3. 状态与恢复

### 3.1 分开表达四类状态

| 状态面 | 玩家需要区分 | 表达原则 |
| --- | --- | --- |
| 观战/玩家会话 | 在线、断连、会话待恢复 | 仅说明观察或提交入口是否可用；不代签 Runtime 停止或 Agent 委托失效。 |
| Runtime 执行 | 未就绪、运行、等待、阻塞、离线、恢复中 | 说明是否能产生下一次决策及主要原因；等待须有可理解的条件或下一步。 |
| Agent 委托与目标 | 当前有效、目标待应用、到期/撤销、需重新确认 | 有效资格优先于缓存目标；恢复连接不自动恢复权限。 |
| 世界行动与成果 | 候选、待决、已结算、拒绝/失效、结果待核对 | 消费权威结果；执行连接状态、对话回复和本地日志不能改写此面。 |

多种状态并存时，主界面先显示影响下一步的资格/安全阻塞，再显示待核对世界结果、可执行恢复、当前目标进展；已结算成果始终保留。网络故障不能把成果归零，也不能将离线期间的推测填成最新事实。

### 3.2 主要故障的处理经历

| 触发 | 保留什么 | 用户下一步 | 恢复后的判据 |
| --- | --- | --- | --- |
| Runtime 版本或能力不符 | 已确认身份、目标草稿与尚未开始的任务 | 按推荐说明修复适配或选择适用目标 | 真正由声明 Runtime 承担会话，所需能力可用。 |
| 连接认证或委托失效 | 权威成果、原请求关联和失效原因 | 重新认证、取得合法委托，或保持观察 | 重新核对资格；不得以公开标识、旧缓存或费用路由替代授权。 |
| 世界拒绝或资源不足 | 已发生消耗、实际成果及当前主要阻塞 | 刷新观察、补足、等待或重排 | 新决策依据新事实；拒绝不会变成模型自报成功。 |
| 超时、响应丢失或状态未知 | 原请求身份与已知世界进度 | 核对原结果、等待或走受支持恢复 | 确认后继续，不隐式重做未知动作或静默启动另一个 Runtime。 |
| Runtime 进程重启 | 可恢复会话信息与可信世界历史 | 核对权限、目标和未决义务后续接；私有上下文缺失时重新规划 | 旧成果不重复，新决策来自仍有效的目标与权限。 |
| 预算耗尽或计量不完整 | 成果、待决结果、已知开销与未知部分 | 停止新受限请求，补充预算或结束委托 | 可执行限制与估计明确区分，不能承诺无法落实的下游硬上限。 |

恢复是核对已有工作再决定下一步。用户若显式选择切换 Runtime，消费[provider 切换主责](provider-agent-experience-continuity.prd.md#21-切换窗口与在途意图)；首期不要求把原 Runtime 的私有记忆、工具进程和在途推理无损迁移到另一个 Runtime。

## 4. 信息层级与入口一致性

### 4.1 默认任务视图

默认层保留目标世界与 Agent、当前有效目标、Runtime 是否在工作、最近权威成果、主要阻塞、预算摘要和下一步。只有与当前决策相关的连接或权限问题前景化。

展开层提供阶段计划摘要、行动与结果关联、等待原因、目标变更和恢复历史。诊断层提供版本、适配方式、模型/profile、调用开销、错误与证据引用；可以核对实际 Runtime 和会话来源，无需暴露凭据、其他 Agent 私有上下文或模型内部思维过程。

工具调用记录能说明 Runtime 尝试了什么；世界 receipt 说明实际发生了什么。两者通过同一任务关系可追溯，展示上保持区别。

### 4.2 Viewer 与 pure API

`viewer` 提供玩家可读的上述信息和间接控制；`pure_api` 提供语义等价、可由客户端消费的状态、目标调整与恢复结果。两者复用世界事实，并分别证明自身入口可用性；产品不要求 pure API 自带图形界面。

执行 lane 的可见范围不随玩家界面故障自动扩大。无 GUI 运行也可以保持受约束的观察；需要改变 lane 时按专业合同显式选择并记录，已有证据不能改标成另一入口或观察范围的成功。

窄屏、语言变化和回访的信息保留消费[玩家可读表面连续性](player-readable-surface-continuity.prd.md)；布局调整不能藏起当前 Agent、资格阻塞、待核对结果或主要恢复动作。

### 4.3 Skill、Game API 和可选适配层

| 接口层 | 对外产品含义 | 应保留的边界 | 不应承诺 |
| --- | --- | --- | --- |
| 官方 Skill | 说明游戏任务和如何用受授权 API，自助引导 OpenClaw/Codex | 可确认来源、适用版本、能力变动时重新查询；不保存秘密或静默执行高权操作 | Skill 文字本身不授权、执行或确认世界成果 |
| 标准 Game API | 真正的外部游戏能力入口，可由普通网络客户端直接访问 | 同一 Agent/世界权限下的观察、查询、合法动作、待决/回执和增量读取；本地与远程授权语义相同 | API 受理不等于世界结算；网络连接不等于委托 |
| MCP / CLI / SDK（按需） | 为某类 Runtime 提供熟悉的调用手感 | 从同一能力目录和认证上下文转译，并返回同一权威结果 | 不产生新世界动作或另造版本/权限合同 |
| 旧 Provider Bridge（可退役） | 游戏向外部 Provider 索取下一决策的旧适配方向 | 存量实际依赖可单独评估，世界权威历史不可丢失 | 不作为 Skill + Game API 的首期必需依赖 |

典型的客户端可调用语义不冻结 URL、字段或语言框架，但必须保留四段：

1. **发现与读取**：协商版本/范围；读取仅限当前合法身份和 Agent 的世界状态、主目标、新鲜度、可用能力及参数要求。
2. **建议与提交**：外部 Runtime 在自己一侧决定行动，使用明确的 Agent 委托和原请求身份提交；读请求不带副作用，提交不直接承诺结算。
3. **结果与继续**：按原意图关联查询权威回执或从可恢复游标读取增量变化；没有流式推送也能以有界轮询完成首局。
4. **错误与恢复**：认证、委托、能力过期、超时未知、资源不足、限流和预算不可混淆；绝不因为错误而静默重签、切其他费用路由或提交第二次世界作用。

远程入口须使用有保护的传输与可信身份绑定；本地入口也不能只凭公开路由字符串授予世界写权。高风险模型凭据与游戏认证材料分离，Skill 的来源核对不能替代 API 每次授权。客户端优先以网络结果而非输出文字判断完成；自助连接失败时可以安全停止并恢复玩家决策权。

## 5. 资源、取舍与验证

世界维护和工业消耗遵循玩法与 Runtime 主责；外部模型/工具成本单独说明。开始前让用户理解“已知会消耗什么、哪些仅是估计、哪层能阻止新增开销”；执行中让用户根据实际进展决定继续、缩小目标、调整预算或停止后续委托。停止不回滚世界，也不保证已发出的推理立即停止计费。

首期优先官方 Skill、独立可调用的 Game API、一条完整首局和两个真实 Runtime 的分别验收。更多工具、长期记忆迁移和多 Agent 协作可以扩展，但不能用扩大功能目录替代现有路径的反馈、目标生效和恢复完整性。

产品评审观察用户能否独立接入、辨认所用 Runtime、理解有效目标与权威结果、处理一次阻塞并回访继续。专业验证核对实际 Runtime 会话、能力执行、反馈、权限、预算和恢复。未经改变的 builtin 基线、额外脚本辅助和 Runtime 的局部成功应按专业 parity 如实记录；该设计不规定统一模型策略或新的分数阈值。

## PRD REQ/AC fragment mapping

| 产品要求 | 产品验收 | 本文设计承接 |
| --- | --- | --- |
| Trace refs: O7-REQ-b3e367c3-c72d-4959-b599-613ac652dab8；[REQ-EXT-001](external-agent-runtime-play.prd.md#req-ext-001) | [AC-EXT-001](external-agent-runtime-play.prd.md#ac-ext-001) | §2.1 推荐路径、真实 Runtime 与就绪表达。 |
| Trace refs: O7-REQ-d30925fe-d798-4056-9641-062255931a61；[REQ-EXT-002](external-agent-runtime-play.prd.md#req-ext-002) | [AC-EXT-002](external-agent-runtime-play.prd.md#ac-ext-002) | §2.1 委托摘要；§3 资格与恢复。 |
| Trace refs: O7-REQ-aad7b341-83b1-4402-8ebc-8540db36ccfb；[REQ-EXT-003](external-agent-runtime-play.prd.md#req-ext-003) | [AC-EXT-003](external-agent-runtime-play.prd.md#ac-ext-003) | §2.2 合法能力、前置变化与首局结果。 |
| Trace refs: O7-REQ-bd792456-4b98-4891-b5e3-e24b1de4f1a1；[REQ-EXT-004](external-agent-runtime-play.prd.md#req-ext-004) | [AC-EXT-004](external-agent-runtime-play.prd.md#ac-ext-004) | §2.1–2.2 自主推进；§4 计划与反馈层次。 |
| Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7；[REQ-EXT-005](external-agent-runtime-play.prd.md#req-ext-005) | [AC-EXT-005](external-agent-runtime-play.prd.md#ac-ext-005) | §2.2 权威后果；§3 未知结果与核对。 |
| Trace refs: O7-REQ-091d6c22-fca5-4284-91dd-2edce9ef440b；[REQ-EXT-006](external-agent-runtime-play.prd.md#req-ext-006) | [AC-EXT-006](external-agent-runtime-play.prd.md#ac-ext-006) | §2.3 目标应用与停止后续委托；§2.6 跨 Runtime 协作。 |
| Trace refs: O7-REQ-c5c90e09-ae74-4d77-9858-a4769562b01e；[REQ-EXT-007](external-agent-runtime-play.prd.md#req-ext-007) | [AC-EXT-007](external-agent-runtime-play.prd.md#ac-ext-007) | §2.3 回访；§3.1 分开的四类状态。 |
| Trace refs: O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330；[REQ-EXT-008](external-agent-runtime-play.prd.md#req-ext-008) | [AC-EXT-008](external-agent-runtime-play.prd.md#ac-ext-008) | §3.2 故障恢复与原世界历史。 |
| Trace refs: O7-REQ-cb69c2a7-f13a-42ea-a929-214fa7ff9d43；[REQ-EXT-009](external-agent-runtime-play.prd.md#req-ext-009) | [AC-EXT-009](external-agent-runtime-play.prd.md#ac-ext-009) | §3.2 预算限制；§5 两类成本与继续选择。 |
| Trace refs: O7-REQ-7283b901-ea06-49e3-956d-2bad46b6c89d；[REQ-EXT-010](external-agent-runtime-play.prd.md#req-ext-010) | [AC-EXT-010](external-agent-runtime-play.prd.md#ac-ext-010) | §2 完整首局经历；§5 两个 Runtime 分别验证。 |
| Trace refs: O7-REQ-a5bbf6e4-3e71-4d48-9eba-68c50867c95c；[REQ-EXT-011](external-agent-runtime-play.prd.md#req-ext-011) | [AC-EXT-011](external-agent-runtime-play.prd.md#ac-ext-011) | §2.1 与 §2.4 官方 Skill 可信分发、适配引导与失败修复。 |
| Trace refs: O7-REQ-8120dffe-17cc-458a-9c70-8121217e6f7b；[REQ-EXT-012](external-agent-runtime-play.prd.md#req-ext-012) | [AC-EXT-012](external-agent-runtime-play.prd.md#ac-ext-012) | §2.4 与 §4.3 标准 API 的主动调用、回执及增量结果。 |
| Trace refs: O7-REQ-7d6cef7f-9419-49ed-b445-663382383458；[REQ-EXT-013](external-agent-runtime-play.prd.md#req-ext-013) | [AC-EXT-013](external-agent-runtime-play.prd.md#ac-ext-013) | §3.2 与 §4.3 身份授权、错误和安全恢复。 |
| Trace refs: O7-REQ-68ffc843-8679-4ad9-8cef-10a239a29c73；[REQ-EXT-014](external-agent-runtime-play.prd.md#req-ext-014) | [AC-EXT-014](external-agent-runtime-play.prd.md#ac-ext-014) | §4.3 旧 Bridge 的非默认地位与替换条件。 |
| Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；[REQ-EXT-015](external-agent-runtime-play.prd.md#req-ext-015) | [AC-EXT-015](external-agent-runtime-play.prd.md#ac-ext-015) | §2.5 Web/native 大世界分别可用。 |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[REQ-EXT-016](external-agent-runtime-play.prd.md#req-ext-016) | [AC-EXT-016](external-agent-runtime-play.prd.md#ac-ext-016) | §2.5 世界概览、缩放、搜索与对象选择。 |
| Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；[REQ-EXT-017](external-agent-runtime-play.prd.md#req-ext-017) | [AC-EXT-017](external-agent-runtime-play.prd.md#ac-ext-017) | §2.5 世界现场、合法指导和回执。 |
| Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；[REQ-EXT-018](external-agent-runtime-play.prd.md#req-ext-018) | [AC-EXT-018](external-agent-runtime-play.prd.md#ac-ext-018) | [无预置角色的机器入场](#autonomous-entry-experience)，逐阶段状态、必要确认与安全恢复。 |
| Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；[REQ-EXT-018](external-agent-runtime-play.prd.md#req-ext-018) | [AC-EXT-002](external-agent-runtime-play.prd.md#ac-ext-002) | [自主入场后的资源关系共存](#autonomous-entry-experience)，自身 owner / 受托 executor 的上下文、目标、预算与撤销隔离；不替代冷入场验收。 |
