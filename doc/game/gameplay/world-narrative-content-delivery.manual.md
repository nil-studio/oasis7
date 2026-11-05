# 世界前提与事实叙事制作验证手册

本手册配套[内容设计](world-narrative-content-delivery.design.md)，用于核对已采纳世界前提、事实摘要、现有 Feed/结果/消息与官网/小说的文本关系。静态审校可以在未启动游戏的情况下完成；实际文字接入后，再按受影响的原入口选择结构、运行和读者理解验证。方法存在不表示执行通过，结果直接回到原 PR 与证据入口。

每次先记录 source/tested tree、实际改动范围和读取的原内容版本。只有设计建议变化时，不运行 Viewer、Runtime、模型或制作工具；不能把目标文字的纸面检查记为运行测试。实际消费方变化时，固定同一候选的源、页面/bundle、语言与数据来源，再取该范围的证据。

<a id="narrative-premise-check"></a>
## 1. NARRATIVE-PREMISE：已采纳前提与表达

输入为[REQ-WR-NARRATIVE-001](../../product/world-rules-core-gameplay/prd.md#req-wr-narrative-001)、[AC-WR-NARRATIVE-001](../../product/world-rules-core-gameplay/prd.md#ac-wr-narrative-001)、原[世界前提](../../product/world-rules-core-gameplay/prd.md#world-premise-and-player-position)，以及实际待交付的 [P01–P06 与组合短文](world-narrative-content-delivery.design.md#narrative-premise-content)。按以下顺序审查，不以“题材一致”替代逐句核对。

1. 把待交付文字中的世界类型、舞台、玩家位置、Agent 行动和历史后果逐项对回原句。可以压缩或换一种表达，但不能删去玩家与 Agent 的分工，或把持续世界写成每次进入重新开始。
2. 核 P04–P06 的时间、资源和后果解释仍沿[原产品承诺](../../product/world-rules-core-gameplay/prd.md#1-产品承诺)及各真实消费者；不从短文推出特定成本、资源已经可用、事件已经发生或恢复已成功。
3. 将待稿与[小说背景](../../../site/story/background/world-background.md#7-使用边界)作边界对照。游戏介绍若出现 2076、五十位老人、量子通信、某位人物/对应关系或小说事件，必须有现行游戏条款的明确采纳；仅来自小说时留在小说导读，不能写成游戏前提。本步骤不要求逐章重审小说。
4. 对“文明之外”做反例审校：把它改写成全知、任意读取私有数据、直接修改物理世界或无授权控制，必须判为越界。具身/block-editing 仍按原未来候选范围，不由题材文案提升能力。
5. 若本次只是文案设计，结果记为静态稿件核对；若已接入某个入口，对照实际加载文字和该入口的模式/版本限制，确认设计建议没有被错标为已开放功能。

通过含义是既有前提不变、每项扩写有真实来源、小说与示意身份仍清楚。发现语义缺口先交内容作者修文；确需新的产品选择时交原产品 owner，不用补一条背景故事绕过决定。实际访客是否理解分工按 §7 取证。

<a id="narrative-fact-check"></a>
## 2. NARRATIVE-FACT：事实断言、归属与校正

承接[REQ-WR-NARRATIVE-002](../../product/world-rules-core-gameplay/prd.md#req-wr-narrative-002)与[AC-WR-NARRATIVE-002](../../product/world-rules-core-gameplay/prd.md#ac-wr-narrative-002)。只对声称描述世界事实的文字核对事实依据；小说或示意可以保留虚构内容，但必须保持身份，不进入当前权利、债务、功劳或状态判断。

准备一个实际待稿，或明确标识的[三组离线样例](world-narrative-content-delivery.design.md#narrative-composition-samples)。先标出其中每项可检验断言：谁、发生什么、是否已确认、何时、属于谁、由什么导致。没有某项断言时无需补齐字段；存在时必须能指回实际有权读取的来源。

| 场景 | 核对步骤 | 判据 |
| --- | --- | --- |
| F1：正常环境读取，对应样例 A | 只给合法 snapshot 类型和 ready Feed，无原操作/个人 receipt。逐句与允许发布的输入对照 | 只说环境快照已记录，不造“这次行动”、个人进度、生产或功劳。类型 summary 的中文建议与当前源码已有文本分别标识 |
| F2：接受与当前等待 | 分别提供当前权威确认的 accepted/pending，及只有旧 accepted、当前结果不可读的输入 | 前者可以表达当前等待；后者保持未知。两者都不写成完成；旧 accepted 不证明尚未执行、尚未扣费或可以重发 |
| F3：个人结果与主体变化 | 用一个与原操作匹配的结果，再用仅同 owner、时间接近或仅切换当前焦点的反例替换输入 | 只有匹配原关联的结果使用对应主体与因果；他人/匿名事件不进入个人回执，玩家级无 Agent 的结果不补选中 Agent |
| F4：缺依据与校正，对应样例 C | 审错误稿“你和 agent-b 保护了聚落，危机解除”，只给历史 resolved/严重度 3，无私人因果和空间 anchor；再审校正文本 | 删除合作、地点、行为和功劳，只保留来源能证明的危机状态。无因果证据不能改写为“确认与你无关” |
| F5：显式虚构或示意 | 将同一段虚构文字分别放进标识清楚的示意稿和声称真实运行的摘要 | 前者可作为内容讨论，后者因无真实来源不通过。不能通过删除示意标签把稿件变成证据 |
| F6：关系和债务 | 只给两个 Agent 同地出现，或一条危机超时记录；检查是否被扩写为伙伴关系、救援承诺或欠款 | 同地不证明关系，超时不证明任何主体承担新债务。删除无依据断言，或回到真正拥有该事实的既有专业来源 |

对普通 World Feed，读取 [world_feed.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/world_feed.rs) 的 `is_public_event/project_event` 与 [world_feed_source.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/world_feed_source.rs)：核 summary/detail 的实际公开范围、原 `receipt_ref` 和受限来源。不要从原始私有载荷或 generic event text 猜角色、地点和数量。危机还须核对 canonical 状态、freshness 和允许披露范围，不能只看到类型名字就拼故事。

校正采用“保留已证事实、删去未经证明的断言、说明还缺什么”的方法。实际世界数据有冲突时转原 source/Runtime owner；文本误归因时修原编辑源；公开正文已发布时按原 Site 纠正合同处理。修正文案不改动 journal、receipt、身份或世界状态，也不补造原稿的证据。

静态审校保存被审文本、输入范围、判定及校正差异即可。实际消费者的主体绑定另外沿 [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)，用同候选 A 操作切 B、缺绑定、玩家级结果、他人/匿名输入和 pending/unknown 等真实适用场景；纸面 F1–F6 不证明服务器隔离或运行因果。

<a id="narrative-knowledge-check"></a>
## 3. NARRATIVE-KNOWLEDGE：时效、可见范围与环境状态

输入为[知识边界文案](world-narrative-content-delivery.design.md#narrative-knowledge-boundaries)、[Feed 六态及危机文本](world-narrative-content-delivery.design.md#narrative-viewer-content)，和当前消费者的合法状态。逐项核对 current、last-known、unknown、conflict 与 Feed 的 ready/replay/gap/unavailable；它们不是同一组枚举，不能相互推定。

| 输入变化 | 内容必须如何变化 | 失败例 |
| --- | --- | --- |
| ready Feed 内有较早的 crisis transition | 保留该事件历史性质，只有其 authority 确认的最新 transition 才按 current 表达 | 因顶部“实时”标签把每条记录写成刚发生 |
| 当前观察变成仍可披露的最近已知 | 使用最近一次记录说明；当前风险/结果未证部分保留未知 | 刷新动画后继续宣称当前状态已确认 |
| 权限撤销或公共来源无法验证 | 按原范围移除不可披露内容，显示当前不可读的真实边界 | 将旧私有正文藏在 tooltip、摘要或原文复制中继续披露 |
| gap、reorg 或同身份载荷冲突 | 停止追加并按原 contract 重新读取；不选其中一条继续串接故事 | 保留旧事件为 current、按最后响应选择真值 |
| replay/historical resolved | 可读历史，不制造新的告警、庆祝、个人回执或世界动作 | 重播一次就又算一次解决/奖励 |
| 无空间 anchor、无合法个人因果 | 留在 ambient 文本，位置和个人功劳保持未确认 | 借最近 Agent 的位置放地图标记，或把事件归给当前焦点 |

核对中文与已有英文时直接读 [world_feed_panel.jsx](../../../crates/oasis7_viewer/software_safe_src/world_feed_panel.jsx) 的 `statusCopy/reasonCopy` 与 [pixel_world_presentation.js](../../../crates/oasis7_viewer/software_safe_src/pixel_world_presentation.js) 的 `pixelWorldMajorEventPresentation`。确认 active/resolved/timed_out、严重度和读回状态语义一致；新中文补充没有英文时记为未交付，不以相邻已有译文代签。

实际修改相关消费者时，选择 [world_feed_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/world_feed_panel.test.jsx) 的六态、空态说明、explicit receipt、anchored-less crisis、historical replay 与 permission loss 具名用例，以及 [world_feed_state.test.js](../../../crates/oasis7_viewer/software_safe_src/world_feed_state.test.js) 的冲突/epoch/schema 用例。现行入口为：

```bash
npm --prefix crates/oasis7_viewer run test:ui -- \
  software_safe_src/world_feed_panel.test.jsx \
  software_safe_src/world_feed_state.test.js
```

该入口是实施时的已有局部验证，文档变化不要求执行。测试定义与运行结果分别记录，fixture 不证明真实公共来源；实际接线仍按 [PW-FRESHNESS](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-freshness)取同候选 cut/来源、重验和世界结果。运行环境或权限不具备时记未执行，不降低判据。

<a id="narrative-dialogue-check"></a>
## 4. NARRATIVE-DIALOGUE：消息来源与配置分离

按[对话、草稿与配置要求](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002)及[设计文本](world-narrative-content-delivery.design.md#narrative-dialogue-content)，准备玩家消息、Agent 回复、回复失败、空正文、未知位置和尚未发送草稿六类输入。

1. 对照 [viewer_app.jsx](../../../crates/oasis7_viewer/software_safe_src/viewer_app.jsx) 的 `chatEntryTitle/Meta/Message`，核说话者、目标、正文和来源标签。错误反馈不显示为 Agent 成功回复；未知位置不从场景焦点补齐。
2. 切换界面语言，只检查外壳改变；原玩家消息、Prompt 和 Agent 自由文本不因语言变化被改成另一条意图或虚构译文。文案、复制和朗读使用相同可见范围。
3. 本地填入或编辑只表达草稿；目标提交、当前已确认目标和 Prompt 应用按原消费者区分。读 [canonical_agent_goal.jsx](../../../crates/oasis7_viewer/software_safe_src/canonical_agent_goal.jsx) 与 [canonical_agent_chat.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/canonical_agent_chat.rs)，确认“目标已提交，等待世界确认”没有被改写成执行完成。
4. 把一条 Agent 自由文本中的“我已经完成了”作为审校反例。消息可以保留原话，但外壳不得据此生成世界成功、库存、奖励或个人 receipt；实际结果必须另有原关联。
5. 若改了实际消息消费者且需要真实对话验收，沿 [PWT-001 原入口](../../testing/manual/web-ui-playwright-closure-manual.manual.md#当前入口)使用该任务适用的真实 UI/provider 链。保留发送前历史边界、目标、请求/反馈及新回复依据；当前手册已指出原脚本没有完整 request/correlation oracle，命中旧回复或 ACK 不能单独证明本次输入导致新回复。不能修改测试事实补足结论。

静态内容审校不启动模型、不发送消息，也不证明 Agent 个性、长期记忆、人物关系或持续目标已应用。来源标签不正确交 Viewer 修复，目标/鉴权/持久化问题回原专业 owner，不能通过替 Agent 编台词隐藏失败。

<a id="narrative-site-check"></a>
## 5. NARRATIVE-SITE：世界介绍、事实案例与小说

输入为实际 [中文首页](../../../site/index.html)、[英文首页](../../../site/en/index.html)、本次引用的案例详情及 [story 入口](../../../site/story/README.md)。顺序读 `#hero`、`#proof`、`#world-laws`、`#lore`，核对下表。

| 内容 | 静态检查 | 采用后的追加验证 |
| --- | --- | --- |
| 世界介绍 | 游戏类型、玩家/Agent 分工与资源压力保持同义；下一个入口真实存在；预览限制不因叙事压缩消失 | [AC-SITE-DISCOVERY-001](../../product/player-entry-distribution/public-site-discovery-and-showcase.prd.md#ac-site-discovery-001)的陌生访客理解 |
| 四步案例 | 目标、尝试/拒绝、实际调整和结果分别回到固定来源；保留日期、版本、准备条件与未覆盖范围 | 按 [Site 外部浏览器验证](../../site/github-pages/public-site-discovery-validation.manual.md#外部浏览器验证)核实际 CN/EN 页面与内容切换；浏览不产生世界写请求 |
| 详情、摘要与 RSS | 同语言 id/标题/类型/日期/状态/来源/链接一致；双语不把一份旧结论继续公开 | 沿 [Site 发布与纠正](../../site/github-pages/public-site-discovery-validation.manual.md#内容发布与纠正)检查实际加载与必要纠正；不承诺清除第三方缓存 |
| 概念与历史界面 | 图片和说明保留各自类型、日期、版本；不与邻接案例拼为同次运行 | 实际页面的标签可读、未被折叠或裁切隐藏；静态图不证明当前实机 |
| 小说 | 保留“世界观小说/中文”、真实 reader 入口和具身边界；不把背景中的剧透迁入首访导语 | 原 Site 方法覆盖中文 reader 的正常、empty、error、重试和快速切章；无英文正文不造英文路由 |

正文来源不足或版本不符时收窄相关陈述，不能用新示意补成真实记录。小说事实的正确性由 V50 原正文/配套资料判断；本范围只检导读身份和采用边界，不以未读全书断言小说品质通过。

现有 `site-link-check.sh`、`site-homepage-claim-check.sh` 等入口由上述 Site 手册选择；它们是实际 Site 变更时的结构检查，不能证明素材真实、当前线上已更新或玩家理解。只改本设计/手册时做链接和源定位即可。

<a id="narrative-handoff-check"></a>
## 6. NARRATIVE-HANDOFF：实际文本接入与变更传播

按[交付索引](world-narrative-content-delivery.design.md#narrative-delivery-index)和[制作交接](world-narrative-content-delivery.design.md#narrative-production-handoff)，在原 PR 直接核以下事项，不另造通用台账：

1. 接收方能取得准确的文本版本、中文/英文实际状态、上下文、条件、原编辑源和消费者。建议文字、源码已有文字、自由文本以及小说分别标明；这些身份不能在复制时丢失。
2. 修改发生在真实源。Viewer 的 JSX/JS 经 [package.json](../../../crates/oasis7_viewer/package.json)既有 `build:viewer`、[Vite](../../../crates/oasis7_viewer/vite.software-safe.config.mjs)和[finalize](../../../crates/oasis7_viewer/scripts/finalize-software-safe-build.mjs)生成实际 bundle/dist；该管线包含原编译依赖，不能把仅手改 bundle 记为源码交付。静态文档审校不执行构建。
3. Site 改动同时核应受影响的译文、详情/摘要/RSS；Viewer 状态文案核对应组件和具名测试；Runtime 的公开事实变化由其 owner 核 DTO/可见性与全部直接摘要消费者。无关文件不为凑数量修改。
4. 同候选实际验证记录程序/文本源、实际产物与加载证据、语言、入口和环境；游戏文本还记录 world/profile/读取范围及相应 authority。用当前页面展示历史案例时保留案例自己的旧版本，不把两个版本合成一个“当前实测”。
5. 来源/许可沿原内容入口和仓库 [LICENSE](../../../LICENSE)核对；外部 prose、字体、图像或角色材料只有取得实际授权与版本才进入交付。本次没有此类新增素材，不能据此宣称所有仓库内容权利均已审计。

具体 gap 说明变更应能从源句追到两语文案、同一恢复按钮、局部状态测试、实际加载文字和理解证据；若未集成，停在文本交付并保留未验证边界。新文字引入了旧源没有的世界事实时，回到 §2 修正，不以多跑浏览器弥补语义错误。

<a id="narrative-reader-check"></a>
## 7. NARRATIVE-READER：读者是否理解文本

静态样稿可先做内容审读，明确标为概念/样稿观察；不能计入真实入口验收。实际采用后，官网复用[Site 的陌生访客理解方法](../../site/github-pages/public-site-discovery-validation.manual.md#内容发布与纠正)，Viewer 复用 [PW-READABILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-readability)的固定场景、五秒首读、参与者与原始回答方法。沿原方法适用的样本和判据，不新增全游戏统一门槛。

主持人事先根据场景输入单独确定判据，不向参与者展示答案或提示归因，也不解释标题或替玩家补来源。先记录参与者独立首读的原话；如原方法允许后续提示，另行记录提示内容与提示后的回答，不覆盖首读记录。下表的“正确语义”仅供主持人核对，提问选择与本次实际改变相符的场景：

| 场景 | 提问 | 正确语义 |
| --- | --- | --- |
| 世界介绍 | “你负责什么，Agent 负责什么？当前能从哪里继续？” | 分工明确；文明之外不等于全知/直接控制，下一步对应实际入口 |
| 只有环境事件 | “发生了什么？能说明是你造成的吗？” | 只复述来源确认的环境事实；缺个人因果时不能归功于自己 |
| pending 或原结果未知 | “哪些已经确认，哪些还不知道？” | 当前等待与旧 accepted 后未知分开；不把未知当失败/未消费/安全重发 |
| 历史危机与切换观察对象 | “这是现在发生的吗？属于谁的结果？” | 历史回放不变当前事件，选中对象不补因果 |
| 消息与配置 | “这是一条回复、一份草稿，还是已经应用的持续目标？” | 按实际状态回答；Agent 的话不独立证明世界效果 |
| 小说入口 | “这里的故事能证明现在能操作什么吗？正文是什么语言？” | 知道是中文小说；具身与人物事件不变成当前游戏能力 |

记录原话、误解、独立回答和提示后回答，分别说明静态稿与实际候选。内容与布局共同影响结果时保存对应页面，不把主持人的补充解释回填为原文可理解。没有真实参与者就记未执行；作者自测、模型 persona、单测或截图均不代签理解。

<a id="narrative-production-check"></a>
## 8. NARRATIVE-PRODUCTION：制作规模与结果回写

需要估算更多内容时，选静态说明、带状态/插槽的文本、跨来源案例各自适用的实际样本；记录原编辑、译文、集成、检查、返工和等待的实际时间。初建流程与重复制作分开，未集成文本与上线内容分开；单个短句的成本不外推到人物对白、未知事件类型或整本小说。

本次没有生产吞吐、批量成本或返工率实测。未来结果仍写入原 PR/Task，不在本手册维护排期和完成百分比。缩减时优先保留已采用入口的必要来源、状态、归属与恢复说明；没有采用的事件或新语言不能先许诺规模再依赖作者填空。

最终结果说明：核对的候选与文本、所用方法/原入口、通过/失败/未执行、原证据位置、未覆盖范围。文案或来源变动只重验实际受影响的场景；旧证据只有在其内容、数据来源、候选和环境仍适用时复用，不把旧失败或未执行刷新为当前通过。
