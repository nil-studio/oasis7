# 玩家可读表面的连续性

## 文档身份

- 所属产品模块：智能体、世界模拟与交互
- 上位产品 PRD：[`prd.md`](prd.md)
- 配对产品设计：[`doc/product/agents-world-simulation/player-readable-surface-continuity.design.md`](player-readable-surface-continuity.design.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- 专业域权威：[`Viewer 手册`](../../world-simulator/viewer/viewer-manual.manual.md)、[`world-simulator PRD`](../../world-simulator/prd.md)、[`Web 语义测试 API`](../../world-simulator/viewer/viewer-web-semantic-test-api.prd.md)
- Last reviewed：2026-10-12

本文定义正式玩家表面在 viewport、信息密度、语言与连接状态变化时保持可理解、可操作和可恢复的长期产品承诺。它不冻结 Viewer 布局、组件、Web/EGUI/Bevy 实现、协议字段、缓存、字体资产或当前发布结论。

本文的规范性条款是目标要求；`active` 表示产品语义权威，不表示每项能力已经实现或完成验收。实现范围和验证结论由对应专业合同、PR 与实际证据说明。

## 设计适用性与生命周期闭合

- 设计判定：`paired-design`。
- 配对关系：[player-readable-surface-continuity.design.md](player-readable-surface-continuity.design.md) 承接本 PRD 的玩家经历、信息层级、状态反馈与恢复解释；产品真值仍由本 PRD 拥有。
- 设计适用性理由：跨 surface 的阅读锚点、连接状态、fallback 与行动反馈由同名 design 承接。
## 1. 目标与产品承诺

玩家在受支持的表面中能够持续找到当前目标、主要 blocker、已接受行动的反馈和下一决策。viewport、信息密度、语言或连接状态变化可以改变呈现方式，但不能让玩家失去这条决策链，也不能把恢复中的界面或后台活动表达为已经取得进展。

当前世界模式可支持的动作集合也属于这条决策链。surface 必须让玩家辨认当前可以做什么；动作不受当前模式支持时，应如实说明这是能力或世界边界，并给出适用的替代动作或安全返回路径，不能把它伪装成断线、发送失败或已经执行。

## 2. 可用空间与信息密度

- 在受支持的 viewport 尺寸下，主要世界或行动表面保持可读和可操作；面板、设置和次级信息可以折叠、隐藏或重排，但可支持的决策面必须能够被发现和恢复。
- 玩家可以降低信息密度或收起次级内容，且不会因此永久丢失当前目标、主要 blocker、接受/结果反馈或下一步。
- 遮挡、窄屏或低高度不能让关键状态只能依赖 hover、颜色、动效或不可恢复的布局位置。
- surface 可以选择全屏、分区或响应式组合，但产品不承诺具体 toggle、panel、module、minimap、toast 或历史 Viewer 控件存在。

### 2.1 当前模式与行动边界

- 当前 surface 只呈现或接受当前模式确实支持的行动；不支持的行动与连接中断、权限拒绝和已接受请求是不同的玩家语义。
- 模式、布局或可见动作集合变化不会创造回退、重放、权限或世界修改能力；玩家仍能获得一条真实的替代动作、等待、返回或重新聚焦路径。
- 行动入口、时间展示或自动推进可以变更，但界面计数、后台调度、重新加载或自动开始不能代替权威世界后果。

## 3. 语言与可访问文本

- 受支持的玩家语言具有明确的选择与 fallback；必要的意图、状态、错误、恢复和下一步文本保持可读。
- 语言选择只改变表达，不改变世界事实、动作权限、接受/拒绝语义或执行结果。
- 缺少翻译或字体覆盖时，surface 必须诚实降级并保留关键含义，不能以空白、乱码或静默回退隐藏状态。
- 本产品承诺不包含自动采用操作系统 locale、特定字体资产、云端或跨设备同步，也不冻结翻译键、缓存路径或持久化机制。

## 4. 连接中断与恢复

- 连接中断、重连中、恢复成功或恢复失败必须可区分，并提供适用的重试、返回或重新进入路径。
- reconnect、重新加载、后台 tick 或重新出现的画面不能代签玩家意图已接受、动作已完成或世界已推进。
- 恢复后，玩家能够重新找到当前目标、主要 blocker、最近可信反馈和下一决策；无法恢复时明确返回安全的决策入口。
- websocket、callback、backoff、timeout、software-safe、WASM 或 transport 兼容由专业域拥有，产品层不复制。

### 4.1 重叠状态的主语义与真值优先级

多个状态同时出现时，surface 不能用最近到达的 transport 事件、页面刷新或客户端计数覆盖已经确认的世界语义。默认主语义按下表决定；其他仍有决策价值的状态可以作为次级 blocker 展示，但不得改写主语义。

| 重叠条件 | 主语义与玩家可见边界 | 允许的下一步 | 禁止的推断 |
| --- | --- | --- | --- |
| 已有 committed receipt，同时断连或恢复中 | 以已确认结果为主；连接只影响查看后续状态，不撤销结果 | 查看 receipt、等待重连或进入新的受支持决策 | 把断连表示为回滚、未执行或需要自动重放 |
| 请求已被接受，且适用权威状态确认尚未 committed，同时断连或恢复中 | 以“已确认尚未生效的待决请求”为主；断连是恢复 blocker | 恢复当前读取资格后查询原请求、等待，或在专业合同允许时明确撤回/替代/重规划 | 从无回执或断连推定尚未生效；把送达、界面计数或重连成功表示为成功、失败或取消 |
| 已送出请求，但无法从适用权威状态确认是否 committed | 保持“提交结果未知、待核对”；会话/读取阻塞单独表达 | 恢复当前读取资格后按原请求核对；查询受阻时等待、重新认证或安全停止 | 将超时、无回执、单端未找到、旧缓存或无读取权限解释为未生效；另起请求来消除未知 |
| 请求尚未被权威接受，同时权威不可用、陈旧或无法验证 | 以“未确认/需重新验证”为主，不提前判定接受或拒绝 | 重新验证、等待、改道或安全返回 | 把本地排队、请求发送或缓存响应表示为拒绝、成功或资格 |
| 当前行动在模式中不受支持，同时连接中断 | 以“能力边界”为主；连接状态可独立展示给其他受支持行动 | 选择当前模式支持的替代、等待或安全返回 | 把不支持误报为传输失败，或因重连制造该行动能力 |
| 已有权威拒绝/规则或权限阻塞，同时随后断连 | 以拒绝或阻塞结果为主；断连只影响查看详情或重新评估 | 查看原因、修复前置、改道或在条件允许时显式重新提交 | 自动重试、把拒绝改写为断连失败，或继承旧权限 |

本表消费[入口的请求结果与会话正交边界](../player-entry-distribution/prd.md#player-session-unknown-outcome-acceptance)：权威确认未提交与客户端结果未知必须分别表达。尚未决定的请求与已有固定执行决定的续接仍分别遵循 runtime/P2P 原合同；本表不新增 wire 状态或授权。

该优先级只规定玩家的默认首要解释，不冻结字段、状态枚举或 UI 排版。任何新的提交仍须按当前权威条件独立校验；重连、刷新和跨入口查看不能产生第二次世界效果。

### 4.2 本人的行动结果与提交前判断

玩家正在查看自己一次行动的结果时，另一位玩家或未登录连接可能同时发起失败请求。玩家应继续读到与本人行动对应的结果；别人的拒绝不能覆盖这条反馈，也不能变成本人的 blocker。共享世界中有权公开的事件仍可显示，但要与本人的操作反馈区分。

正常路径是先确认当前操作身份和目标；行动提供提交前检查时，查看当前资格、主要成本和缺少的条件，再选择修改、等待、返回或明确提交。检查通过只说明本次检查可确认的条件，不能预留未来权限或保证执行成功。提交后按真实的待决、拒绝或已确认结果继续；拒绝应给出本人有权获得的原因和真实下一步，未知结果先核对原请求，不引导重复行动。

## 5. 叶级产品要求与验收

<a id="req-agent-surface-001"></a>
### REQ-AGENT-SURFACE-001：跨 surface 保留同一决策锚点

Trace ID: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef

- 功能点：[REQ-AGENT-SURFACE-001](#req-agent-surface-001)；适用范围沿本条原条件与 [AC-AGENT-SURFACE-001](#ac-agent-surface-001)，不扩大入口或阶段。
- 系统设计状态：未核实
- 系统设计依据：本轮已读本条、对应验收及所列专业 authority；尚未核清“viewport/语言/连接状态的同一决策锚点”的逐项专业设计承接。产品体验 design 或模块入口本身不作技术覆盖证明；不据此断言无设计。
- 实现状态：部分实现
- 实现与验证依据：[Runtime gameplay快照生产](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/control_blocking.rs#L345-L455)→[snapshot到玩家摘要](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_feedback_module.js#L332-L380)→[snapshot/summary输入host](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L44-L65)→[世界HUD目标/blocker/next入口](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L315-L350)及[同一原回执读取](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L231-L273)构成真实生产/读取链；[双语locale选择与fallback](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_locale_preferences_module.js#L68-L78)与[Web世界host与图形失败文本/恢复](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L1255-L1325)支持双语及图形失效保留目标/回执的直接子范围。
- 核对日期与基线：2026-10-17；需求/设计/静态实现固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，源码逐文件 Git blob 校验；本轮未运行模型、Runtime、浏览器或产品验收。既有PR运行记录仅按各自候选/范围引用。
- 剩余范围：完整desktop/窄屏/低高度、双语fallback和连接中断/恢复中/恢复失败组合仍未逐一核清；不是仅欠测试。需核四个决策锚点在每个适用表面的实现、可读性与真实下一步；保留权限/世界结果不因展示变化改变的验收。

- 要求：在受支持 viewport、语言和连接状态下，玩家必须能找到当前目标、主要 blocker、最近可信反馈和下一决策或恢复入口；呈现变化不得改写权限或世界结果。
- 验收：AC-AGENT-SURFACE-001

<a id="ac-agent-surface-001"></a>
### AC-AGENT-SURFACE-001：窄屏、fallback 与恢复状态仍可读

Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；承接[REQ-AGENT-SURFACE-001](#req-agent-surface-001)。

- 覆盖要求：REQ-AGENT-SURFACE-001
- 场景与结果：代表性 desktop、窄屏、低高度和双语 fallback 样例中，四个决策锚点仍可辨认；中断、恢复中、恢复失败和已接受结果保持不同语义，并提供真实下一步。
- 证据边界：只证明玩家可读语义和不误导边界；具体布局、控件、locale 资源和连接字段由 Viewer/WASM/QA authority 验证。

<a id="req-agent-surface-002"></a>
### REQ-AGENT-SURFACE-002：不支持行动不得伪装成传输失败

Trace ID: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64

- 功能点：[REQ-AGENT-SURFACE-002](#req-agent-surface-002)；适用范围沿本条原条件与 [AC-AGENT-SURFACE-002](#ac-agent-surface-002)，不扩大入口或阶段。
- 系统设计状态：未核实
- 系统设计依据：本轮已读本条、对应验收及所列专业 authority；尚未核清“模式不支持、权限/规则阻塞、传输中断与未知请求区分”的逐项专业设计承接。产品体验 design 或模块入口本身不作技术覆盖证明；不据此断言无设计。
- 实现状态：部分实现
- 实现与验证依据：[模式/就绪拒绝代码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/control_blocking.rs#L482-L527)生产llm_mode_required/llm_init_failed；[实际发送前gate及请求状态](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/legacy_core.js#L1207-L1297)区分blocked、unsupported-before-send与queued，[状态读取和可读解释](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_feedback_module.js#L475-L553)区分accepted等待committed、world_constraint、rejected及completed。[FeedbackCard 实际界面](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L491-L516)；[gameplay 执行状态实际界面](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2575-L2602)消费独立状态。这是模式/阻塞/已受理的直接子范围，不是仅凭enum存在。
- 核对日期与基线：2026-10-17；需求/设计/静态实现固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，源码逐文件 Git blob 校验；本轮未运行模型、Runtime、浏览器或产品验收。既有PR运行记录仅按各自候选/范围引用。
- 剩余范围：断连时[pending失败处理](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/legacy_core.js#L1134-L1152)仍写semantic error；尚需核清每条实际行动的unknown原请求lookup、适用替代/等待/返回，以及重连不会创造能力/效果的全链。不能把完整未知请求恢复仅写成待验收；本轮未执行行为验证。

- 要求：当当前模式不支持某行动、权限/规则阻塞或请求已被接受时，surface 必须与连接中断和未确认请求区分，并给出适用的替代、等待、返回或重新验证路径。
- 验收：AC-AGENT-SURFACE-002

<a id="ac-agent-surface-002"></a>
### AC-AGENT-SURFACE-002：重连不会创造能力或世界效果

Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64；承接[REQ-AGENT-SURFACE-002](#req-agent-surface-002)。

- 覆盖要求：REQ-AGENT-SURFACE-002
- 场景与结果：不支持行动、权限阻塞、已接受未结算请求和断连同时出现时，首要解释按当前权威语义呈现；重连、刷新、后台 tick 或跨入口查看不会把请求变成成功、取消或第二次世界效果。
- 证据边界：请求 acceptance、世界结果、去重与恢复合同由 runtime/Agent 专业 authority 提供；产品层不冻结 transport 或 UI 实现。

<a id="req-agent-surface-003"></a>
### REQ-AGENT-SURFACE-003：个人行动反馈必须属于当前玩家

Trace ID: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6

- 功能点：[REQ-AGENT-SURFACE-003](#req-agent-surface-003)；适用范围沿本条原条件与 [AC-AGENT-SURFACE-003](#ac-agent-surface-003)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation)：请求连接、已验证玩家及世界故障的隔离与身份切换；各正式表面的呈现及真实多人组合范围须另核。
- 实现状态：部分实现
- 实现与验证依据：[固定生产源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/gameplay_feedback_scope.rs#L60)：生产反馈按已验证player/key/session/world/binding归属并隔离旧代次；[对应隔离回归源码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/tests/auth_actions_feedback.rs#L263)已审读但未重跑；实际交付PR映射未核清，文档Task [#4694](https://github.com/eng-cc/oasis7/issues/4694)不代签。剩余实际双玩家/匿名浏览器与跨surface验收。
- 核对日期与基线：2026-10-15；需求/设计与静态实现基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；入口为条目所述子集；本轮未执行真实Runtime、浏览器、部署或真人验收，历史运行候选依各记录独立限定。
- 剩余范围：设计：保留现有已核实范围，完整承接仍须核对；实现/验证：实际双玩家/匿名浏览器与跨surface验收。

- 性质与适用条件：目标要求；适用于正式玩家表面的个人行动反馈及其重连、刷新和身份切换。
- 要求：产品必须依据经确认的当前身份展示该玩家有权读取的行动反馈；无关玩家或匿名请求的失败不得覆盖、追加到或暴露为本人的个人反馈。匿名请求可以获得自身有权获得的拒绝说明，但不能借请求中的身份声明影响已鉴权玩家。
- 理由与上位承诺：保留本人结果、避免误判和私有反馈泄露，细化[根成功标准 SC-3 / SC-10](prd.md#5-done：成功标准与验收)。
- 专业权威：[玩家反馈归属合同](../../world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md#player-scoped-gameplay-feedback)；产品经历见[反馈归属设计](player-readable-surface-continuity.design.md#player-feedback-ownership)。
- 验收：[AC-AGENT-SURFACE-003](#ac-agent-surface-003)。

<a id="ac-agent-surface-003"></a>
### AC-AGENT-SURFACE-003：两名玩家与匿名失败互不污染

Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；承接[REQ-AGENT-SURFACE-003](#req-agent-surface-003)。

- 覆盖要求：[REQ-AGENT-SURFACE-003](#req-agent-surface-003)。
- 给定：两名已鉴权玩家各自已有可信个人反馈，以及一个未鉴权连接。
- 当：分别由本人、另一玩家和匿名连接发起被拒绝的请求，并交叉刷新、重连或切换身份读取反馈。
- 则：有权的请求方能看到自己的拒绝；另一玩家的个人反馈不变，匿名拒绝不进入任何已鉴权玩家的个人反馈。身份切换后不显示先前身份的私有反馈；合法公共事件仍按现行可见性展示，不冒充本人行动结果。

<a id="req-agent-surface-004"></a>
### REQ-AGENT-SURFACE-004：拒绝原因必须支持真实下一步

Trace ID: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4

- 功能点：[REQ-AGENT-SURFACE-004](#req-agent-surface-004)；适用范围沿本条原条件与 [AC-AGENT-SURFACE-004](#ac-agent-surface-004)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#gameplay-error-code-preservation)：合法鉴权后的业务错误码保真；各失败类别到玩家恢复动作的完整表现承接须另核。
- 实现状态：部分实现
- 实现与验证依据：[合法鉴权后builder错误码保真](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/gameplay_submit_api.rs#L275-L295)当前直接返回err.code/err.message，已不再把该分支统一改写为invalid_request。这是HTTP adapter切片，不是所有失败类型的玩家恢复体验。历史[PR#2485 review](https://github.com/nil-studio/oasis7/pull/2485#discussion_r3614431998)保留原问题语境；本轮没有取得此修复的独立运行结果，不将现状静态核对写成测试通过。
- 核对日期与基线：2026-10-17；需求、设计与静态实现固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；历史执行候选、入口和环境分别按上列记录限定。本轮只读核查，未运行测试、模型、浏览器或部署。
- 剩余范围：已确认合法鉴权后的builder错误码保真；各失败类别到修正输入、身份/权限、世界条件、等待和安全停止的正式surface映射、未知结果与所有入口验收仍未核清。

- 性质与适用条件：目标要求；适用于本人有权获得的行动拒绝与失败说明。
- 要求：产品必须保留足以区分修正输入、重新确认身份或权限、补齐世界条件、等待或安全停止的失败含义；已有明确原因不能统一降成“请求错误”或“请重连”。原因不足或结果未知时如实说明，不能编造成功、失败细节或必然有效的重试。
- 理由与上位承诺：减少无效重试并让玩家继续决策，细化[根成功标准 SC-3 / SC-10](prd.md#5-done：成功标准与验收)。
- 专业权威：[行动错误语义合同](../../world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md#gameplay-error-code-contract)；产品经历见[可操作拒绝设计](player-readable-surface-continuity.design.md#actionable-rejection)。
- 验收：[AC-AGENT-SURFACE-004](#ac-agent-surface-004)。

<a id="ac-agent-surface-004"></a>
### AC-AGENT-SURFACE-004：不同失败仍能选择正确恢复路径

Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4；承接[REQ-AGENT-SURFACE-004](#req-agent-surface-004)。

- 覆盖要求：[REQ-AGENT-SURFACE-004](#req-agent-surface-004)。
- 给定：身份有效但行动输入不合法、权限不足、世界前置不满足，以及投递后结果未知的独立样例。
- 当：玩家读取各自反馈并选择下一步。
- 则：可确认的失败保留相应原因和可行恢复路径；输入问题不会被包装为连接问题，未知结果先核对原请求。解释不泄露其他玩家私有信息或凭据，也不因语言或入口变化改变拒绝的含义。

<a id="req-agent-surface-005"></a>
### REQ-AGENT-SURFACE-005：提交前检查必须与当前提交资格一致

Trace ID: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d

- 功能点：[REQ-AGENT-SURFACE-005](#req-agent-surface-005)；适用范围沿本条原条件与 [AC-AGENT-SURFACE-005](#ac-agent-surface-005)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#collect-data-preflight-consistency)：CollectData 同一权威状态下预检与提交资格一致；其他受支持行动的适用范围和正式表面反馈须另核。
- 实现状态：部分实现
- 实现与验证依据：[CollectData共同授权和资格计算](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/collect_data.rs#L113-L164)确认Preflight/Submit消费同一请求及data_collection_preflight，blocked时拒绝；[精确签名请求测试](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/tests/auth_actions_collect_data.rs#L855)为已读测试定义。[原交付#2485](https://github.com/nil-studio/oasis7/pull/2485)在source cac54892196bfd304401cebc6bb70d2b3ea3d681报告局部测试通过，不能代签当前全部受支持行动；本轮未执行。
- 核对日期与基线：2026-10-17；需求、设计与静态实现固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；历史执行候选、入口和环境分别按上列记录限定。本轮只读核查，未运行测试、模型、浏览器或部署。
- 剩余范围：已确认runtime-live CollectData同请求共用资格计算切片；需逐项核清chain/WorldService正式路径、其他已提供preflight的受支持行动及状态漂移时玩家提示，并取得同候选组合验收。

- 性质与适用条件：目标要求；仅适用于已提供提交前检查的受支持行动，不要求所有行动新增检查入口。
- 要求：对同一经确认身份、同一行动意图与同一权威世界状态，检查对共享资格条件的判断必须与提交时的资格校验一致；无法确认身份或权威状态时明确不可确认或阻塞。检查不得被表达为行动已接受、已执行或已结算，也不保证状态变化后的提交成功。
- 理由与上位承诺：玩家能够依据检查选择下一步，同时理解观察与行动之间的变化风险，细化[根成功标准 SC-3 / SC-5 / SC-6](prd.md#5-done：成功标准与验收)。
- 专业权威：[CollectData 检查与提交合同](../../world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md#collect-data-preflight-contract)；产品经历见[检查与提交边界](player-readable-surface-continuity.design.md#preflight-submission-boundary)。
- 验收：[AC-AGENT-SURFACE-005](#ac-agent-surface-005)。

<a id="ac-agent-surface-005"></a>
### AC-AGENT-SURFACE-005：资格一致且状态变化后重新判断

Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d；承接[REQ-AGENT-SURFACE-005](#req-agent-surface-005)。

- 覆盖要求：[REQ-AGENT-SURFACE-005](#req-agent-surface-005)。
- 给定：相同身份、行动与权威状态下的合格和不合格样例，另有检查后权限、绑定或资源条件改变的样例。
- 当：依次进行提交前检查和真实提交。
- 则：未变化样例对共享资格条件给出一致判断；条件变化时按提交时权威事实重新判断，并能读到变化或拒绝原因。检查通过及请求受理均不显示为世界执行成功；无法取得权威状态时不使用旧检查结果继续承诺资格。

## 5.1 组合验收

- PSC-1：代表性 desktop、窄屏和低高度 viewport 中，当前目标、主要 blocker、接受/结果反馈与下一步保持可读且可恢复。
- PSC-2：信息密度或 panel 可见性改变后，玩家仍能找到主要决策面；隐藏次级内容不会制造权限、结果或世界事实变化。
- PSC-3：受支持语言及其 fallback 中，关键意图、状态、错误和恢复语义等价，语言选择不改变权威结果。
- PSC-4：断连与恢复样例明确区分连接状态、request acceptance 与权威进展；恢复或后台活动不代签成功。
- PSC-5：证据来自当前 Viewer、runtime 与 QA 专业权威；历史 EGUI/Bevy/Web 完成记录、单张截图或本地 fallback 不能单独成立产品结论。
- PSC-6：代表性模式转换或不支持行动样例能区分能力边界、断连、权限/规则阻塞与已接受请求，并提供真实替代或安全返回；自动推进或接口计数不代签权威世界结果。
- PSC-7：代表性重叠状态（权威确认未结算+断连/恢复中、已送出且结果未知+查询受阻、未接受+权威不可用、不支持行动+断连、已拒绝+断连）按 4.1 的主语义显示唯一首要解释和可执行下一步；重连、刷新或跨入口重试不覆盖已确认结果、不把 pending 变成成功/失败/取消，也不产生第二次世界效果。

## 5.2 专业 owner、authority 与测试层级追踪

本表是产品语义到专业合同和验证证据的导航，不是当前实现、通过结论或发布/readiness 声明。每个 owner 只负责表中列出的边界；证据必须由对应专业 authority 在适用的任务、测试和 artifact 中产生。

| REQ / AC | 专业 owner 与真实边界 | 专业域 PRD-ID | 专业权威（可导航） | 验证证据（应提供，不预示当前结果） | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-SURFACE-001](#req-agent-surface-001) / [AC-AGENT-SURFACE-001](#ac-agent-surface-001) | `producer_system_designer`：产品语义、非目标和组合验收，不拥有专业实现或 readiness verdict；`viewer_engineer`：viewport、fallback 文本、连接状态和下一步在正式 surface 的可读表达，不拥有世界真值；`runtime_engineer`：committed/pending/recovery 的权威结果与无第二次效果，不拥有布局；`wasm_platform_engineer`：WASM bridge/executor 的 ABI、限制、结构化失败和兼容边界，不拥有 locale 或布局；`agent_engineer`：Agent observation/action、provider 失败反馈和模式边界，不授予 world acceptance；`qa_engineer`：跨 surface 的可复验证据与失败签名，不以单一局部 green 代签产品结论。 | `PRD-WORLD_SIMULATOR-001/039/041/046`；`PRD-WORLD_RUNTIME-001/031/033`；`PRD-WORLD_RUNTIME-002/036/037`；`PRD-WORLD_SIMULATOR-040/040B`；`PRD-TESTING-003` / `PRD-TESTING-WEB-001/002/003` | [`world-simulator active baseline`](../../world-simulator/prd.md#active-requirement-baseline)、[`Viewer 证据边界`](../../world-simulator/viewer/viewer-manual.manual.md#证据边界)、[`Viewer 连接与 no-throw 合同`](../../world-simulator/viewer/viewer-web-semantic-test-api.prd.md#current-connection-and-no-throw-boundary)、[`runtime receipt 边界`](../../world-runtime/prd.md#executionreceipt、tick-compatibility-与-finality-边界)、[`WASM sandbox 与工件完整性`](../../world-runtime/wasm/wasm-executor.prd.md#sandbox-与工件完整性契约)、[`Agent 双模式验证`](../../world-simulator/llm/provider-agent-dual-mode.prd.md#6-validation-decision-record)、[`testing validation`](../../testing/prd.md#6-validation-decision-record) | `Viewer`：headed S6 desktop、窄屏、低高度和双语 fallback 样例，保存 screenshot、console、snapshot/state 与实际操作结果，检查目标、blocker、可信反馈和恢复入口；`runtime`：同一 fixture 对账 committed receipt、pending/recovery、canonical state/journal/replay root；`WASM`：ABI/limit/capability rejection、工件 hash、结构化错误和 replay/no-effect 证据；`Agent`：`player_parity`/`headless_agent` 的同场景 observation/action、mode metadata 与 provider 失败到 Wait/拒绝的证据；`QA`：把上述输入合并为可复核的跨 surface 证据包，并记录未覆盖 viewport、语言、连接状态和专业边界。 | `test_tier_required`：产品/authority 链接治理、Viewer/Agent/runtime/WASM 定向合同和最小 S6 required 输入；`test_tier_full`：跨 surface 的双语、窄屏/低高度、恢复与 replay 对账，含 Agent parity、WASM 兼容/失败路径和 QA 组合审查。 |
| [REQ-AGENT-SURFACE-002](#req-agent-surface-002) / [AC-AGENT-SURFACE-002](#ac-agent-surface-002) | `producer_system_designer`：产品语义、非目标和组合验收，不拥有专业实现或 readiness verdict；`viewer_engineer`：把能力边界、权限/规则阻塞、accepted/pending 和断连表达为不同可读状态，不制造 action 或 world effect；`runtime_engineer`：acceptance、commit、拒绝、去重、恢复和 canonical world effect 的唯一顺序；`wasm_platform_engineer`：模块调用的 deterministic limits、compatibility fault 和失败时无副作用边界；`agent_engineer`：Agent/provider 的 action contract、Wait/拒绝与恢复反馈，不把 provider 建议当作权威应用；`qa_engineer`：覆盖重叠状态和负例的 required/full 证据，不把 reconnect/refresh 当成功。 | `PRD-WORLD_SIMULATOR-039/041/046`；`PRD-WORLD_RUNTIME-001/031/033/047`；`PRD-WORLD_RUNTIME-002/036/037`；`PRD-WORLD_SIMULATOR-040/040B`；`PRD-TESTING-003` / `PRD-TESTING-WEB-001/002/003` | [`runtime snapshot/replay contract`](../../world-runtime/prd.md#snapshot、replay、version-与-canonical-timeline-兼容)、[`Agent authority/capability boundary`](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md#6-agentintentv2-与-authoritycapability-边界)、[`Agent deterministic/replay acceptance`](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md#8-acceptance-与-deterministicreplay-verification)、[`WASM sandbox 与工件完整性`](../../world-runtime/wasm/wasm-executor.prd.md#sandbox-与工件完整性契约)、[`Agent 双模式验证`](../../world-simulator/llm/provider-agent-dual-mode.prd.md#6-validation-decision-record)、[`Web UI 验证决策记录`](../../testing/manual/web-ui-agent-browser-closure-manual.prd.md#6-validation-decision-record)、[`testing validation`](../../testing/prd.md#6-validation-decision-record) | `Viewer`：不支持行动、权限/规则阻塞、accepted 未结算、未确认和断连重叠样例的 headed S6 状态/操作证据；`runtime`：fresh/stale/duplicate/conflict、accepted-but-uncommitted、reconnect/replay/restart fixture 的 disposition、receipt/state root 和 exactly-once 对账；`WASM`：超时、超限、坏工件、compatibility fault 与恢复后的 no-effect/结构化错误证据；`Agent`：相同 observation/action contract 下 provider failure、Wait/拒绝、重试和 replay 不产生隐式替代行动或第二次效果；`QA`：按状态优先级核对首要解释、真实下一步与未覆盖组合，明确 required 与 full 的证据窗口。 | `test_tier_required`：状态分类、能力边界、拒绝/Wait/no-effect 与玩家可读下一步的定向合同及 S6 required 输入；`test_tier_full`：四类重叠状态的恢复、replay、去重、跨入口/跨 adapter 组合证据，以及 Viewer/runtime/WASM/Agent/QA 的同候选对账。 |
| [REQ-AGENT-SURFACE-003](#req-agent-surface-003) / [AC-AGENT-SURFACE-003](#ac-agent-surface-003) | viewer_engineer / runtime_engineer：身份与个人反馈隔离；qa_engineer：交叉负例 | PRD-WORLD_SIMULATOR-039 / PRD-TESTING-003 | [反馈隔离系统设计](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation) | 两名玩家和匿名请求的反馈读取、刷新、重连及身份切换；本人拒绝可达、其他个人反馈不变 | test_tier_required |
| [REQ-AGENT-SURFACE-004](#req-agent-surface-004) / [AC-AGENT-SURFACE-004](#ac-agent-surface-004) | viewer_engineer / gameplay_designer：错误与恢复语义；runtime_engineer：真实拒绝原因；qa_engineer：分类与入口核对 | PRD-WORLD_SIMULATOR-039 / PRD-TESTING-003 | [错误语义系统设计](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#gameplay-error-code-preservation) | 输入、权限、世界条件和未知结果独立判定；身份有效但输入非法的专项负例 | test_tier_required |
| [REQ-AGENT-SURFACE-005](#req-agent-surface-005) / [AC-AGENT-SURFACE-005](#ac-agent-surface-005) | viewer_engineer / gameplay_designer / runtime_engineer：检查与提交资格；qa_engineer：同状态及变化样例 | PRD-WORLD_SIMULATOR-039 / PRD-TESTING-003 | [检查与提交系统设计](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#collect-data-preflight-consistency) | 同身份/意图/权威状态的资格正反例，以及检查后权限、绑定或资源变化与未知状态 | test_tier_required |

### 5.3 表现与视觉证据边界

视觉改动继续消费[决策锚点](#req-agent-surface-001)、[世界舞台的目的与因果](player-readable-world-stage.prd.md#req-agent-stage-001)和[观察来源与未知](player-readable-world-stage.prd.md#req-agent-stage-002)，不新增固定颜色、帧率、渲染算法或产品页面。画面优化须保留适用范围内的选中对象、空间含义和可访问反馈；回归对比、渲染复用与浏览器验证方法由[渲染专业合同](../../world-simulator/viewer/viewer-pixel-world-bridge-rendering.prd.md#render-reconcile-and-visual-regression-requirements)与[视觉回归设计](../../world-simulator/viewer/viewer-pixel-world-bridge-rendering.design.md#visual-regression-target)负责。

确定性视觉比较证明其选中场景的表现是否变化；浏览器证据另证明真实表面能否读到对应语义。只有非空画面、某种颜色存在或历史截图，不能单独证明指定视觉特征仍然有效，也不能外推为完整玩家体验。证据声明应限定候选、场景、入口和已覆盖的状态。

## 6. 范围与非目标

覆盖可用空间、信息密度、当前模式支持的行动、语言、关键文本、连接中断与恢复的跨 surface 产品连续性。不定义具体 panel/module、fullscreen 控件、布局宽度、hit boundary、字体/资产、local JSON/cache、控制 profile、时间/事件计数、WebSocket 时序、Test API、测试命令或发布/readiness claim。

同一受支持行动也通过 `pure_api` 提供时，个人反馈归属、拒绝含义和提交资格保持一致；不要求两种入口布局相同，也不据此新增行动能力。反馈存储、鉴权机制、错误码、数据结构、检查算法及性能指标仍归专业域。

## 7. 接口 / 数据

产品层只定义 `决策锚点 -> surface 状态变化 -> 权威反馈 -> 恢复入口` 的可读语义。viewport、locale、connection、request、ack、transport 和测试字段由 Viewer、runtime 与 testing 专业 authority 定义。

## 8. 里程碑

1. 维持稳定且可从模块入口到达的 PRD 与 design。
2. 吸收并删除历史 panel、declutter、fullscreen、i18n、Web usability 与 step acknowledgement 碎片文档。
3. 当前 Viewer/runtime/testing authority 持续提供实现和验证证据。

## 9. 风险

- 将旧 EGUI/Web 控件误写成当前能力。
- 将连接恢复或 request acceptance 误写成世界已经推进。
- 将语言、viewport 或本地偏好实现细节冻结为产品合同。
- 删除仍承担协议真值的专业文档；本专题只允许删除已有代码、测试或当前专业文档承接的历史来源。

## 10. 设计取舍与未决问题

个人结果的正确归属优先于显示一条看似连续的“最新反馈”；无法确认归属时保留未知或返回安全入口。检查提供当前判断而不冻结世界，玩家需要承担检查后条件变化、重新观察或重新提交的机会成本。两者都不能用自动重试掩盖。

未决的具体错误展示、身份切换后的历史反馈可见范围和检查失效提示，由 Viewer、runtime、gameplay 与 QA owner 在相应入口实施前依据现行授权和专业合同明确；未确认的范围不承诺跨账号反馈保留、固定重试策略或未来资格。
