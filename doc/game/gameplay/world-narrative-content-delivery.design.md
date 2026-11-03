# 世界前提与事实叙事内容交付设计

- 主责：`gameplay_designer`；所属产品模块为[世界规则与玩法系统](../../product/world-rules-core-gameplay/prd.md)，专业入口为 [game](../README.md)。`game_visual_interaction_designer` 审读玩家表达，Viewer 与 Site 维护各自实际消费者。
- 生命周期：`active`。适用于已采纳世界前提、官网世界介绍，以及现有 Viewer 环境事件、行动结果和 Agent 消息的内容制作；不建立新剧情、任务树或叙事执行系统。
- 审读基线：`74867d54d92a771d968e91be8ba991abc81e5775` 固定既有原条款、小说入口、官网和直接消费者源码。`REQ-WR-NARRATIVE-001/002` 是本批为既有义务补充的稳定身份，不在该固定提交中；本文已对照本批接收原文。本文交付可编辑文本与审校样例；建议文字尚未据此替换运行文案，实际消费和验证另由对应候选记录证明。
- 上游：[世界前提与玩家位置](../../product/world-rules-core-gameplay/prd.md#req-wr-narrative-001)、[事实叙事边界](../../product/world-rules-core-gameplay/prd.md#req-wr-narrative-002)及 §2.1 的入口要求。制作与验证方法见[配套手册](world-narrative-content-delivery.manual.md)。

## 1. 问题、目标与范围

玩家需要知道自己身处怎样的世界、通过谁影响世界，以及一段文字究竟是在描述规则、表达某个 Agent 的意见，还是报告已经确认的结果。现有官网、World Feed、Action Receipt 和消息流已有这些材料，但文字来源和可表达范围分散，内容作者容易把相邻事实写成因果，或借小说填补尚未发布的游戏事实。

本设计交付六项世界前提的短文、现有内容类型与双语状态、正常/未知/纠正三组完整样例，以及编辑源、转换和检查方法。故事的吸引力来自可辨认的对象、承诺、选择与后果；文字帮助玩家看懂它们，不为世界增加未发生的情节。

当前不采纳小说中的年代、老人共同体、通信技术、人物关系或事件为游戏设定；不编写固定 NPC、分支对白、剧情任务、奖励或新私有知识。首局工业成果和续接文案复用[首局内容 C01–C09](../../world-simulator/viewer/first-session-content-delivery.design.md#content-units)，本设计不重做其配方、产量、里程碑或页面布局。长期治理、组织、工业网络和协议历史仍按原专业范围建设，不能由本目录推定当前 Feed 已支持全部类别。

## 2. 上游要求与约束

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-NARRATIVE-001](../../product/world-rules-core-gameplay/prd.md#req-wr-narrative-001)；Trace refs: O7-REQ-5bde78bc-7ce9-48f3-820b-8415dc02ee00 | 世界介绍准确表达已采纳舞台与玩家/Agent 位置 | [世界前提文本](#narrative-premise-content) | 产品原世界前提；Site/Viewer 各自采用入口 | 不采纳小说专有事实，不表示具身或当前发行可用 |
| [REQ-WR-NARRATIVE-002](../../product/world-rules-core-gameplay/prd.md#req-wr-narrative-002)；Trace refs: O7-REQ-5f3a2585-c3f2-4bce-9da6-08ac6638cf4f | 事实叙事保留对象、承诺、选择和真实后果，不凭摘要增加世界事实 | [叙事事实边界](#narrative-fact-boundaries) | game 原历史义务；Runtime 的合法记录与因果 | 不新增历史类别、叙事生成器或完整历史系统 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | 当前目标、相关对象、阻塞、下一步与原结果可读 | [Viewer 内容](#narrative-viewer-content) | Viewer 原舞台、目标和回执消费者 | 不重排界面，不由环境动态补个人进度 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | 当前、最近已知、未知和冲突的信息保留来源与时效 | [知识与时序](#narrative-knowledge-boundaries) | 原观察范围、授权和权威重验 | 不创建侦察、情报购买、公开 baseline 或新的读取资格 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | 个人结果、公共环境和当前观察焦点分别表达 | [人称与归属](#narrative-perspective) | 当前身份、原操作与可见性 | 不披露他人私有反馈，不以人称授予控制权 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)；Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | 意图接受、等待、推进与权威后果不混写 | [目标与结果文案](#narrative-intent-content) | Runtime 意图与结算；原结果查询 | 不推定旧请求未生效，不新增执行或取消动作 |
| [REQ-PROMPT-002](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002)；Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293 | 一次对话、预设填充、本地草稿与持续配置可辨认 | [Agent 消息内容](#narrative-dialogue-content) | 原 Command、消息流与 Prompt 合同 | 不写固定角色对白，不把回复当世界权威 |
| [REQ-SITE-DISCOVERY-001](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-001)；Trace refs: O7-REQ-4544302c-40be-4aa3-a557-21fe6f475c19 | 官网首访能理解游戏类型、玩家分工与资源压力 | [官网介绍](#narrative-site-content) | Site 的实际双语首页和预览说明 | 不开放新加入入口，不改安装或发行承诺 |
| [REQ-SITE-DISCOVERY-002](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-002)；Trace refs: O7-REQ-4b01d2d4-ec3a-4dbb-90f8-fca0c4b0a822 | 展示中的目标、解释、实际结果与证据性质分开 | [官网事实案例](#narrative-site-evidence) | 原案例来源、详情/摘要/RSS | 不把历史 CLI 案例当当前 GUI 或公共世界 |
| [REQ-SITE-DISCOVERY-005](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-005)；Trace refs: O7-REQ-ee159d08-7998-44e2-bac5-930b7fa8a0ae | 小说保持创作身份、实际语言与当前玩法边界 | [小说导读](#narrative-fiction-entry) | V50 正文与原 reader 的状态/恢复 | 不更改小说正文，不补造英文正文或新剧情 |

<a id="narrative-consumer-boundaries"></a>
### 2.2 直接消费者与技术边界

| 消费者与实际编辑源 | 已核实的内容范围 | 本设计如何消费 |
| --- | --- | --- |
| [中文首页](../../../site/index.html)、[英文首页](../../../site/en/index.html)的 `#hero`、`#proof`、`#world-laws`、`#lore` | 题材与玩家分工、四步事实案例、四条世界规则、中文小说导读 | 保留原页面分工与 claim 条件；新短文只是可选编辑目标 |
| [Runtime World Feed](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/world_feed.rs)的 `is_public_event/project_event`，及[公共来源选择](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/world_feed_source.rs) | 显式允许的快照、Agent 注册/移动、模块视觉变化和可验证危机投影；普通 summary 为通用事件文本，detail 只含公开 DTO；当前 `receipt_ref` 为空 | 不读取被过滤的原始事件正文来“丰富故事”；字段不足时使用类型说明 |
| [WorldFeedPanel](../../../crates/oasis7_viewer/software_safe_src/world_feed_panel.jsx)、[Feed 状态归一](../../../crates/oasis7_viewer/software_safe_src/world_feed_state.js)、[危机表现](../../../crates/oasis7_viewer/software_safe_src/pixel_world_presentation.js) | 六种 Feed 状态、事件顺序、恢复、危机生命周期/严重度；明确回执才可链接 | 使用原语义、原恢复入口；历史/回放不变成新发生或个人成功 |
| [Viewer 主组件](../../../crates/oasis7_viewer/software_safe_src/viewer_app.jsx)、[目标组件](../../../crates/oasis7_viewer/software_safe_src/canonical_agent_goal.jsx) | 原 Action Receipt、目标文本、玩家消息/Agent 回复/失败；消息按 Agent 过滤 | 不由选择、附近对象、文本相似或时间相近补因果 |
| [canonical chat](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/canonical_agent_chat.rs)和[原 chat 路由](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/agent_chat.rs) | 目标提交、鉴权、原请求/结果；transport receipt 与 Runtime 接受分开 | 对话内容和配置状态依原消费者，不让编辑样例成为执行输入 |
| [小说入口](../../../site/story/README.md#配套资料)、[V50 背景](../../../site/story/background/world-background.md#7-使用边界) | 小说及其背景/正文边界 | 提供明确标识的外部阅读入口；不用小说补齐运行世界事实 |

Feed 的 schema、可见性、cursor、去重和恢复由[原 Feed 合同](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.prd.md#world-feed-v1-implementation-contract)及其实现拥有；实际权威重验沿[观察到生效](../../world-simulator/viewer/external-agent-world-view.design.md#view-admission)，反馈归属和结果查询沿[玩家反馈隔离](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation)与[原结果查询](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#gameplay-original-result-lookup)。本设计只提供表达，不增加 DTO、私有读取或查询权限。

## 3. 边界、结构与设计条款

<a id="narrative-premise-content"></a>
### 3.1 已采纳世界前提的可编辑文本

下列 P01–P06 是本专题编辑单元，不是新要求、运行字符串键或世界实体。中文为完整设计目标；其中官网已经表达的原意见右列。进入游戏后的新导语位置尚未指定，不以本文暗示已有欢迎页、百科或教程。

| 单元 | 中文设计目标文本 | 采用来源与已有表达 |
| --- | --- | --- |
| P01 舞台 | oasis7 是一款 AI Agent 文明模拟游戏，舞台位于破碎的小行星带。 | [原世界前提](../../product/world-rules-core-gameplay/prd.md#req-wr-narrative-001)；首页已使用“小行星带”与文明模拟题材 |
| P02 玩家位置 | 你从文明之外设定方向与边界，再根据世界结果决定是否继续。 | 同一产品前提；不把玩家写成降落在场的角色 |
| P03 Agent | Agent 在规则、授权与资源约束中选择具体行动。你能指导它，但不能把一句愿望当成已经发生的行动。 | 原间接控制；官网已有玩家/Agent 分工 |
| P04 时间 | 已经发生的变化留在同一世界的历史中。刷新或重新打开页面，不会抹掉已经确认的变化。 | [产品承诺](../../product/world-rules-core-gameplay/prd.md#1-产品承诺)及首页持久时间线；恢复和治理纠错继续按原专业合同 |
| P05 资源 | 每一次建设和生产都有条件与代价。先看当前可用的资源和授权，再决定要承担什么。 | 原资源约束；具体成本、可用性和取得方式由当前专业 profile 发布 |
| P06 后果 | 把目标、尝试和结果分别读清楚。已确认的后果可以追溯；还没有确认的部分，继续保留未知。 | [事实叙事边界](../../product/world-rules-core-gameplay/prd.md#req-wr-narrative-002)及原因果/观察要求 |

**组合短文（中文设计目标，可独立交给内容消费者）：**

> oasis7 是一款 AI Agent 文明模拟游戏。在破碎的小行星带，你从文明之外为 Agent 设定方向与边界。它们在资源和规则约束中行动，世界保留已经发生的后果。先看清当前目标、主要阻塞和可做的下一步，再决定继续、调整或暂缓。

这段文字解释产品世界前提，不声称每个入口已支持所有动作或当前世界正在发生某项活动。入口的模式、版本和可用动作继续在各自原位置说明。P01–P06 与组合短文尚无本专题核定的新英文译文；已有官网英文仍按下节使用，不能记为整段翻译已交付。

<a id="narrative-fact-boundaries"></a>
### 3.2 事实叙事的表达边界

当文字声称描述当前或历史世界事实时，按“对象在什么条件下作出什么选择，留下了什么可确认后果”的方向核对。每一项有依据才表达，不要求所有事件强行拼成四段或具备同一组字段。小说、概念说明和未执行示意可以创作，保留其身份并与世界事实分开。`Agent 接近工厂` 不证明合作或所有权，`工厂出现产出` 不证明是当前玩家的功劳，`收到回复` 不证明行动已经完成。

| 已有输入 | 可写的内容 | 不能由该输入补出的内容 |
| --- | --- | --- |
| 已采纳产品前提 | 世界类型、玩家位置与持续世界的解释 | 某个运行世界的年代、人口、历史或能力已就绪 |
| 当前合法可见的对象投影 | 发布的名字、类型、状态及其范围 | 未发布身世、动机、关系、职业或私有目标 |
| 被接受的意图或承诺 | 这个对象接受了什么、还在等待什么 | 结果已结算、费用尚未发生或一定能完成 |
| 与原操作匹配的权威结果 | 原主体、本次确证后果和可追溯记录 | 另一主体的功劳、未发布损失/奖励、后续阶段完成 |
| 公共环境事件 | 已发布事件类型、生命周期、时效和允许披露的背景 | 当前玩家导致事件、事件发生地点、私人损失或角色心理 |
| Agent 自由文本 | “该 Agent 的回复/说明”；保留实际说话者与原文 | 世界已经确认它说的全部事实，或它代表另一主体同意 |
| 小说或概念素材 | 明确标为创作的题材体验 | 当前玩法、实机、世界历史或可操作能力 |

普通 Feed 摘要目前只由已发布类型生成，不能借 raw event、detail 字符串或客户端推断填入地点、数量和因果。若要交付更具体的事件段落，先由原 producer 提供相应的合法事实；内容作者只在已存在的范围里选词。历史记录类别仍沿[原专业 §6.3](gameplay-top-level-design.prd.md#63-历史记录系统)，本设计不定义自动编年史、重大事件选取算法或各类别的新增消费者。

<a id="narrative-knowledge-boundaries"></a>
### 3.3 玩家知识、时序与未知

下列句子是中文设计目标，面向现有信息说明位置；不是新状态字段或自动弹窗。取值沿原观察合同，Feed 的传输状态与单条事件的 freshness 分开：页面显示 ready，不意味着其中每条历史事件都刚刚发生。

| 情况 | 可编辑文本 | 采用条件 |
| --- | --- | --- |
| 当前范围内已确认 | “这是当前可读取范围内已确认的信息。” | 来源、scope 与当前确认条件成立；不能用刚刷新代替确认 |
| 最近已知 | “这是最近一次可读取的记录；当前状态可能已经变化。” | 旧内容仍获准披露；撤权后不保留私有正文 |
| 尚未取得信息 | “当前还不能确认这部分信息。” | 不改成零、没有事件、已失败或安全 |
| 来源冲突 | “现有记录互相冲突，需要重新确认。” | 不按最后到达、客户端时钟或叙事需要选择真值 |
| 原操作结果未知 | “这次请求的结果还没有确认。先核对原请求的结果，再决定下一步。” | 使用原结果查询的既有合法入口；查询失败不说明原行动失败 |
| 重看历史 | “这是历史记录，描述的是当时的变化。” | 不新增结果提示或重放世界动作；已确认历史不因当前断连被改写 |

`tick` 不自动换算为现实分钟或小说日期。没有空间 anchor 的事件只作为环境记录，不借当前 Agent 的位置安排“发生在你身边”的描写。Frontier 的商业情报、公共 baseline 和侦察仍按[长期产品边界](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#53-未决问题与证据边界)另行采用。

<a id="narrative-perspective"></a>
### 3.4 人称、归属与校正

“你/你的”用于当前玩家有权读取且归属已确认的内容；“这次决定导致”还需要匹配的原操作因果。公共事件优先用“世界记录/该事件”，Agent 回复标说话者。观察 agent-b 时收到 agent-a 原操作结果，继续表述 agent-a；没有原关联则保持不能归因。

文案校正只修表达。错误稿“你解决了危机”在没有个人因果时改为“危机已解决”；不会为原稿补造回执，也不改写世界记录。若来源冲突使事实本身不能确认，则撤下确定结果句并使用 §3.3 的冲突表达。已经取得的真实旧结果、当前读取失败与新的权威纠正分别处理，不用覆盖文字抹去历史。公开文章的撤回/纠正沿 Site 原合同；本设计不新增运行时纠正面板。

<a id="narrative-viewer-content"></a>
### 3.5 Viewer 环境内容与已有双语状态

以下均为基线 [WorldFeedPanel](../../../crates/oasis7_viewer/software_safe_src/world_feed_panel.jsx) 的现有静态文案；保留六态区别，不声明完整本地化已完成。事件的自由 summary 与外壳语言不同，是当前消费者的真实边界。

| 原状态 | 已有中文 | 已有 English | 内容判定 |
| --- | --- | --- | --- |
| loading | 正在加载世界动态… | Loading world activity… | 读取中，不能代写 empty |
| ready | 环境上下文已更新 | Ambient context updated | 表示环境上下文可读；不是个人完成 |
| empty | 暂无世界动态 | No world activity yet | 本次合法读取没有可显示动态；不宣称世界毫无活动 |
| replay | 回放上下文 | Replay context | 历史环境；不是新发生或再次成功 |
| gap | 世界动态已过期 | World activity is stale | 原合同要求停止追加、清理并重读快照 |
| unavailable | 世界动态不可用 | World activity unavailable | 来源/能力/权限按原原因处理；不伪装没有事件 |

现有边界句为“环境动态仅作上下文呈现；因果仍以 Action Receipt 为准。” / “Ambient activity is context only; Action Receipt remains the causal source.”；现有恢复动作分别是“重新加载权威快照” / “Reload authoritative snapshot”与“重试 World Feed” / “Retry World Feed”。文案接入不得交换这两个恢复含义。

**可采用的补充中文目标：** 空态正文“当前读取范围内暂无世界动态。你仍可查看当前目标和已经确认的行动结果。”；断档正文“世界动态记录不连续，已停止追加。重新加载权威快照后继续核对。”；不可用正文“当前无法读取世界动态。这不说明世界中没有发生变化。”。具体按钮仅使用原状态下真实存在的动作，不新造“继续剧情”或“重做事件”。

当前 Major World Events 只消费通过 authority 校验的 crisis。其已有状态句由 [pixelWorldMajorEventPresentation](../../../crates/oasis7_viewer/software_safe_src/pixel_world_presentation.js)生成：

| 原生命周期 | 已有中文模板 | 已有 English 模板 |
| --- | --- | --- |
| active | 危机进行中 · 严重度 {severity} | Crisis active · severity {severity} |
| resolved | 危机已解决 · 严重度 {severity} | Crisis resolved · severity {severity} |
| timed_out | 危机已超时 · 严重度 {severity} | Crisis timed out · severity {severity} |

`severity` 使用原合同的已发布值；本设计不新增分级名称。没有 authority、生命周期冲突或撤权时不显示危机叙事；不靠“未知严重度”兜底恢复被拒的事件。历史 transition 和 cursor replay 不播报新危机或新成功。没有空间位置时不增加地点、marker 或人物遭遇；opaque subtype 不能由作者猜成“小行星风暴”等题材情节。

<a id="narrative-intent-content"></a>
### 3.6 目标、行动结果与叙事的连接

[当前目标组件](../../../crates/oasis7_viewer/software_safe_src/canonical_agent_goal.jsx)已有“当前目标” / “Current goal”和“目标已提交，等待世界确认。” / “Goal submitted; waiting for world confirmation.”。目标正文取已鉴权的当前值；本地草稿仍标草稿，pending 不能编成 Agent 已执行。

个人结果继续由原 Action Receipt 呈现。可编辑的连接句为：“先看这次行动已经确认的结果，再决定下一步。”；没有可确认结果时为：“尚未取得这次行动的可确认结果。”。句子不代替实际结果细节或原查询动作；只剩旧 accepted、没有 receipt 且无法确认当前状态时，采用未知句，不写“仍未执行”。

首局铁锭成果、投入/产出、归属与 Assembler 后继候选全部引用[原成果内容](../../world-simulator/viewer/first-session-content-delivery.design.md#outcome-attribution)。未来使用其他结果类型时同样需要其真实专业来源，不把工业的成功句泛化为任何任务都完成。

<a id="narrative-dialogue-content"></a>
### 3.7 Agent 消息的内容角色

消息流的标题与正文分别编辑。现有 [chatEntryTitle/Meta/Message](../../../crates/oasis7_viewer/software_safe_src/viewer_app.jsx)区分 `玩家 -> {target}`、`{agent} 回应`、`{target} 回复失败`；英文对应 `Player -> {target}`、`{agent} Reply`、`{target} reply failed`。没有位置时已有“未知位置” / “unknown location”，空正文为“这条消息没有可读正文。” / “This message has no readable text.”，错误正文以“Agent 回复没有完成” / “Agent reply did not complete”起句。

可用于现有消息区说明的中文目标为：“这里保留你发送的消息与该 Agent 的实际回复。回复中的判断仍要与当前可读取的世界事实核对。”。这是一段外壳说明，不能替实际 Agent 写回复，也不建立它的固定性格、过去或立场。

一次消息、填入草稿、持续目标/Prompt 配置与世界动作保持不同内容角色。显示语言切换仅改变外壳，不自动翻译或改写玩家意图和 Agent 自由文本；参数、字体和缺译方法复用[原本地化设计](../../world-simulator/viewer/viewer-localization-and-context-help.design.md#parameter-expression)。声音和表情没有本次采用输入，不新增配音、情绪标签或口型交付。

<a id="narrative-site-content"></a>
### 3.8 官网世界介绍

官网继续使用现有完整中英文介绍：“你设定目标与边界，Agent 在资源约束中行动。” / “In the asteroid belt, set goals and boundaries. Agents act under resource constraints.”。已有玩家分工三句为：

> 你决定要达成什么、允许做什么。\
> Agent 选择具体行动，并处理执行中的问题。\
> 你根据结果，决定是否继续原来的方向。

现有英文为：

> You set the goal and what is allowed.\
> The Agent chooses concrete actions and handles execution problems.\
> You read the result and decide whether to keep that direction.

维护这些句子时与 P01–P06 核对同一含义；世界介绍不附带“立即进入世界”的新能力承诺。原预览说明和平台/版本边界继续在 Site 的决策点保持可达。

<a id="narrative-site-evidence"></a>
### 3.9 官网事实案例的叙述方式

现有 `#proof` 已采用“设定目标 → 遇到阻塞 → 调整行动 → 核对结果”四步，以固定来源的受控 CLI 首批铁锭记录说明后果链。保留各步角色：目标来自输入，拒绝是已确认拒绝，调整来自实际执行记录，结果来自世界读回。案例的测试日期、源码、预置条件和未覆盖范围随原文一起维护，不把叙事压缩变成删除关键条件。

该案例继续链接[原详情](../../../site/devlog/first-iron-batch/index.html)，不在本设计复刻其数量与时间记录；对应译文位于[英文详情](../../../site/en/devlog/first-iron-batch/index.html)。历史 GUI 图、概念图、当前安装包和 CLI 案例属于各自来源。正文、首页/列表摘要与 RSS 的传播沿[Site 内容发布与纠正](../../site/github-pages/public-site-discovery-validation.manual.md#内容发布与纠正)。

<a id="narrative-fiction-entry"></a>
### 3.10 小说导读文本

沿[小说与世界记录分离](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-005)保留现有首页内容：

> 世界观小说 · 中文\
> 绿洲 2076\
> 退休以后，一群人把生活延伸到小行星带。第一件事不是征服宇宙，而是把灯亮起来。小说中的具身描述不代表当前游戏能力。\
> 开始阅读

英文首页已有标签 `WORLD-BUILDING FICTION · CHINESE`，原说明为 “After retirement, a group of people builds a life in the asteroid belt. Their first task is to keep the lights on. Fictional embodiment is not a current game feature.”，入口为 “Start reading (Chinese) →”。这不是英文小说正文；没有对应译文时仍进入实际中文 reader。

小说中的 2076、五十位老人、量子通信、角色记忆与具体事件只在小说自己的上下文成立。导读不向游戏发放身份、物品或任务，也不提前透露背景页中的情节。V50 正文、梗概和背景的版本关系沿 [story 原入口](../../../site/story/README.md)维护。

<a id="narrative-composition-samples"></a>
### 3.11 三组完整组合样例

以下为**虚构的离线审校输入和中文设计目标**，没有在运行世界发生，不是截图内容或真实 QA 记录。`agent-a`、`agent-b` 只是不同观察对象的占位 ID，不是新增人物。样例块给出完整可读文本，排版标题只示意原消费者的内容分工，不指定新面板或按钮。

#### 样例 A：正常读取环境，不把观察写成成果

输入：合法来源发布一个 `snapshot_created` 环境记录，Feed 为 ready；当前已鉴权玩家观察 `agent-a`，没有可显示的个人行动回执。此例没有提交目标或触发生产。

```text
oasis7 是一款 AI Agent 文明模拟游戏。
在破碎的小行星带，你从文明之外为 Agent 设定方向与边界。
它们在资源和规则约束中行动，世界保留已经发生的后果。

当前观察：agent-a

世界动态
环境上下文已更新
世界快照已记录。
环境动态仅作上下文呈现；因果仍以 Action Receipt 为准。

行动结果
当前没有可显示的个人行动回执。
```

“世界快照已记录。”是对已知类型的建议摘要，当前 Runtime 仍生成通用英文 event summary；没有接入新中文摘要。样例没有提交行动，所以不出现“这次行动仍在等待”等暗示，也不以快照数量生成玩家进度。

#### 样例 B：旧目标可读，原请求结果未知且 Feed 断档

输入：当前身份仍可读取 `agent-a` 此前已确认的目标文本“检查当前可用行动”；该目标下一个关联行动请求只留下旧 accepted，当前结果无法确认；Feed 为 gap。目标可读不意味着这个行动仍在执行。实际样例只提供原有快照重载入口，不增加新的提交按钮。

```text
当前目标：检查当前可用行动
观察对象：agent-a

行动结果
这次请求的结果还没有确认。
先核对原请求的结果，再决定下一步。

世界动态
世界动态记录不连续，已停止追加。
重新加载权威快照后继续核对。
最近一次可读取的记录，不代表当前状态。

重新加载权威快照
```

若原结果查询也不可用，说明查询的真实限制并保留未知，不将其写为“请求失败”“尚未扣除资源”或“请再次执行”。若旧目标本身已经撤权，则不使用本样例里的目标正文，改为原权限边界下的可披露说明。

#### 样例 C：纠正一段误归因的环境叙事

输入：合法公共来源通过 cursor replay 提供 crisis 的历史 resolved transition，Feed 为 replay，严重度为 3，无个人 receipt 关联、无空间 anchor；当前已鉴权玩家查看 `agent-b`。待审错误稿把它写成“你和 agent-b 保护了聚落，危机解除”；这句仅是故意错误的离线反例，没有发布。

**校正后的完整文本：**

```text
当前观察：agent-b

世界动态
回放上下文
危机已解决 · 严重度 3
这是历史环境记录，描述的是当时的危机状态。
这条环境记录没有提供与你的行动结果的关联。

行动结果
尚未取得可归属于你的行动回执。
```

校正删除无来源的“保护聚落”、合作关系和玩家功劳；保留已知生命周期与严重度。没有原始地点就不补“聚落”或地图标记。若后来取得真实个人因果，先沿原 receipt 核对主体与结果，再更新该结果的表达，不修改这条环境记录为另一事实。因果信息缺失与不存在因果是不同结论。

## 4. 制作规格与交付物

<a id="narrative-delivery-index"></a>
### 4.1 编辑源、交付版本与适用规格

| 内容组 | 可编辑源与交付物 | 规格、依赖和实际可用边界 |
| --- | --- | --- |
| 世界前提、补充文案、三组组合样例 | 本文件 §3 的 Markdown 文本；固定版本由消费 PR 的 Git revision 定位 | 中文设计目标可编辑；新段落未核定英文。这里只交付文本，不交付新页面、图片或运行世界 |
| 官网现有双语与小说导读 | `site/index.html`、`site/en/index.html`；原案例详情/摘要/RSS | 页面现有文本可定位；修改需保持同一来源和实际语言，当前线上消费版本未由本文验证 |
| Feed 与 crisis 文案 | 原 `world_feed_panel.jsx`、`pixel_world_presentation.js`；Runtime `world_feed.rs` 负责可发布摘要 | 静态外壳有中英分支，自由 summary 保留原语言；没有统一叙事字符串仓或完整自动缺译检测 |
| 目标、消息与个人结果 | 原 `canonical_agent_goal.jsx`、`viewer_app.jsx` 及各自数据来源 | 对象、时效和结果由原投影提供；本文不成为草稿/profile/消息的持久化源 |
| 小说正文 | [V50 原阅读版本](../../../site/story/releases/v50/README.md)及现有 reader | 本次只链接，不编辑正文/背景或生成译文；小说质量不由游戏内容审校代签 |

文本以 UTF-8 Markdown/HTML/JSX 原格式维护；不新增硬编码年代、世界人数、事件地点、材料数值或角色名。动态插槽只表示原合法值，不新增 DTO 或翻译键；显示名作为文本处理，身份仍使用原关联。长中文、换行、焦点、字号与无障碍沿[原语言设计](../../world-simulator/viewer/viewer-localization-and-context-help.design.md#parameter-expression)和实际 surface 规范，不用新的字数上限截断必要条件。

本次文字来源为仓库现有条款、字符串及本专题新编设计文本；仓库 [LICENSE](../../../LICENSE)为 MIT，保留原声明。本文未引入外部散文、人物素材、字体或图像，也未审计仓库全部资产的权利；未来引用外部材料时在原交付入口记录实际作者、来源、授权与复用范围，不能由本段代签。

<a id="narrative-production-handoff"></a>
### 4.2 制作、转换、多人交接与变更传播

1. 内容作者在原 PR 指明所改 P 单元、状态或样例，提供完整上下文、采用条件及新旧文本；由原产品/专业条款决定含义，不能只交一串脱离状态的句子。
2. 消费者维护原编辑源。Site 保持实际 CN/EN 页面和关联详情/摘要/RSS；Viewer 修改原 JSX/JS，Runtime 摘要变更由其 owner 检查公开 DTO 和来源边界。不能直接改生成 bundle 代替源文件。
3. Viewer 原 [package.json](../../../crates/oasis7_viewer/package.json)的 `build:viewer` 经 [Vite 配置](../../../crates/oasis7_viewer/vite.software-safe.config.mjs)与[finalize 脚本](../../../crates/oasis7_viewer/scripts/finalize-software-safe-build.mjs)输出 bundle/dist，并包含其原编译依赖；它不是纯文本导出。只有实际消费变更且具备条件时由实施方执行，文档审校不触发构建、游戏或模型。
4. 同一文字的源、译文和必要导出物由本次消费方协调提交；不同来源组可以分别编辑。冲突时回到各自原源合并，再按原管线生成，不靠选择旧 bundle 解决；改名、替换或删除同步检查原引用与入口。
5. 接收方先核事实/范围和文字，再核实际加载与语义；文件收到、编译成功与玩家理解分别取证。发现来源越权交 Runtime/Viewer，叙述误归因交内容与玩法 owner，小说版本问题交原 story 维护范围；不通过改写世界记录修饰文本缺陷。

具体传播例：将 gap 说明从短标签细化为断档正文时，先保留 `world_feed/v1` 的原状态语义；只改现有 panel 的文案及对应中英说明，核 snapshot reload 按钮和 gap 测试，不改变 cursor 或将 unavailable 变为 gap。若仅文档交付这条建议，记录尚未集成；若 producer 改变了可发布事实，则重新核对所有消费该事实的摘要和引用，不能沿用旧文字验收。

<a id="narrative-candidate-composition"></a>
### 4.3 候选组合与制作规模

组合验证按原构建/发布记录固定实际程序、文本源、bundle/页面、语言、world/profile/读取范围和环境。官网案例另保留其原测试来源，不能与当前页面构建或安装包合并成一个运行事实；Viewer 对照实际加载文字与同一候选的合法投影。只固定本文件 SHA 不证明某个客户端已经使用这些建议。

本次内容规模为六项前提、一组 Feed/危机状态、目标/消息/官网/小说的有限文本及三组审校样例。生产复杂度分别为静态说明、带状态/插槽的内容、跨来源的案例组合；现有双语句子可以复用，新段落仍需独立翻译与审校。未测制作工时、返工率、可持续批量规模或成本，不设每周产量。需要扩大到更多事件或语言时，以实际代表批次记录初建、复用、整合、验证、返工和等待，再由原 owner 决定范围；未公开字段、未采用事件类型和未取得译文不纳入可交付规模。

## 11. 验证与验收

静态文本检查、源关系核对、真实目标入口和读者理解各自记录。本文件与手册交付只提供设计及方法；未执行的游戏、浏览器、模型回复、玩家观察或公开发布保留未执行，不由文案存在推定通过。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-NARRATIVE-001](../../product/world-rules-core-gameplay/prd.md#req-wr-narrative-001)；Trace refs: O7-REQ-5bde78bc-7ce9-48f3-820b-8415dc02ee00 | [世界前提文本](#narrative-premise-content) | 原设定和玩家位置准确，未采纳小说特有事实 | [NARRATIVE-PREMISE](world-narrative-content-delivery.manual.md#narrative-premise-check)：固定产品原文与 P01–P06、组合短文逐句核对；静态内容层 | 当前 PR 的文本审读与源版本 | 游戏内新导语接入、具身能力与发行可用性 |
| [REQ-WR-NARRATIVE-002](../../product/world-rules-core-gameplay/prd.md#req-wr-narrative-002)；Trace refs: O7-REQ-5f3a2585-c3f2-4bce-9da6-08ac6638cf4f | [叙事事实边界](#narrative-fact-boundaries) | 摘要每项断言可追溯，不创造关系/功劳/世界状态 | [NARRATIVE-FACT](world-narrative-content-delivery.manual.md#narrative-fact-check)：正常、未知、误归因纠正逐句对照输入与缺失项；静态审校，集成另取同候选合法读面 | 原 PR 的断言对照与实际消费证据 | 全历史系统、生成叙事质量或全部事件类型 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | [Viewer 内容](#narrative-viewer-content) | 读者分清环境、当前目标与原结果 | [NARRATIVE-READER](world-narrative-content-delivery.manual.md#narrative-reader-check)：按 PW-READABILITY 的实际候选、语言与原始回答方法选择本范围场景 | 消费 PR 的真实页面与理解记录 | 静态样例不能代签目标玩家理解或全部舞台 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [知识与时序](#narrative-knowledge-boundaries) | 当前/历史/未知/冲突和读取范围不混同 | [NARRATIVE-KNOWLEDGE](world-narrative-content-delivery.manual.md#narrative-knowledge-check)：Feed 状态与单事件 freshness 交叉审校，实际变更再沿 PW-FRESHNESS | 静态对照；实施 PR 的同候选投影和恢复证据 | 授权、真实 Runtime 重验与全入口验证 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | [人称与归属](#narrative-perspective) | 观察焦点、本人结果与公共事件不互换 | [NARRATIVE-FACT](world-narrative-content-delivery.manual.md#narrative-fact-check)：样例 C、A 操作切 B、其他 owner/匿名输入；实际路径沿 PW-IDENTITY | 原 PR 的校正文案；实际隔离证据 | 仅文字审校不证明服务器权限和多人隔离 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)；Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [目标与结果文案](#narrative-intent-content) | pending、旧 accepted/未知与已确认后果分别表达 | [NARRATIVE-FACT](world-narrative-content-delivery.manual.md#narrative-fact-check)：用例 F2/F3 对照原请求与回执；运行层按原结果查询核对 | 原 PR 的语义对照与消费候选结果 | 不能由句子证明执行、幂等或取消能力 |
| [REQ-PROMPT-002](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002)；Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293 | [Agent 消息内容](#narrative-dialogue-content) | 消息/回复/失败、草稿与配置来源可辨 | [NARRATIVE-DIALOGUE](world-narrative-content-delivery.manual.md#narrative-dialogue-check)：原文与外壳逐项审校；真实回复沿 PWT-001 并保留其因果 oracle 限制 | 原 PR 的文本对照；实际对话运行证据 | 不证明本次新回复、持久目标或人物主体感 |
| [REQ-SITE-DISCOVERY-001](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-001)；Trace refs: O7-REQ-4544302c-40be-4aa3-a557-21fe6f475c19 | [官网介绍](#narrative-site-content) | 类型、玩家分工和下一步不扩大可用性 | [NARRATIVE-SITE](world-narrative-content-delivery.manual.md#narrative-site-check)：首屏/首说明 CN/EN 文本核对，实际页面按 Site 既有理解验证 | Site 消费 PR 的来源与理解记录 | 页面静态文字不证明正式玩家发布 |
| [REQ-SITE-DISCOVERY-002](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-002)；Trace refs: O7-REQ-4b01d2d4-ec3a-4dbb-90f8-fca0c4b0a822 | [官网事实案例](#narrative-site-evidence) | 案例各步与其固定来源一致，类型及窗口可读 | [NARRATIVE-SITE](world-narrative-content-delivery.manual.md#narrative-site-check)：正文/摘要/译文/RSS 与原案例逐项核；浏览不产生世界效果沿原 Site 方法 | Site 原证据与消费 PR | 历史 CLI 不证明当前 GUI、安装包或持续生产 |
| [REQ-SITE-DISCOVERY-005](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#req-site-discovery-005)；Trace refs: O7-REQ-ee159d08-7998-44e2-bac5-930b7fa8a0ae | [小说导读](#narrative-fiction-entry) | 小说身份、实际语言与具身边界保留 | [NARRATIVE-SITE](world-narrative-content-delivery.manual.md#narrative-site-check)：CN/EN 导读与中文 reader，正常/空/失败沿原 Site 检查 | Site 消费 PR 的文本与入口结果 | 小说质量、英文正文或可玩剧情 |

## 12. 决策、风险与未决问题

将世界前提与事实叙事放在 game，是因为它们共享同一世界含义；具体布局、翻译方法和运行能力继续归原 Viewer/Site/Runtime。独立内容专题保存完整文本、跨来源样例和制作关系，避免把这些工作散落在小说规划、代码字符串和发行说明之间。

当前有三个明确限制：普通 Feed 的安全 summary 很简短，内容作者不能擅自丰富；新世界导语及补充中文尚未指定实际接入位置和完整英文译文；长期历史类别没有因本次审读取得完整消费证据。前两项在实际采用文字前由对应消费方与内容 owner 核定，第三项在进入相应实现时沿原产品/专业条款核验。它们不阻止本次有限内容交付，也不被记为已完成能力。

叙事流畅度与可信度可能冲突时先保留能确认的事实，再改善句子。一个简短但不误导的记录，优于依赖虚构人物、关系、地点或功劳的完整故事；是否足以让玩家愿意回看，仍须实际读者与游戏体验证据。
