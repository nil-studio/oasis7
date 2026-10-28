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

### 9.2 容量与过载反例

测试100/1000/1001可见实体、200/201突出labels、200/201单页、2 MiB边界和超额事件；世界总量固定50000但不每帧传全量。授权后再聚合，记录实际绘制数量、关系端点和cache。关键选中/目标/blocker在裁剪后仍可达，partial不伪装对象不存在。

弱网络、大JSON/长文本、密集同地点Agent和频繁查询取消下，输入/退出跟随/stop不能被数据洪泛阻塞。取消读请求不取消世界operation。CPU、内存、查询、下载和GPU瓶颈分开，不从实体池存在推导已提高帧率。

不达标保留原失败与样本，修复后同fixture比较；预算调整须有实测依据和owner决定，不能为通过临时改门槛。该专项不新增统一CI gate；普通CI覆盖算法/容量边界，实测组合由适用实现和发行验证运行。

<a id="pw-platform"></a>
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
