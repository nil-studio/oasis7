# 像素世界可视化验证手册

- Owner role：`qa_engineer`；协作：viewer_engineer、game_visual_interaction_designer、runtime_engineer、agent_engineer及Launcher owner。
- 适用范围：世界舞台的主体、授权读模型、探索、图形恢复、可访问性、质量与Web/native同义验收。
- 要求入口：[世界舞台 PRD](../../product/agents-world-simulation/player-readable-world-stage.prd.md)。
- 技术入口：[大世界系统设计](../../world-simulator/viewer/external-agent-world-view.design.md)、[玩家可读渲染](../../world-simulator/viewer/viewer-pixel-world-player-readable-rendering.design.md)。
- 总策略：[testing-manual.md](../../../testing-manual.md)；原组合场景：[外部 Runtime 验证手册](external-agent-runtime-contract-validation.manual.md)。

本文是可重复执行的方法，不是运行报告。它补充原viewer-*场景的具体判据，不取消其完整义务，也不要求每个文档PR运行GPU、原生客户端、付费模型或真人测试。不存在的端点、fixture、安装包或测量能力记为未覆盖；不得用mock补出通过结果。

## 1. 执行准备与证据边界

每次记录完整source HEAD、实际build/manifest hash、测试/发行profile、API/projection版本、world/epoch/source cut、fixture版本和预期对象集合。图形运行另记录OS、CPU/GPU、内存、浏览器或WebView精确版本、viewport、framebuffer、DPR、语言/字体、网络限制及reduced-motion设置。正常WebGL2、GPU-disabled、软件raster、mock DOM和Rust派生分开标识。

只在本轮临时世界、测试账号和自有进程中注入故障。至少准备两个主体、A/B两个Agent、不同目标/操作和可对账的初始世界结果；不访问其他玩家数据、不清理其他会话、不修改线上世界。日志/截图/导出脱敏，不保存token、模型Key、私有prompt、观察正文或隐藏对象位置。

普通CI先执行纯数据、状态机、权限/并发和fake Context/operation合同；实际API/Runtime接线使用临时持久世界，真实GPU/S6和原生发行资产另验。真实模型、完整首局和L5仍遵守原测试策略及用户授权，不因验证Viewer而自动调用模型。

### 1.1 固定场景族

| 场景族 | 数据构成 | 必须固定的预期 |
| --- | --- | --- |
| F0 空与不可用 | 无snapshot、空授权集合、无gameplay摘要、过滤无结果各自独立。 | 不把加载/过滤/无权误写为空世界，不制造目标或进度。 |
| F1 稀疏 | Agent仅有location_id，无独立关系envelope；另有缺坐标/无可解析地点对象。 | 可以有location_derived显示位置，但零自动关系线；missing不填精确零坐标。 |
| F2 权威关系 | 有效kind/status/source_class/freshness及两端位置；逐项撤销或使其失效。 | 只在完整当前authority下有线；撤线不改变仍合法的派生位置。 |
| F3 多主体与原操作 | A操作在途、查看B、玩家级无Agent结果、其他owner结果。 | 观察/行动/回执身份独立，原操作不改、不重复。 |
| F4 多区域/密度 | 50000对象测试世界；单视点1000实体、200突出标签的边界及超限组合。 | 授权集合、聚合成员、关系端点、部分页与关键对象清单可对账。 |
| F5 异常与恢复 | 旧响应、断连、cursor失效、reorg、撤权、GPU启动/运行失败。 | 原操作/世界/权限各自恢复，不以图形健康推断业务状态。 |

fixture必须包含真实中文长名称、未知字段、重叠对象、无空间anchor事件和空/负例，而非只有理想截图。实现PR登记实际fixture/test函数与本手册场景ID，本文不虚构已存在的测试代码。

<a id="pw-identity"></a>
## 2. PW-IDENTITY：焦点、动作与回执归属

Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096。
对应[AC-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-001)与[AC-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-005)。

给定F3，分别在真实Rust host派生、host UI和实际受权API组合执行：

| 用例 | 操作 | 必须成立 |
| --- | --- | --- |
| 原操作A、当前选择B | 提交A，切B，再让A结果到达。 | 原receipt保持A/原operation；B详情没有伪造成功；不自动把camera拉回A。 |
| 推荐和列表变化 | 在A等待时推荐改为B，列表重排/过滤/增删。 | next-action和receipt不借用active_agent_id或首项补目标。 |
| 新Intent覆盖风险 | 已显示新目标B，再收到较早A的feedback。 | 旧feedback按原关联解释，不使用最新acceptedIntentTarget猜归属。 |
| 缺绑定 | 有结果文案但缺原操作/主体关系；另测无gameplay摘要。 | 无权威因果不显示Agent成功，无摘要不由计数生成目标/进度。 |
| 玩家级结果 | 当前玩家存在合法但无Agent目标的原结果。 | 保留玩家级和null目标，不绑定当前选中Agent。 |
| 其他owner/匿名结果 | 注入非当前主体的响应/反馈或过期授权。 | 不披露其私有内容，不进入本人的成功回执。 |
| 状态区别 | 草稿、pending、advanced、no-progress、blocked、rejected、unknown逐项输入。 | 草稿不当accepted；pending无完成或世界delta；unknown先回查原操作。 |
| 只读操作 | 平移、选择、跟随、过滤、图层、地形点击。 | 无新增世界action提交或世界副作用；合法高层指导单独验证。 |

保存原operation/主体/目标、selection前后、DTO和实际结果关联。固定RED负例必须能让旧first-agent/当前selection兜底失败；Rust与mock loader结果分别记录。不能仅凭按钮disabled断言服务端不会越权。

<a id="pw-freshness"></a>
## 3. PW-FRESHNESS：时效、并发与生效重验

Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096。
对应[AC-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-002)与[AC-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-005)。

将current、last-known、unknown、conflict、expired分别与只读、无副作用预览、owner目标保存/激活、适用审批和executor实际世界动作组合。owner操作固定主体/方案及expected revision；实际executor动作另固定原Context/fence。分别在预览前、预览后提交前、各自权威接受边界和commit后reply前注入变化。

必须验证：

- 旧请求最后到达不覆盖新视点/对象/权限；几何来源不等于时效；互相冲突的当前观察不被客户端“最新响应”选成真值；陈旧信息最多支撑明确标注的预览。
- owner目标保存/激活由原owner API重验权限及task/goal版本；补测executor未绑定或离线时合法激活仍成功并显示awaiting_executor，不将executor Context作为前置。审批单独按其身份、scope、版本及适用条件验证，不从离线目标激活推导任意审批可离线通过。
- 实际世界动作由原API/Runtime在事务边界重验执行主体、Context、控制/目标版本、fence及适用世界条件；转让、撤权或版本变化应拒绝或要求重新确认，不自动批准调整后的方案。

另测stop/撤销/原操作查询：它们按自己的服务器授权工作，不因GPU不可用或观察陈旧被统一阻断；真正撤权时仍拒绝。commit后响应丢失，必须保留原键和recovery_required，回查同一结果，不重新报价、换Agent或新建副作用。

world与task流各自使用cursor/binding；交换到达顺序不能形成伪原子状态。World Feed重复事件去重、同身份异内容拒绝、gap/reorg重新snapshot；accepted和环境事件不能成为world effect。

证据包含显示使用的cut/来源分类、原操作、重验前后控制版本、实际拒绝原因及前后世界结果。fake authority测试只证明客户端分支，真实Runtime接线必须另验。

<a id="pw-projection"></a>
## 4. PW-PROJECTION：空间读模型与Rust适配

Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3。
对应[AC-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-003)与[AC-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-002)。

| 边界 | 执行 | 判据 |
| --- | --- | --- |
| 同切点分页 | 200/201项、不同页返回顺序、页中途cut/audience改变。 | 只合并同cut同scope；未完成标partial，失配reset，不能混合后标complete。 |
| 字节容量 | 真实JSON编码2 MiB前后1字节，含CJK、转义、单个超大项。 | 按编码字节限制，单项不能截字段伪完整；错误或partial有原因。 |
| 删除与淘汰 | 对比明确tombstone、完整scope替换、viewport移出、过滤、LRU淘汰。 | 只有前两者在其权威范围说明删除；其余不是对象不存在。 |
| 世界尺度 | 相同对象分布在不同查询范围/页/缩放/搜索结果。 | world_bounds基准不随页包围盒变化；相同世界位置投影一致。 |
| 来源组合 | 旧权威坐标、当前地点派生、当前关系布局、missing。 | 几何与时效分别保留；旧renderer无法表达时降级文本，不标current精确。 |
| 精度与校验 | 大于2^53的tick/revision、无效/非有限坐标、未知字段、错误签名/绑定。 | 精确身份不经Number丢失；先验完整签名投影再显示裁剪；未知坐标不填0。 |
| 私有范围 | 两账号/两世界同entity ID，撤权时有在途详情/hover/搜索响应。 | cache隔离；先清受限内容再初始化；迟到数据不重新出现。 |
| 关系与事件 | F1→F2→撤销；事件有/无anchor。 | 无authority不造线；无anchor仅Feed，不能借最近Agent位置。 |
| terrain | 缺bounds/fragment、block重排/撤销、world切换及DPR/zoom组合。 | 当前Rust派生正确，旧色块回收，index键不作为持久业务身份，无block命中。 |

坐标/LOD fixture固定世界角点、中心、有效边界、CSS/framebuffer/DPR及当前camera，直接比对现有投影与hit region；不引入另一份测试专用生产算法。地形完整AC-5仍按[Fragment PRD](../../world-simulator/viewer/viewer-pixel-world-fragment-lod.prd.md#6-acceptance-criteria)执行，本文不能以适配测试替代Bevy像素和browser smoke。

<a id="pw-exploration"></a>
## 5. PW-EXPLORATION：完整观察旅程

Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf。
对应[AC-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-003)。

用F1/F4执行“总览→区域/Fragment→授权搜索→定位/选择→检查→合法跟随→退出→返回当前目标”。鼠标、单指针不拖拽、键盘与受支持触控分别执行，不用测试API代替release UI。

检查搜索能区分当前Targets过滤与授权空间搜索；重叠对象通过稳定候选列表选中正确身份；聚合只下钻不授予批量控制；partial/过滤/加载失败不被表述为空世界。目标无可信位置时仍有文本上下文和返回路径。

跟随后手动平移应退出跟随，后续数据不抢回视点；可信位置失效则暂停，恢复位置后需明确重新跟随。退出跟随与既有drawer/Focus Escape顺序不冲突。切选时旧详情晚到不覆盖新对象；图层/过滤不改任务或原回执。事件缺anchor时无marker/highlight，仍可在Feed检查。

保存操作序列、选择身份、范围/camera/来源和实际可访问反馈；能拖地图但不能可靠找到目标不算通过。

<a id="pw-recovery"></a>
## 6. PW-RECOVERY：图形、连接、权限与原操作恢复

Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3。
对应[AC-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-004)及[AC-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-002)。

必须分开执行启动无GPU、optional payload缺失/损坏/hash不匹配、Rust派生失败，以及真正ready后context loss/fatal。不能用GPU-disabled截图代签ready后恢复。

在隔离测试构建中可用[WEBGL_lose_context规范](https://registry.khronos.org/webgl/extensions/WEBGL_lose_context/)注入真实context loss；按事件确认lost和restored，恢复调用返回不是恢复完成。测试驱动不得替应用实现恢复逻辑；生产release不增加测试后门。扩展不可用时注明限制并使用该平台受支持故障方法，不编造已注入。

每个失败点核验：旧canvas命中和动画停止；仍获准的目标、Targets、原回执/原操作回查与安全控制可达；旧帧不标实时；无第二JS世界。恢复必须重新应用当前授权视图、camera和合法selection，并验证命中正确、无重复实体/订阅、无重复world action。

在恢复期间撤权/换账号、切world或让旧加载promise返回，旧私有图形/详情不得复现。断连或cursor/reorg恢复不能把已确认结果重置成普通pending而重做；原结果未知只能查原operation。只允许有界自动图形重试，失败后显式重试/文本入口，禁止无限reload。

原生窗口关闭只清本视图资源，不杀独立worker或其他进程。重开窗口使用新授权确认当前内容，不借旧缓存扩大权限。

<a id="pw-accessibility"></a>
## 7. PW-ACCESSIBILITY：语言、密度与输入方式

Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf。
对应[AC-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-001)及[AC-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-003)。

至少覆盖1440×1000桌面、768×1024平板、390×844窄屏及844×390低高度；受支持最窄宽度另测。DPR1/2、zh/en、长CJK名称/阻塞说明、200%文本放大、reduced motion与F0/F3/F4分别取代表组合。不是要求盲目全笛卡尔积：正常核心旅程必须覆盖每种声明支持的表面，高风险组合按变更命中追加并写明未覆盖。

通过条件：世界/目标/下一步仍先读；关键操作没有遮挡/裁切/水平溢出；选中/来源/冲突/回执不只靠颜色或动画；tooltip触控可读并可关闭；重叠对象可通过候选列表可靠选中。键盘可进入/退出画布及详情，有可见焦点和invoker恢复；单指针不拖拽可完成等价浏览，不靠hover唯一传递信息。

IME合成期间Escape不关闭页面表面；非合成时先最内层drawer再Focus。聊天/文本输入不被画布快捷键截获。数据刷新不抢焦点，不逐实体洪泛辅助播报，原结果不重复宣布为新成功。对象名称按文本处理，含HTML样式输入不能变为脚本/原始HTML执行。

原生CJK字形回归复用 `npm --prefix crates/oasis7_viewer run test:pixel-world:cjk:visual`，必须显式提供 `AGENT_CJK_ARTIFACT_ROOT`（不可变产物目录）与 `AGENT_CJK_SOURCE_HEAD`（实际构建提交），前提为 agent-browser、Python/Pillow。脚本先以实际Agent atlas确认已绘制，再以相同中文A/A像素不变、不同中文A/B像素有差的成对控制检查native label，保存长中文/Latin数字标点及真实context loss→Reload后的同一检查。检查前后产物SHA须不变；桌面、窄屏、DPR2逐一记录实际尺寸。该受控输入仅证明所采字形/输入与恢复，不代签全部Unicode、真实授权或原operation身份。`AGENT_CJK_MODE=cold-observation`分别记录bridge ready与实际atlas可见时点，只是单次观察；正式冷/热启动样本、网络和缓存方法仍按§9执行。

记录实际键盘/触控操作与辅助语义，不用DOM节点存在代签真实可达；截图审读沿既有[S6](web-ui-agent-browser-closure-manual.manual.md)及[视觉评审SOP](model-visual-review-sop-2026-05-29.manual.md)，不新建另一套视觉放行机制。

<a id="pw-readability"></a>
## 8. PW-READABILITY：五秒首读与因果理解

Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616。
对应[AC-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-001)。

从实际候选已取得相应授权状态、完成首屏可操作呈现后开始计时；加载时间单独进入性能测试。固定场景、密度、语言及状态，不在计时中由主持人解释界面。

给出5秒观察窗口，再让参与者回答：当前目的是什么、主要blocker是什么、下一步能做什么。另在A操作/选择B、只有ambient事件、无回执、pending、无进展/拒绝和信息冲突场景询问：发生了谁的什么结果，哪些还未确认。允许答案同义，但预先写出正确主体/状态/下一步，不以主持人事后解释判通过。

首轮小样本至少5位符合目标用户特征且未预读该fixture的人；各核心题至少4/5正确，信任负例不得出现把他人成果/ambient/pending当本人成果的误归因。失败按原始答案定位内容/布局问题；这些是初始体验验收目标，不是统计显著性、留存或完整L5证据。缺真人测试时记未执行，不把模型persona或作者自测替代真人理解。

模型视觉审读可检查层级/裁切等，但继续按既有SOP判定；真实理解、独立评审、发行和公开宣传分别负责，不由一次pass+high一并代签。

<a id="pw-performance"></a>
## 9. PW-PERFORMANCE：负载、冷启动与长稳

Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616。
对应[AC-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-003)与[AC-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-001)。

数值预算以[系统设计view-quality](../../world-simulator/viewer/external-agent-world-view.design.md#view-quality)为唯一来源，本文定义测法，不维护第二份阈值。基准A/B、设备、浏览器、电源和网络限制必须实际记录，不用“普通电脑”作为可比较身份。

### 9.1 测量窗口

- 启动：每组合至少20次冷启动、30次热启动，按实际样本报告P50/P95/max和原始时间；不足样本只作为探查，不宣称稳定P95。冷启动清本测试隔离profile的HTTP/WASM缓存，热启动保持同版本资源；认证等待另计，不悄悄从renderer时间中扣下载。
- 分段：分别记录页面核心UI可操作、optional payload传输字节/耗时、WASM解码/编译、snapshot读取/解码、Rust派生、reconcile、首个真实可命中画面。占位图/DOM不算真实世界帧。
- 持续交互：在F4里连续平移/缩放、选择、退出跟随和事件突发至少5分钟，记录可见帧间隔、输入反馈和query。静止的按需不渲染、后台节流和Runtime完成时间分别报告。
- 长稳：同一已稳定负载执行30分钟区域往返/选择/过滤与恢复。比较预热后相同阶段的初/末窗口，记录JS、WASM、应用缓存及可获得的GPU/进程资源；无法测量的分量标unknown，不用JS heap代替全部内存。

### 浏览器缓存隔离能力前置（5772）

`VIEWER_STARTUP_OUTPUT=<不存在的绝对输出目录> npm --prefix crates/oasis7_viewer run test:startup:cache-capability` 只执行三组短能力检查，不采产品启动预算：新空 owned profile A／新 Chrome 进程、同 profile A／新 Chrome 进程、独立空 profile B／新 Chrome 进程。复用现有 Playwright 依赖，可用 `OASIS7_PLAYWRIGHT_NODE_MODULES` 和 `OASIS7_PLAYWRIGHT_CHROME_PATH` 显式指定现有依赖与浏览器；不自动下载。sandbox 必须启用，工具只启动自己的 profile，不清用户或 OS 缓存。缓存资源是固定8字节合法 WASM，仅验证 HTTP/profile 方法，不能代表产品 payload 编译成本。

保存服务端实际资源请求数与 CDP `fromDiskCache`／service-worker 状态；冷样本要求真实网络请求，热样本要求同 origin/profile 的磁盘复用。只读本人启动的 browser 命令行核对精确 profile 后取得 native PID，关闭后必须确认 PID 已退出；失败、清理未知或120秒总硬停均非零，不进入下一 case。保留 owned profile、原始结果与脚本 SHA，不强杀不明进程。HTTP cache 不证明 WASM compiled-code cache；OS page cache 未隔离。该能力通过后，产品核心 UI／bridge ready／Atlas 已可见上界／实际命中端点仍须单独建立，之后才能按上文20冷／30热执行。M3/16GiB 本机结果不代签正式8GiB网络 A/B、P5或P6。

### 自有浏览器 RSS 能力前置（5773）

`VIEWER_RESOURCE_OUTPUT=<不存在的绝对输出目录> node crates/oasis7_viewer/scripts/viewer-owned-resource-capability.mjs` 复用上述唯一 owned-profile factory、现有 Playwright 和已安装 Chrome，启用 sandbox，最多三短组且总界120秒。先以精确 profile 参数及本 context 的 CDP native browser PID 建立归属，只读取 PID/PPID/birth 身份表以求后代，再对已证根与子 PID 读取 RSS；前后身份或后代集合变化、CDP 出现外部 PID、页面归属变化均拒绝。额外自有页面负控须在 RSS 读取前拒绝，关闭后 root PID 退出未知则不能启动后继探针。保留 private profile 和脚本 SHA，不读取用户进程内存、不修改系统权限。

空页与受控两张1024×1024 RGBA8纹理场景仅报告各 owned 进程 RSS 和可能重复共享页的原始和；8MiB 是请求存储字节，不是实际 GPU 分配。GPU 进程 RSS 不等于显存；当前 CDP ProcessInfo 的 PID/type/CPU 时间不能提供总显存，GPU allocated bytes、完整应用缓存及物理呈现继续 unknown。该短能力结果不是产品稳定负载或64MiB/10%预算验收。

### 局部工具证据边界

性能探查先检查 canonical `viewer_terminal_shell.css`：响应成功且 MIME 为 `text/css`、浏览器实际加载有规则的 stylesheet、非活动 renderer hit-only target 的背景图为 none、背景与边框透明、阴影为 none。该前置保存为 `canonical-style-prerequisite.json`；不满足时判入口无效并停止，不将缺样式造成的额外绘制耗时写作生产预算违约，也不让后续阈值通过覆盖无效入口。前置通过只证明测量入口正确，不能代签性能预算通过。

`node crates/oasis7_viewer/scripts/viewer-performance-probe.mjs --agents 1000 --interaction-samples 20 --max-interaction-p95-ms 100` 使用 gated visual fixture 的正常身份对齐，再将本项受控 Agent 绑定到该身份；不建立真实服务授权。原生 Enter 的可信 keydown 时间到目标 `data-selected` 更新及两次 rAF 作为选择反馈探查，逐样本记录身份、时间和真实输入。两次 rAF 只是绘制机会，不等于屏幕光子或世界 Runtime 完成；未覆盖平移、缩放、stop、五分钟和正式 A/B。输入为空、null 或无目标时必须失败，不能用零延迟补样本。`actualProjection` 的 DTO 与 native target 数单独报告，不能从 `--agents` 的源对象数量推断可见密度。rAF 间隔也不能单独证明世界实际渲染 FPS。

扩展交互测量使用 `--paint-trace` 分开保留三种终点：可信输入后 `data-selected` 首次变true的观察时间（末端core选中身份另核）、同目标/时间关联且带 interaction ID 的 browser EventTiming next-rendering estimate、原有 DOM 后两次 rAF 绘制机会。DOM 接受不是可见绘制；EventTiming 也不是世界 GPU fence 或 Runtime 结果。[W3C Event Timing](https://www.w3.org/TR/2026/WD-event-timing-20260223/) 的事件暴露下限为16ms、duration以8ms粒度呈现，因此缺条目记 unknown、不补零；保留±4ms区间，并核对选中变化不晚于关联绘制区间下界（中心−4ms），且直到区间上界（中心+4ms）未撤销。不选取较快的一组替换旧门槛或代签正式P6。selected DOM观测按每批回调的最终connected节点真值记录状态变化，覆盖属性更新、预置选中节点插入、移除与身份替换；不推断未观测的中间真值。保留DOM记录数、活动索引峰值和回调耗时，成本不从延迟扣除；索引/trace仍有界且溢出失败。PerformanceEventTiming若未暴露同目标，paint继续unknown，不以时间戳替代身份，不用pending ACK代替selected，也不事后补写旧raw。

`--interaction-window-ms 300000 --mixed-camera --snapshot-feed` 使首个可信输入到最后反馈的实际浏览器时间跨越至少五分钟，期间使用原生pan/zoom与受控snapshot的2Hz/有限10Hz更新；snapshot更新不是WorldFeed事件，未协商的follow/stop/query不得用QA的Stop updates冒充。用独立窗口遵守适用[发布性能合同](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#viewer-performance-verification)的重复要求，逐窗报告与保留最差一次，不以平均数掩盖失败。`--compact-scene --agents 476 --locations 24 --min-viewport-markers 500` 只用于既有500 marker边界，逐段对账真实投影与屏内DOM几何，不旁路256KiB/500 marker或100 label的硬上限。100 label是上限而非实测负载/可读保证；没有原生实际label计数时标unknown。此500子集不覆盖[独立worldView显示profile](../../world-simulator/viewer/external-agent-world-view.design.md#world-view-render-density-profile)的1000/200，不能把旧入口上限或本工具结果当作正式负载验收。

worldView密度回归用 `DENSITY_QA_ARTIFACT_ROOT=<immutable-dist> DENSITY_QA_SOURCE_HEAD=<actual-built-commit> node crates/oasis7_viewer/scripts/pixel-world-world-view-density-visual.mjs`，需要仓库固定的ws开发依赖、agent-browser与Python/Pillow。owned loopback WS仅作为受控只读peer，借现有runtime config/hello协商与标准request_world_view五页200项取得1000记录；snapshot基底无对象，不注入worldView或修改客户端cap/权限。它不证明signed WorldService、真实玩家授权、operation身份或P5。该helper是受控observer/debug技术探针，显式记录既有trusted_local_only技术配置与owned density-qa.localhost、authReady=false及无playerId/publicKey/bound Agent、无identity请求；不将已退出普通用户流程的模式重新暴露给玩家。正常hosted_public_join登录门禁保留，其未登录拒绝是预期；技术场景不代签正常玩家认证后的端到端体验。

只读 `readPixelWorldDisplayMetrics()` 限test_api=1，从已挂载实例读取PostUpdate布局/可见性统计，不初始化第二实例。只采 `observed` 且render_version/source_cut与同次capture前后一致的帧；pending不借旧数。分别记录projected marker、独立查询Sprite组件得到的实际Agent body/相机内body数、budget visible、in-view与有glyph布局数、实际CSS/F/DPR及≤200诊断bbox；frame_entity只稳定于该帧，bbox不是GPU fence或遮挡证明。以公开名称normal→零宽名称负控→normal恢复，保持相机/几何等非名称输入相同，逐bbox比较真实PNG；不能只看到配置label_cap=200就签200个已绘字形。保留partial页、mixed-cut、删除、旧回应、新scope/拒绝、能力移除时清理、真实context loss/显式Reload的实际结果。缺新scope证明的legacy回切只可称清理/锁定，不以QA直接授权冒充恢复。


#5605 的提示布局回归可执行 `PENDING_LAYOUT_ARTIFACT_ROOT=<冻结visual目录> PENDING_LAYOUT_SOURCE_HEAD=<实际构建源> node crates/oasis7_viewer/scripts/pixel-world-pending-status-layout.mjs`。此项在真实shell中复制Selected A和pending/unavailable状态，覆盖桌面/窄屏与Feed开闭，比较透明文字负控的真实像素、文本边界、可信指针穿透及Canvas CSS/backing尺寸不变；产物SHA前后核对。它是明确的DOM-only布局验证，不连接WS，不证明pending状态机、获准后页选择、玩家授权或性能；逻辑另跑 `pixel_world_pending_selection*.test.js`，真实标准WS/原生输入与性能沿同一固定生产组合另验，不能把pending冒充selected。

同一冻结组合可设 `DENSITY_QA_SOURCE_HEAD=<实际生产源> DENSITY_QA_ARTIFACT_ROOT=<冻结目录> DENSITY_QA_SCOPE=baseline DENSITY_QA_PENDING=1 DENSITY_QA_PENDING_CASE=confirm node crates/oasis7_viewer/scripts/pixel-world-world-view-density-visual.mjs` 运行真实WS挂页探针；可选case为 `confirm|replace|absence|scope`，输入为 `DENSITY_QA_PENDING_INPUT=keyboard|mouse`。组合产物另传 `DENSITY_QA_COMPOSED_MANIFEST=<清单>` 并核每文件SHA，不把复用Rust产物称为新canonical build。工具只在实际观察到1000场景/目标附着后，用正常rAF边界对齐响应、收到第三请求的协议因果和owned CDP可信输入记录真实partial→旧呈现窄窗；不暂停rAF/renderer、不改DTO、不重试失败输入。未命中窗口须非零退出并保留unmeasured，不能把支持scope分支等同已测。`absence`为新cut旧cursor拒绝与完整授权投影缺失，不把普通Agent省略称为world_deleted；预算与正常玩家P5另验。

剩余取消接缝可另运行有限夹具：`SCOPE_PENDING_ARTIFACT_ROOT=<冻结visual目录> SCOPE_PENDING_SOURCE_HEAD=<实际构建源> SCOPE_PENDING_COMPOSED_MANIFEST=<组合清单> node crates/oasis7_viewer/scripts/scope-pending-visual.mjs`。仅协商world_view/pins，关闭delta轮询，以A/C/B三实体每响应一条分页（客户端请求上限仍200）保留真实Host/WS/WASM；收到第三请求后只发一次可信输入，必须实际看到pending且B未selected。默认在同connection、同request_generation上返回标准scope_unavailable，验证可见unavailable及原已准备晚页不得复活B，随后普通拒绝连接清理/恢复另记。独立设置`SCOPE_PENDING_RESET_CASE=1`，只验证pending期间连接代际重置和旧包拒绝，不把它改称authorization-epoch撤销。窄窗未命中须非零保存unmeasured并停止；不得冻结renderer/rAF、重试输入、造身份或grant。该有限夹具不签1000/200、真实授权撤销、权威删除、P5或P6，不覆盖或改标原1k探针miss及五分钟原始结果。

worldView交互窗口在上述技术入口增加 `DENSITY_QA_SCOPE=baseline DENSITY_QA_PERFORMANCE_MS=300000`，用 `world-view-density-performance.mjs` 复用严格 DOM/EventTiming matcher。选择只观察现有 renderer target 的 `data-selected`，不把空间检查写成业务 core 选择；默认旧 sampler 仍使用 core 身份。renderer测量与挂页探针通过唯一匹配owned页面的CDP发送可信Enter，仅使用key/code与标准Windows虚拟码，不指定平台nativeVirtualKeyCode；默认core入口仍用原CLI。失败记录armed状态、实际焦点与同目标是否仍附着，不换目标重试，也不把输入未送达记为已选中延迟。每个真实 keydown 同时记录只读 metrics 及其观测耗时，分别保留确认1000/200、pending未知与真实partial输入，全部输入和确认密度子集的分布各自报告；不从延迟扣观测耗时，不把未知补零或静默丢样本。250ms密度监测、原生pan/zoom的实际相机变化、标准WS完整分页及获准资源变更另外落盘。原生滚轮采用实际固定倍率，越过本fixture的合法密度区间前只经真实World overview恢复并记录该动作，不修改相机DTO、producer LOD或目标数量。

预算失败、DOM/paint未知或无法证明全部输入密度时，工具在保留raw后非零退出。输入或采集失败时，先保存原始错误、performance-window、可取得的monitor和完整paint trace，再尝试评分；评分异常单列scoringError，不覆盖原输入错误。overflow仍失败且不清标记，不过滤异常条目后重新签通过；未取得的采集项明确记录collectionErrors，不补空数据冒充完整。EventTiming量化区间跨越主责阈值时另列不确定性，不据中心估计宣称确定的真实显示超时。`DENSITY_QA_PERFORMANCE_TRACE=1` 仅开启有界Chrome DevTools trace根因诊断，其开销与未开trace的预算窗口分开；rAF机会、长任务和trace都不单独证明世界GPU提交/物理呈现帧率。分页中的partial或pending使全程恒定1000/200尚未证明，不能因窗口跨越五分钟就签完整P6。

长期分量探针复用同一技术入口：`DENSITY_QA_SCOPE=baseline DENSITY_QA_MEMORY_MS=1800000`（短方法校准可先设1），与performance/pending/frame模式互斥。固定构建/runner/18资产与可选组合清单后，先三周期预热，再执行可信键盘选择、6CSS像素有界视口往返/滚轮（不称广域多区探索覆盖）、真实搜索10条与清空恢复1000条；相机变化须有实际WS查询bounds对应，不写DTO。每周期至少等待10s，流式保存数字分量与原始wire，内存中只留初/末各6个同home阶段样本；本地实例/范围序号、当前query与cut比对、页面timeOrigin和相机必须匹配，不在末端GC/清缓存/Reload。客户端统计的逻辑UTF-8编码/唯一对象不是heap或全缓存；quiescent采样不证明分页峰值及64MiB总缓存上限。JS heap/DOM读取前后用便宜stamp括定；同scope/cut正常poll跨读保存全部原值并标未匹配，该窗口对应内存分量unknown，不重采/挑低值。身份、实例/范围/页面或实际负载前置丢失则保存后停止。

采集容量前瞻注册为固定512MiB raw wire上限：先前四周期约11.28MB、计量周期14.457s，30分钟外推约361MB；按10s最低等待的粗上界接近516MB，仍可能触顶，触顶时保跨界原行后terminal拒绝新source，不丢行续跑或临时放宽。该容量不是产品64MiB/10%预算。JSONL数字、原始协议、实际写入字节与错误分栏；采集IO/阶段原错优先于cleanup或summary写盘失败。只有至少30分钟、初末各6且不重叠才比较已知内存分量10%增长；逻辑字节/对象与JS保留容量仅趋势。未知WASM线性/renderer/GPU/owned RSS不补零，CLI 0仅表示方法完成且已知分量无已证违反，不能签整体P6、正式8GiB A/B或真实P5授权。详见[只读保留观测](../../world-simulator/viewer/external-agent-world-view.design.md#view-retention-observation)。

测量必须显式声明实际构建 `AGENT_VISUAL_SOURCE_HEAD` 和冻结的 `AGENT_VISUAL_ARTIFACT_ROOT`，保存全部产物与runner内容hash、tracked dirty状态、真实设备/电源/viewport/DPR和采样窗口。移除QA专用计数面板及其周期读数时用 `--dispose-fixture-controls` 明示，不能删除产品HUD/世界目标或裁剪权限。浏览器rAF仍只作为heartbeat观察，不能单独判世界实际帧预算；16GiB/headless/loopback子集不代签8GiB A/B、物理显示、50k query或30分钟长稳。

组合材料恢复可运行 `AGENT_VISUAL_SOURCE_HEAD=<固定构建源> MATERIAL_VISUAL_SCOPE=recovery node crates/oasis7_viewer/scripts/agent-browser-material-visual-test.mjs`。本地 HTTP QA shim 只包装原 gated `material_shells` 工厂，在每次真正页面 Reload 前提供当前受控 known/removed 输入；不保存或恢复私有 DTO，不新增生产权限 API。三个视口分别真实触发 context loss，检查失效目标清空、无自动 Reload、原生 Enter 显式重载、非空 fixture 身份及 receipt target 连续、无世界 Command 或变更 HTTP。PNG oracle 检查工厂全部源色与删除负对照，Depot 避开既有 Location 右下角框的源区域仍覆盖全部八色；矿纹使用结构化侧带检查组合画面恢复，独立矿纹消费须另用同底色 runtime oracle。它们不代签真实服务撤权、原 operation 身份或正式 P6 预算。

<a id="pw-local-feedback-measurement"></a>
### 本地接受与最终确认的前置测量计划

权威端点和预算沿[WorldView本地反馈](../../world-simulator/viewer/external-agent-world-view.design.md#view-local-feedback-measurement)及[9.2质量预算](../../world-simulator/viewer/external-agent-world-view.design.md#view-quality)。以下是待实现/验证的方法，不声明当前helper已取得完整ACK证据；旧strict selected列、阈值和unknown继续保留，旧466样本不重新标记。通用发行profile与WorldView专项分别报告。

1. **采样前注册。** 固定生产/runner/观测脚本及资产hash、设备/浏览器/电源/viewport/DPR、schema/profile、输入集合、端点、字形校准和失败判据。每个计划输入保留，明确区分本地接受状态、可见ACK、selected最终确认、网络/Runtime完成；不得事后挑较快的一列替换原结果。错误目标/无可信输入/无新反馈/代际不符/时间或可见性缺证据均有独立unknown原因，不填零。
2. **真实关联。** 保存实际native输入的ref、timeStamp/timeOrigin及同world/scope的显式用户意图代。反馈所有者需提供同一次意图的只读关联，不能由QA写入或从邻近时间猜测。现有泛pending文本/role=status不足以绑定输入；连续旧pending不因不可见token更新就算新的可见响应。字段缺失时只报告状态观察，不开始完整ACK预算计分。新增观测不保存旧实体/权限事实、不发pin或命令，不暴露私有authority key。
3. **可见性校准。** 在实际shell中先覆盖桌面/窄屏、声明DPR、Feed开闭及遮挡/裁切反例。固定相同候选/字体/文本的完整语义字形区域；A/A稳定正控、透明文本和遮挡负控须能区分有效字形与背景变化，再冻结覆盖与噪声门槛。此校准在计时外单独留证；计时内只观察真实反馈，不复制节点、改文本/样式、隐藏HUD/目标或降低密度。DOM rect、aria-live和非零像素差本身不证明可见ACK。
4. **短能力核对先行。** 每轮至多三组预声明的短探查，先测直接selected、本查询partial的pending→后页确认、旧徽标/错误代际负控；错误目标、快速替换、换scope与未送达另有确定性负例。只用原有标准读查询，实际输入窄窗未命中即保存unmeasured停止，不重试求pass、不造grant、不冻renderer/rAF。先验证当前Chrome实际提供的图像、正常产生的合成帧关联和前后时钟锚；保存协议版本/trace字段，传播时钟误差到逐样本区间。不把Paint/DrawFrame或截图API返回当作呈现，不强制paint，不用缺帧证明此前无ACK；条件不足时可见时间unknown。trace开销与无trace预算窗口分开，不从时延扣除。
5. **分列判定。** 记录反馈类型、意图绑定、输入前基线、DOM状态时间、可见性检查/字形证据、帧与时钟区间、密度、撤销/更新状态及未知原因。图像必须对应同意图、同scope且状态持续的完整区间，不能跨代借图。新ACK列按主责上界/连续无响应判据评估；原selected DOM/EventTiming/额外rAF分布与原gate不动。ACK合格也不把target=null的selected unknown改成通过，不代签P6整体。

首轮实施限定为能力探查：由owned Chrome读取Browser版本和`Tracing.getCategories`，只使用实际支持的timeline/图像/UserTiming记录；每组trace预设短时限，前后以`performance.mark`及其调用时间区间建立可审计时钟锚。归档原始字段/图像及映射误差，不假定某个截图事件天然带可用呈现时间。若采集会主动推进帧，或只有稀疏截图而不能证明同帧/时钟关系，该数据只作诊断，ACK可见时延不计合格。既有pending布局helper只能供非计时校准，不能提供真实意图接受证据。

`DENSITY_QA_FRAME_OBSERVATION_MS=3000` 在既有冻结source/artifact、真实WS与1000/200字形前置后运行1–10秒能力探针；与performance、memory、pending模式互斥。工具session只有一个managed tab不证明整个Chrome独占：browser-wide只读枚举每次重新核对唯一owned page；额外newtab/其他页导致该项unavailable，不关闭未知页、不启动global trace。独立page-target screencast只接attached session的正常图像与原始swap metadata，保存page performance/timeOrigin/wall锚；600图/64 MiB上限，任何overflow、采集或清理错误保留且非零退出。稀疏图像不证明每帧覆盖或物理显示，缺world render-version→GPU完成→合成映射时帧预算仍unknown，未校准输入/字形可见性前不计ACK。图像/metadata保全不改变原selected oracle。

### 9.2 容量与过载反例

测试100/1000/1001可见实体、200/201突出labels、200/201单页、2 MiB边界和超额事件；世界总量固定50000但不每帧传全量。授权后再聚合，记录实际绘制数量、关系端点和cache。关键选中/目标/blocker在裁剪后仍可达，partial不伪装对象不存在。

弱网络、大JSON/长文本、密集同地点Agent和频繁查询取消下，输入/退出跟随/stop不能被数据洪泛阻塞。取消读请求不取消世界operation。CPU、内存、查询、下载和GPU瓶颈分开，不从实体池存在推导已提高帧率。

不达标保留原失败与样本，修复后同fixture比较；预算调整须有实测依据和owner决定，不能为通过临时改门槛。该专项不新增统一CI gate；普通CI覆盖算法/容量边界，实测组合由适用实现和发行验证运行。

<a id="pw-platform"></a>
### 9.3 P6剩余工作与依赖次序

下表是同一P6义务的实施顺序，不新增阈值、通用CI gate或已完成声明；数值沿[专项质量预算](../../world-simulator/viewer/external-agent-world-view.design.md#view-quality)与本节原测量窗口。测试资源按Cargo/CPU/GPU串行协调，未知分量不补零。

| 优先级/项 | 本机可执行部分 | 必要依赖与仍不能代签的范围 |
| --- | --- | --- |
| 先做：反馈合同与短观测 | 独立审定上述端点，再补真实意图只读关联/可辨反馈及至多三组短校准。 | 同意图/可见时钟缺证据先unknown；定义未定不再运行五分钟，不借旧466样本。 |
| 随后：真实世界帧间隔 | 在冻结硬件加速候选上先做短连续pan/zoom能力核对，分别确认world pass提交、GPU完成、浏览器合成/呈现的实际信号与关联。 | 当前PostUpdate metrics和rAF不是世界呈现fence；原生UI刷新或重复旧texture不能充世界帧。缺实际世界帧关联不签帧率；可能需要Render/浏览器只读观测实现。 |
| 可独立准备：30分钟长稳 | 固定预热后同负载、区域往返/选择/过滤/恢复序列；初末匹配同阶段，分别采JS、WASM线性内存、应用缓存对象/字节、可取得的GPU及owned进程资源。 | 需先补缺失的只读计数与有界长期采集；不长期累积诊断trace把工具缓存算产品增长，不在末端清缓存/重载伪造下降。缺GPU/进程/缓存分量标unknown；M3/16GiB子集不代签8GiB目标机。 |
| 条件具备后：退出跟随/stop | 只读follow/退出可先做受控UI技术回归，保实际位置/失效与手动导航语义。 | 正常hosted及真实可停止操作沿P5的合法owner/原operation/receipt；guest或假授权Stop不证明可用Stop预算，UI接受仍不等于世界已停止。 |
| 外部与发布前置：A/B、冷/热与50k query | 可先核实际encoded字节/响应头/cache语义，准备固定样本/网络和50000对象获准scope夹具。 | 正式8GiB CPU/GPU/OS与两条真实受控链路缺失时不能用16GiB或CPU节流冒充；WS/HTTP的实际RTT/带宽分别核，假page延迟不是链路。冷/热样本数、重复窗口、真实50k索引/授权与query耗时均沿原合同；1000对象fake peer或热cache不代签服务端50k。 |

组件可测不等于整项完成。正式headed硬件加速与目标机要求不能由headless子集代签；普通Web、原生容器和正常玩家认证各沿自身环境。同候选/frame/内存记录不替代P5或发布就绪。新增可并行实施片段另按原生子Issue/独立owner领取，不在本表重复创建已归属的义务。

## 10. PW-PLATFORM：Web与原生各自通过

Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a。
对应[AC-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-004)。

以实际release build和同一真实世界，分别在已声明支持的浏览器、macOS/Windows或其他原生候选执行进入、概览/局部、定位/选择、原结果、合法高层指导和断连回访。记录OS/架构/WebView依赖、安装/升级/启动资产hash；只有Launch按钮、系统浏览器跳转、截图或另一平台green一律不能证明客户端内能力。

检查受信origin/redirect/loopback策略，尝试第三方URL、Origin:null、携带token的深链、通用shell/任意文件IPC和资源hash不符。应拒绝超范围导航/IPC，不禁用TLS，不把模型token或world action后门交给网页。深链重新鉴权且只读定位，不自动提交/批准。

重复PW-RECOVERY的初始不可用、ready后失效、资源缺失与撤权恢复。共享代码不免除逐端运行；未通过的组合标未验证/不支持，不改变其他组件已存在的支持声明。

## 11. 结果与追踪

| 本手册 | 原组合场景 | 主要产品验收 |
| --- | --- | --- |
| PW-IDENTITY | viewer-guidance、viewer-authority、viewer-primary | AC-AGENT-STAGE-001/005 |
| PW-FRESHNESS、PW-PROJECTION | viewer-freshness、viewer-explore | AC-AGENT-STAGE-002/003/005 |
| PW-EXPLORATION、PW-PERFORMANCE | viewer-explore、viewer-scale | AC-AGENT-STAGE-003 |
| PW-RECOVERY、PW-PLATFORM | viewer-dual、viewer-native | AC-AGENT-STAGE-004 |
| PW-ACCESSIBILITY、PW-READABILITY | viewer-primary、viewer-dual | AC-AGENT-STAGE-001/003/004 |

报告只记录实际执行的PASS/FAIL/BLOCKED/NOT RUN、source/build/环境、fixture、输入/操作、预期与实际、原始证据位置和未覆盖边界。没有运行不填PASS，不凭HTTP200、动画、截图文件存在或一次重跑成功覆盖先前失败。普通CI、真实Runtime、GPU/S6、原生、真人理解分别结论；Task/PR保存运行记录，长期文档不追加每轮计数和CI流水。

### 当前查询总览的有限浏览器回归

按[当前查询总览合同](../../world-simulator/viewer/external-agent-world-view.design.md#overview-query-fit)，设置 `QUERY_OVERVIEW_ARTIFACT_ROOT=<冻结visual目录>`、`QUERY_OVERVIEW_SOURCE_HEAD=<实际构建源>`、`QUERY_OVERVIEW_COMPOSED_MANIFEST=<逐文件size/SHA清单>`，运行 `npm --prefix crates/oasis7_viewer run test:pixel-world:query-overview`。清单名兼容组合产物，summary保留其实际kind；canonical不得改标旧source。`QUERY_OVERVIEW_CASE=complete|cancel|terminal` 分别独立执行一次；`QUERY_OVERVIEW_WIDTH=390` 覆盖窄屏complete及状态文本边界，缺省1440。夹具仅用三个获准观察实体、每响应一条且请求上限仍200，在真实Host/WS/WASM中挂第三页。验证原生手动平移后，partial总览保持相机、不清搜索、不新增查询；同绑定complete实际应用一次，后台full不抢手动相机；取消及终止partial保持相机。每阶段最多12秒，无输入重试、无renderer/rAF冻结，失败保raw并非零退出。

此回归只证明受控技术观察入口的分页导航与指定视口布局。`navigation_result`是主Update消费，不是GPU fence；旧缺schema兼容另由Rust确定性回归覆盖，不能冒称旧入口GPU已验。该工具不证明原内存长窗触发时刻的因果，不代签真实玩家授权、P5、五分钟/30分钟或P6。产物SHA前后核对，记录实际构建源和runner分别身份；不改标已保留失败或旧QA头的raw。

### 后台读取与显式搜索竞态回归

按[显式搜索合同](../../world-simulator/viewer/external-agent-world-view.design.md#view-explicit-search-intent)，设置 `SEARCH_RACE_ARTIFACT_ROOT`、`SEARCH_RACE_SOURCE_HEAD`、`SEARCH_RACE_COMPOSED_MANIFEST` 为冻结目录、实际构建源及逐文件size/SHA清单，运行 `npm --prefix crates/oasis7_viewer run test:pixel-world:search-race`。缺省搜索；`SEARCH_RACE_CLEAR=1` 清空搜索；`SEARCH_RACE_SCOPE=1` 为兼容名称，实际只测试同scope查询scope_unavailable错误，绝非真实授权撤销。`SEARCH_RACE_WIDTH=390` 覆盖窄屏。分别单次执行，失败非零保原raw，不重试输入。

真实fill/focus后发送标准snapshot推进触后台query并挂响应，再仅一次sharedCDP可信Enter；验证submit及新search实际到wire、旧响应不能覆盖结果。查询错误允许保仍获准last-known模型，但必须明确失败、屏内主体0、旧targets不可交互，实际canvas点击不选中且旧晚包不复活。该路径不以DOM删除判定撤权，不伪造身份／grant或暂停renderer/poll。清单保实际source/runner身份与完整依赖SHA；三个实体的技术observer/debug夹具不签真实WorldService、P5/P6或任何性能／长窗结果。
