# 玩家可读的世界舞台产品设计

## 文档身份

- 配对产品 PRD：[玩家可读的世界舞台](player-readable-world-stage.prd.md)
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-16
- 专业域权威：[Viewer手册](../../world-simulator/viewer/viewer-manual.manual.md)、[页面模块设计](../../world-simulator/viewer/viewer-page-module-design-2026-06-18.design.md)、[视觉规范](../../world-simulator/viewer/viewer-visual-design-spec-2026-06-05.design.md)、[大世界系统设计](../../world-simulator/viewer/external-agent-world-view.design.md)。

本设计描述可验收的目标体验，不把未来地图、多尺度、原生世界窗口或截图写成已交付。现有Shell的世界优先方向和Player/Director边界不变；具体DTO、布局参数、renderer、平台依赖和性能数值回到专业authority。

<a id="1-阅读顺序"></a>
## 1. 目标与阅读顺序

玩家应先辨认当前世界与目的、相关行动者/路线、主要blocker和下一决策，再看原行动结果及支持信息，最后按需读诊断。命令路径一步可发现，不以三张平级卡片、常驻多栏或长日志压过世界。

首次进入、回访和故障恢复共用同一概念结构：世界舞台为主，目标/下一步是紧邻上下文，选中详情按需展开，Action Receipt解释原操作，World Feed只是环境活动。无目标、无可见对象、无回执都是有解释的状态，不填充“看起来完整”的假数据。

## 1.1 P0 冻结与 P1-A Agent Context Lite

### P0：稳定框架，不扩大常驻界面

保持单一世界舞台、Next Move主区、从属Objective/Player Leverage、按需Targets/Command和次级Diagnostics。允许来源、授权、回执唯一性、窄屏与恢复的focused修正，不借本设计重排Shell。

### P1 有序组合

P1-A先确立一个选中Agent的身份、来源/时效、适用目标、Activity、Intent、回执与权限；P1-B再加入独立权威的对象、关系/路线、位置；P1-C最后处理有身份、来源、时效和生命周期的重大事件注意力。后阶段不能用未成立前置代替验收。

专业实现收据已有P1-B stage cues和P1-C crisis-only ambient projection；crisis缺空间锚点没有marker/highlight。Major Event消费独立显式runtime/operator audience policy，默认Unknown不披露；登录、控制、selection、Director不产生披露资格。局部实现不代签全部阶段。

### P1-A：只从选中的 Agent 开始

| 阅读步骤 | 应呈现的内容 | 缺失/不适用时 |
| --- | --- | --- |
| 辨认 | Agent可读身份、公开且有来源的所在/父级上下文和状态。 | 明确未知/不可用，不挑另一Agent替代。 |
| 理解局势 | 仅明确绑定该Agent的Objective、Next Move、blocker、Player Leverage。 | 全局玩家目标保留全局标签，不重命名为私有目标。 |
| 区分运行 | Activity、权威Intent、未提交草稿各自状态。 | local_pending不是已接受Intent，通信成功不是世界成功。 |
| 确认结果 | 原Player/Cinematic单一因果锚点中确属该主体的结果。 | 无绑定回执；非因果反馈独立，不补效果/成功。 |
| 继续/恢复 | 有适用来源、权限、时效的capability或下一入口。 | 说明待同步/无权/不适用；可只读，不制造按钮可用性。 |

选中到辨认上下文保持0–1次情境转换。选中/可见不等于认领或控制；账号/授权沿[Agent对话与Prompt控制](agent-conversation-and-prompt-control.prd.md)与[自治、委托和责任](agent-authority-ownership-and-accountability.prd.md)。不从距离、同屏、Feed、名称、raw object推断计划、关系、资源、成功或权限。

### P1-A 组合验收映射

| 设计检查 | 可观察承诺 | 产品验收 |
| --- | --- | --- |
| Agent identity/context | 正确主体、适用状态和空状态，不制造默认目标。 | P1A-CTX-01 / RW-1 / RW-6 |
| Source and freshness | 当前、最近已知、冲突、未知及恢复方向可辨。 | P1A-CTX-02 / RW-8 / RW-9 |
| Activity/Intent/Receipt | 与草稿、通信反馈不合并；切对象不改归属。 | P1A-CTX-03 / RW-3 / RW-4 |
| Capability boundary | 只有明确适用权威能力；全局信息不改变控制。 | P1A-CTX-04 / RW-2 / RW-6 |
| World-first readability | 解释层次级，目的/blocker/下一步可读。 | P1A-CTX-05 / RW-1 / RW-4 / RW-5 |

### P1-A 非承诺

不建立所有实体通用Inspector；Facility/Territory/Organization/Depot/Module/Location不继承Agent控制或私有目标；不新增hotbar、inventory、经济/治理数值、直接动作；不把plan/rationale/ETA/memory/provider/prompt/auth诊断当默认详情；不在此定义Major Event新因果面、DTO、坐标、颜色或布局断点。

## 2. 对象辨认与聚焦

可见对象可观察，有权对象才可能被指导，当前行动对象与当前选中对象可以不同。初始构图的默认视点只是展示，不暗示认领、控制或已有目标。

<a id="subject-continuity"></a>
### 2.1 主体连续性设计

玩家对A提交指导，再选B：舞台/详情进入B的观察上下文，原操作仍关联A。A回执到达时，在现有回执位置说明A/原目标及结果，不把视点拉回A，也不污染B的私有结果。可提供查看原对象/原操作，但仅在仍获准披露时进入；撤权删除受限内容，不展示旧详情。

无Agent绑定的玩家级结果保持玩家级；同一结果在详情/回执被引用不是两次效果。推荐变化、过滤、列表重排、重连和重载不改变已提交对象。缺明确行动对象时Next Move引导合法选择/同步，不借首项。

### 2.2 对象能力矩阵

| 对象/状态 | 可提供的观察 | 指导边界 |
| --- | --- | --- |
| 自己当前有权Agent | 适用身份、来源、活动、目标及原回执。 | 只进入现有高层目标/Prompt/委托/审批/stop。 |
| 可见但不可控Agent | 只读且已授权公开内容。 | 明确只读，不复用另一个Agent的能力。 |
| Location/设施/Depot/路线 | 实体authority已提供的身份、状态、位置/关系与来源。 | 不套Agent模板；地图点击无采集/建造/移动。 |
| terrain/Fragment block | 背景和允许的材料语境。 | 非交互背景无可点击手势或控制暗示。 |
| 有权威anchor事件 | 事件身份、来源、时效和检查入口。 | 环境提示不是玩家因果回执。 |
| 无anchor事件 | 相邻Feed的可访问内容。 | 不放估算marker/highlight。 |
| 聚合/重叠对象 | 已授权成员下钻/候选列表，说明范围/部分性。 | 聚合不是新实体，不批量授予操作权。 |

### 2.3 选择、遮挡与消失

选中同时有形状/文本/焦点，不只靠颜色/动画。对象重叠时提供稳定候选列表，不要求玩家反复点击像素来试选。tooltip/标签避开主操作；无法放置时转入相邻详情，不让画布裁掉唯一信息。

过滤、离开视点、区域未加载、来源失效、撤权和权威删除分别解释。过滤当前对象保留“被过滤”及清除过滤入口；跟随失去可信位置暂停，不另选对象。撤权立即清不可披露内容，只保留获准的通用不可用提示。

## 3. 空间与比例诚实

权威几何、派生关系布局、最近已知和未知可区分。几何与时效独立：新收到的派生坐标不是精确位置，旧权威坐标不是实时位置。放大marker/halo不是物理尺寸，关系线不产生关系或可执行距离。

高密度优先目标、选中、blocker和相关路线；次级标签渐退。overview/detail改变密度但保持身份/目的/下一步，裁剪不等于数据消失，未知不是零库存/空地图/不存在。

### 3.1 大世界可操作视觉体验与两端同义

默认旅程为概览→区域/Fragment→授权搜索→定位/选择→检查→合法指导→原结果→返回目标；这是目标，不是当前2D/zoom/native交付声明。

| 玩家操作 | 最小反馈 | 失败或中断 |
| --- | --- | --- |
| 平移/缩放/下钻 | 当前范围、尺度、身份、目标和返回。 | 旧范围可标最近已知；新范围加载失败不是空世界。 |
| 搜索 | 当前列表过滤/全授权搜索有区分；加载/无结果/partial。 | 旧查询不覆盖新查询，不泄漏隐藏数量。 |
| 定位 | 可信位置才定位，关系位置标抽象。 | 暂不能定位仍可读获准详情并刷新/返回。 |
| 跟随 | 明确主体，随时退出。 | 手动浏览退出，失效暂停；恢复需明确重新跟随。 |
| 选择/检查 | 所选对象、来源/时效、状态和适用能力。 | 不执行动作，旧详情不覆盖新对象。 |
| 返回总览/目标 | 两个意图不同，保留合法上下文。 | 目标不可定位返回文本，不跳任意对象。 |
| 提交指导 | 方案、主体、授权及保存/接受/生效/结果区分。 | 冲突/过期解释重验，不自动批准变化方案。 |

Web/native分别在自身界面完成，原生可复用Web，但外部浏览器不能代签。

<a id="exploration-transitions"></a>
### 3.2 关键状态转换机制

| 当前状态 | 触发 | 下一状态与保留内容 |
| --- | --- | --- |
| 初始同步 | 授权首份一致读模型 | 可浏览，完整/部分/未知明确，无默认控制对象。 |
| 自由浏览 | 选择授权对象 | 选择上下文，目标/原回执不变，camera不必跳转。 |
| 已选择 | 明确跟随 | 绑定该身份，仅可信位置移动视点。 |
| 跟随 | 手动平移/尺度浏览/退出 | 自由浏览，保留选择，后台不抢camera。 |
| 跟随 | 位置失效/断连 | 暂停、最近已知，不推演当前位置。 |
| 任意浏览 | 旧详情/搜索晚到 | 丢弃不适用结果，不改当前视点/选择/权限。 |
| 已选择 | 未加载/过滤 | 保留获准上下文与原因，有刷新/清过滤/返回。 |
| 任意状态 | 换账号/world/撤权 | 清私有数据、交互和旧请求，再按新范围初始化。 |
| 提交等待 | 网络/renderer失败 | 原操作、结果待确认及回查保留，不重投。 |

Escape/Focus/IME沿[页面交互合同](../../world-simulator/viewer/viewer-page-module-design-2026-06-18.design.md#27-interaction-state-machine-and-focus-contract)：先最内层drawer，再Focus；IME合成不截获。跟随退出有独立可访问入口，不抢占该顺序。

### 3.3 输入方式与探索可达性

拖动/触控不是唯一路径。方向/缩放/尺度/返回控件或等价列表使单指针不拖拽和键盘可到达相同授权对象。画布快捷键只在画布上下文生效，不吞输入框、聊天、文本选择和IME。

触控选择保留Selected上下文，不只靠hover说明；tooltip显式关闭并返回触发点。每种声明支持的输入/屏幕组合在同候选验证，未验证不标支持。

## 4. 支持文本与诊断

目标、blocker、回执、恢复有文本/等价语义；原始DTO、LLM输入输出、模型/token/延迟/source默认次级。只有专业surface明确支持才有复制/导出，不夹带token、私有观察或其他账号内容。

<a id="failure-experience"></a>
### 4.1 故障与恢复体验

| 情况 | 玩家解释 | 下一步 |
| --- | --- | --- |
| 初次无snapshot | 正在同步或尚无授权数据。 | 等待/刷新/返回，不说世界为空。 |
| query部分完成 | 当前已加载/获准范围，其他待加载。 | 重试本范围/继续浏览，不清全世界。 |
| 陈旧/冲突 | 最近已知/待确认及决策影响。 | 刷新/等待/改道，高后果由权威重验。 |
| Renderer不可用 | 图形、任务和网络状态独立。 | 文本/Targets/原操作查询/合法控制/显式重试。 |
| ready后失效 | 画面不实时，旧像素不能命中。 | 重建后重验来源/选择/命中。 |
| 断连 | 新信息不确认，获准历史仍可查。 | 重连/回查，不把完成改pending重做。 |
| 撤权/换账号 | 内容不能继续披露。 | 清旧内容后按新身份进入。 |
| 原结果不确定 | 恢复确认中，不是普通失败/成功。 | 查原操作，不以新键重复副作用。 |

文本降级不绘第二JS世界，恢复不自动提交指导/切世界/加权限/调用模型。保留上下文不能成为泄漏已撤权内容的理由。

## 5. 响应式与可访问性

CJK、长名、低高度、窄屏和放大文本保持主读、下一步、恢复。状态有文本/形状/位置，不只靠颜色或运动；减少动效仍可辨认选择/blocker/结果。

键盘可进入/离开画布和详情，焦点可见并合理返回。刷新不抢焦点、不逐实体洪泛播报，不重复宣布原结果成功。图形缺失时文本入口仍支持获准观察和安全返回。

视口/DPR/语言/输入/密度按[可访问性场景](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility)验证，不以一张桌面截图代签移动、辅助技术、renderer或真人理解。

<a id="6-非承诺"></a>
## 6. 决策、风险与非承诺

不把Locate Agent、overview、native、空间marker或手势当当前已实现；不新增世界动作/规则/所有权/通用Inspector；不在产品层定义厘米字段、LOD、shader、资产和帧率。工业/实体只在对应authority支持时展示。

选择复用现有Shell而不是重排信息架构，代价是通过按需上下文承载更复杂解释；选择明确未知而非自动补数据，代价是部分情况下只能只读/等待。上游原操作关联、平台依赖、实际密度质量和真人理解仍需实施/验收，设计不能宣布风险已消除。

<a id="prd-reqac-fragment-mapping"></a>
## 7. 验证与追踪

| PRD requirement | PRD acceptance | 本设计承接 |
| --- | --- | --- |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[REQ-AGENT-STAGE-001](player-readable-world-stage.prd.md#req-agent-stage-001) | [AC-AGENT-STAGE-001](player-readable-world-stage.prd.md#ac-agent-stage-001) | 首读、主体连续性、原结果与恢复。 |
| Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；[REQ-AGENT-STAGE-002](player-readable-world-stage.prd.md#req-agent-stage-002) | [AC-AGENT-STAGE-002](player-readable-world-stage.prd.md#ac-agent-stage-002) | 来源/时效独立、冲突、重验和失败解释。 |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[REQ-AGENT-STAGE-003](player-readable-world-stage.prd.md#req-agent-stage-003) | [AC-AGENT-STAGE-003](player-readable-world-stage.prd.md#ac-agent-stage-003) | 探索、跟随、重叠、partial及多输入。 |
| Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；[REQ-AGENT-STAGE-004](player-readable-world-stage.prd.md#req-agent-stage-004) | [AC-AGENT-STAGE-004](player-readable-world-stage.prd.md#ac-agent-stage-004) | 两端自身交互，重连/图形/权限恢复。 |
| Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；[REQ-AGENT-STAGE-005](player-readable-world-stage.prd.md#req-agent-stage-005) | [AC-AGENT-STAGE-005](player-readable-world-stage.prd.md#ac-agent-stage-005) | 观察/指导分离、主体固定和无副作用浏览。 |

具体测法在[专项手册](../../testing/manual/pixel-world-visualization-validation.manual.md)，实际记录在PR/Task。产品设计完成不代替系统实现、独立评审、真人理解或发行批准。
