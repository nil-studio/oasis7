# Viewer Pixel World Player-Readable Rendering 详细设计

- 配对要求：[玩家可读渲染PRD](viewer-pixel-world-player-readable-rendering.prd.md)。
- Owner：`viewer_engineer`；视觉/QA协作。
- 审读：2026-10-16；基线 `a2389f437e6cedc3b870a91d33a1cdf40a00a8da`。
- 历史任务：`task_b399bf37eff94c44a300c55f5db739d3` / #1294，原证据从Git/Task追溯。
- 本文定义主体隔离和降级合同，保留已有HUD/Feed；主体隔离的当前实现见§3，其余目标与完整运行验收仍须分别核验，不宣称整体验收通过。

## 1. 问题、目标与非目标

保持世界优先、有效关系可读、单一因果锚点与合法恢复，防止显示fallback污染动作/回执。复用现有Rust投影、WASM和Shell；不扩世界schema、宿主ABI、布局或权限，不建立第二JS世界。

## 2. 上游约束与相关角色

产品拥有玩家承诺；Shell拥有Player/Director、dock、Search/Quote、布局；Runtime/Agent拥有gameplay、权限、原操作/结果；Rust host派生DTO，JS消费并交互；QA分别验证Rust、mock、WASM和真实浏览器。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[Commercial DTO](viewer-pixel-world-player-readable-rendering.prd.md#41-commercial-surface-dto) | gameplay派生HUD，无数据不造目标/进度。 | [DTO](#dto-shape) | Runtime/gameplay、Rust host。 | mock不证明Rust/世界执行。 |
| Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；[主体绑定](viewer-pixel-world-player-readable-rendering.prd.md#subject-binding-requirements) | 焦点/提交/回执独立，选择不改原操作。 | [主体解析](#subject-binding) | 原operation关联、owner授权。 | 不在Viewer新建admission。 |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[Receipt](viewer-pixel-world-player-readable-rendering.prd.md#411-action-receipt-dtostate-mapping) | pending/advanced/no-progress/blocked/unknown/无回执分别解释。 | [回执映射](#action-receipt-mapping) | 原feedback/operation/receipt。 | Feed不代签因果。 |
| Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；[来源与降级](viewer-pixel-world-player-readable-rendering.prd.md#render-degradation-requirements) | 几何/时效独立，partial/失效/撤权不抹平，回退无新世界。 | [来源与恢复](#display-provenance) | 授权客户端、WASM/host。 | 旧DTO无法表达时不能假装current。 |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[UI层级](viewer-pixel-world-player-readable-rendering.prd.md#42-ui-layering) | Next Move主区、从属上下文和窄屏顺序。 | [布局](#ui-layout) | Shell/visual。 | 不新增布局或证明完整S6。 |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[Feed合同](viewer-pixel-world-player-readable-rendering.prd.md#412-world-feed-v1-implementation-contract) | 精确顺序/去重/cursor恢复和环境隔离。 | [Feed](#world-feed-visual-handoff-implemented) | journal/transport/Viewer。 | recentEvents不是正式Feed。 |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[产业要求](viewer-pixel-world-player-readable-rendering.prd.md#industry-detail-requirements) | 权威节点/流/blocker及推断/未知可读。 | [产业细节](#industry-detail-target) | Runtime产业projection/visual。 | routes/HUD不等于完整图谱。 |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[产业分层](viewer-pixel-world-player-readable-rendering.prd.md#industry-detail-requirements) | World/Region/Node可见集和负载有界。 | [产业分层](#industry-scale-target) | 空间投影/Viewer/QA。 | 不单独证明semantic zoom/探索。 |
| [UI层级](viewer-pixel-world-player-readable-rendering.prd.md#42-ui-layering) | 局部细化世界图层/形状、密度、观察主体与原因果分离、失败后的合法文本；不改变上游范围。 | [Bevy详细视觉交接](#bevy-detailed-visual-handoff) | viewer_engineer / visual / QA；Runtime原操作与授权、Rust投影。 | 静态源码与设计图不证明真实GPU、完整恢复、权限链、真人理解或新增runtime能力。 |
| [UI层级](viewer-pixel-world-player-readable-rendering.prd.md#42-ui-layering) | 局部细化世界图层/形状、密度、观察主体与原因果分离、失败后的合法文本；不改变上游范围。 | [密度](#bevy-density-handoff) | viewer_engineer / visual / QA；Runtime原操作与授权、Rust投影。 | 静态源码与设计图不证明真实GPU、完整恢复、权限链、真人理解或新增runtime能力。 |
| [来源与降级](viewer-pixel-world-player-readable-rendering.prd.md#render-degradation-requirements) | 局部细化世界图层/形状、密度、观察主体与原因果分离、失败后的合法文本；不改变上游范围。 | [恢复](#bevy-recovery-handoff) | viewer_engineer / visual / QA；Runtime原操作与授权、Rust投影。 | 静态源码与设计图不证明真实GPU、完整恢复、权限链、真人理解或新增runtime能力。 |
| [主体绑定](viewer-pixel-world-player-readable-rendering.prd.md#subject-binding-requirements) | 局部细化世界图层/形状、密度、观察主体与原因果分离、失败后的合法文本；不改变上游范围。 | [Bevy详细视觉交接](#bevy-detailed-visual-handoff) | viewer_engineer / visual / QA；Runtime原操作与授权、Rust投影。 | 静态源码与设计图不证明真实GPU、完整恢复、权限链、真人理解或新增runtime能力。 |

<a id="current-state"></a>
## 3. 当前状态与差距

pixel_world_host.jsx将bounded snapshot交Rust host_state.rs::build_render_state，消费world_bounds、locations、fragment_terrain、agents、links、hotspots、selection与commercial_surface；WASM绘制世界。focus/right-panel兼容hooks、Targets的entity-search仍存在；world_feed/v1来自独立Runtime journal。

当前active_agent_id只取可见集合中的明确selection；next_action只取推荐动作自己的target。回执消费独立receiptTargetAgentId（包括显式null），本地反馈固定其提交主体，Runtime兼容绑定来自原player-facing feedback；不再借推荐/选择/首项补主体。缺绑定保留未知或玩家级范围，不推断控制资格。

<a id="design-decision"></a>
## 4. 边界与设计决策

保留字段和Rust owner，分离内部主体解析，删除显示焦点流向业务target的隐式路径。来源/时效不能表达时抑制不诚实动态图形并保留合法文本，不新写JS世界或重排Shell。

<a id="dto-shape"></a>
### 4.1 显示DTO

commercial_surface保留objective.title/detail/progress_percent、next_action.label/detail/target_agent_id/execute_kind、active_agent_id、player_leverage.state/summary/detail、blocker、world_read和action_receipt。无权威gameplay不由计数合成业务字段；world_read说明当前范围/partial，不是全世界总量。

内部可增加以下类型化结果，而不新增世界/WASM字段：

```text
DisplayFocus = explicit selection | supported recommendation | none
ActionBinding = explicit authorized subject + action/goal context | unavailable
ReceiptBinding = original-operation agent binding | authorized player scope | unknown
```

这是设计类型，不声称已有同名实现。焦点与原操作分别传递，不能由一个可选字符串承担三种意义；JS只消费Rust与已有授权UI状态，mock的buildTestRustRenderState不能进入生产。

<a id="subject-binding"></a>
### 4.2 主体解析与不可变关联

active_agent_id只表达关注对象，缺明确来源可null，构图不说明认领/控制。next_action.target_agent_id仅取当前适用、明确主体的动作/指导上下文；缺目标/授权则不可执行并给选择/同步/Command，不能or_else(active_agent_id)。

回执先关联原operation和受权feedback，再取明确主体；新的acceptedIntentTarget、recommendedAction、selection不足以给旧回执归属。合法玩家级结果可target=null且明确玩家级；无法确认属当前玩家/操作则不进成功展示。

A在途切B时原结果仍归A，B详情只读自己的结果；重复引用不是重复效果。当前Rust已拆分回执主体解析，build_action_receipt不再接收display active_agent_id作为fallback；后续必须保持此隔离。上游缺原操作关联则fail closed并跟踪缺口，不伪造Runtime字段；真正新增上游字段须同步其authority与版本。

<a id="host-algorithm"></a>
## 5. 关键运行流程

Rust先解析授权世界集合/selection，独立解析gameplay目标、下一步和原操作主体，构建receipt与world_read，最后附加commercial_surface。位置/关系/Activity/Intent/因果各自校验，不因有位置或活动推导关系与成功。缺gameplay仍可绘获准世界，但不制造目标。

<a id="action-receipt-mapping"></a>
### 5.1 回执映射

只消费权威recent_feedback及同一原操作关联的execution_state、causality_kind/detail、last_world_change。本地请求反馈只能表达原身份匹配的等待，local_pending草稿不是accepted。

| 状态 | 玩家表达 | 禁止替代 |
| --- | --- | --- |
| 无回执 | No action receipt yet。 | Feed/计数补效果。 |
| accepted/submitted/queued/ack | 原对象/目标pending。 | 完成文案或世界delta。 |
| completed_advanced | 原操作权威进展。 | 他人成果或当前selection补主体。 |
| completed_no_progress | 完成但无进展，原因/下一步。 | 一律变成功奖励。 |
| blocked/rejected | 原因和重新确认/修正。 | 改参数自动重投。 |
| recovery_required/unknown | 查原operation、待恢复确认。 | 普通失败后换新键。 |

delta_logical_time/event_seq只在既有合同允许的权威已发生阶段展示；缺失未知，不用动画或本地时间算。confidence说明来源确定性，不把猜测变完成。

<a id="display-provenance"></a>
### 5.2 来源、完整性与权限恢复

消费[大世界适配](external-agent-world-view.design.md#view-adapter)的几何/时效、cut、audience、partial侧带。旧renderer无法区分last-known/current时，使用明确陈旧样式，否则抑制动态图形并给文本；旧精确坐标不能直接当当前snapshot。页缺项不是删除，world_read不是总量。

换主体/audience先清旧私有图形、DOM、hover、搜索，再接新数据。保留上下文仅限仍可披露内容。current badge不是写许可，高后果沿原API/Runtime重验；图形和文本的恢复完整采用[回退表面合同](#fallback-route-rendering)。

## 6. 接口与数据合同

createPixelWorldBridge、mount/update/tick/unmount、camera_state_changed、hover_entity、select_entity保持。主体整改不需改WASM ABI，target字段存在不等于授权。tick/revision沿精确API表示，不能Number化参与身份/排序。

<a id="ui-layout"></a>
### 6.1 布局映射

pixel-world-host__summary简短解释世界/目标；pixel-world-command-strip是Next Move主区及Objective/Player Leverage从属，窄屏主区第一行。canvas保持terrain、有效关系、实体和选中/目标/blocker层次；render-diagnostics折叠renderer/camera/counts/raw DTO/测试控制。

<a id="terminal-shell-relationship"></a>
### 6.2 Shell关系

[Shell PRD](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary)拥有Player/Director、dock、console、Search/Quote与布局，不复活三张平级卡、Next Move第二行、常驻Feed或第二模式。

## 7. 状态、持久化与恢复

显示状态可重建，原operation/receipt归Runtime/API持久保存；重载/选择不重做动作。renderer/network/query/task/executor独立，图形失败不意味着世界停止。

<a id="fallback-route-rendering"></a>
### 7.1 DOM、WASM与文本降级边界

保留旧anchor供引用，但不再笼统要求WASM失败时DOM画一个世界。

| 表面 | 有效合同 | 不证明/不允许 |
| --- | --- | --- |
| 正式WASM | Rust派生且获准、来源有效的关系，route介于terrain与前景实体。 | location_id/邻近不造关系，terrain/route不新增控制。 |
| Rust派生DOM语义测试/明确辅助表面 | 同坐标投影测合法关系、层级、非交互，固定fixture/source。 | 不证明GPU，不是独立JS世界或正式ready。 |
| 无GPU但合法读数据存在 | 目标/Targets/原回执/文本/安全控制及Renderer Unavailable。 | 不用DOM重建“正常世界”。 |
| Rust派生/资源不可用 | 既有独立授权UI可显示合法文本/原结果及原因。 | 不用JS重算世界或旧私有图形兜底。 |

ready后失效停止旧命中，旧帧不实时，重建会话后重应用授权视图/camera/selection并验证命中；probe成功不是恢复完成。沿[恢复状态机](external-agent-world-view.design.md#view-recovery)，不重置授权/任务/原操作，不自动执行世界动作。

<a id="world-feed-visual-handoff-implemented"></a>
### 7.2 World Feed既有交付合同

world_feed/v1是fullscreen Player顶部边缘紧凑折叠ambient chip，按需展开有界面板；移动/平板可调整安全区，不做常驻首屏列或success receipt。

按source order和 `(world_id,reorg_epoch,event_seq)`去重，epoch/seq为精确非负十进制字符串，按精确比较器；legacy数值只沿已支持兼容，不经JS Number或本地时间排序。

| Feed状态 | 表达 | 恢复 |
| --- | --- | --- |
| loading | 预留安静框/短文案。 | 保留合法board/receipt。 |
| empty | 明确无事件。 | 不补事件。 |
| ready | 环境列表。 | receipt_ref仅Runtime显式身份。 |
| replay | 明确重放。 | 有界历史，不是新收益。 |
| gap/reorg | 警告。 | 重取cursor/snapshot，不拼未知事件。 |
| unavailable | 原因。 | 适用恢复入口。 |

Recent Events/Feedback不重命名成Feed；没有明确关联receipt_ref=null。稳定源码/生成锚点仍为viewer-world-feed。

<a id="player-visual-feedback-invariants-2026-09"></a>
### 7.3 既有移动与交互约束

Selected chip在安全区换行，触控/键盘仍关联正确实体。LIVE仅连接且Feed ready，REPLAY/NO EVENTS/GAP/UNAVAILABLE独立，不代表成功。展开Feed不盖Next Move/Receipt。

Hotspot只读检查，可触控/键盘读kind/label，说明保持至Escape或显式关闭并恢复焦点，不提交玩法动作。长Next Move/blocker/恢复文本换行或内部滚动，不挤走主操作；Escape/IME顺序沿页面设计。

## 8. 部署、安全与兼容

canonical viewer.js和dist compat由同一build/finalize，optional WASM遵守manifest/hash。源码/生成物分别验，release与visual-test不互相代签；不为自动化把测试API装进release。

诊断/DOM probe/截图只在授权测试范围留证，不含token/模型Key/他人私有数据；native/Web共享合同但逐平台验证。

## 9. 质量与产业分层

<a id="industry-detail-target"></a>
### 9.1 产业细节目标

支持时同一projection驱动文本/舞台的节点、流类型/方向、吞吐/损耗、层级/阶段/状态及定位；R1~R5、bootstrap/scale/governance只在数据支持时表达。推断根因与确认分开，缺失未知；已有HUD/routes不证明完整图谱。

<a id="industry-scale-target"></a>
### 9.2 产业空间层级目标

World/Region/Node优先热区/主干、区域关系、配方/库存，目标定位与节点/边负载有界。复用支持资产，不复活旧图谱；数据扩展由Runtime承接。

选中/目标/blocker/关键路线优先背景，重叠有可访问候选。数值预算复用[view-quality](external-agent-world-view.design.md#view-quality)，不维护第二份帧率/容量真值。

## 10. 迁移与回滚

先补A/B切换、无绑定和玩家级结果失败用例，再拆Rust helper并删除业务target兜底，更新host/同候选产物。时效/partial适配独立验，不能放宽relation authority修测试。

回滚只到满足来源/归属/权限的构建，不兼容则合法文本及原操作恢复；不恢复错误主体、不改世界历史/pending。旧JS算法从固定基线Git追溯，不混作当前owner。

<a id="verification"></a>
## 11. 验证设计

真实host_state_tests.rs、pixel_world_host.test.jsx及定向入口保留，新增case须能使旧fallback失败。mock loader的buildTestRustRenderState不证明Rust/WASM。软件raster、真实GPU、浏览器输入和真人理解分别留证。

### 11.1 验证映射表

| 上游 requirement / acceptance | 本设计条款 | 独立 obligation 与条件 | 验证方法 / source / scenario / environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [Commercial DTO](viewer-pixel-world-player-readable-rendering.prd.md#41-commercial-surface-dto) | [DTO](#dto-shape) | 无gameplay不造目标/进度。 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)空输入，host_state_tests.rs真实Rust同候选。 | 输入/DTO/退出码。 | mock不证明Rust。 |
| [主体绑定](viewer-pixel-world-player-readable-rendering.prd.md#subject-binding-requirements) | [主体解析](#subject-binding) | A在途切B、推荐/列表变化、缺原绑定。 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)，Rust/host分别回归，固定candidate。 | 原operation/target对selection，旧行为负例。 | 未运行不说已修复。 |
| [Receipt](viewer-pixel-world-player-readable-rendering.prd.md#411-action-receipt-dtostate-mapping) | [回执映射](#action-receipt-mapping) | pending/advanced/no-progress/unknown/无回执。 | [PW-FRESHNESS](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-freshness)原操作恢复及既有receipt tests，同候选S6另验。 | 状态/delta/原结果和恢复。 | UI不证明世界效果。 |
| [来源与降级](viewer-pixel-world-player-readable-rendering.prd.md#render-degradation-requirements) | [来源与恢复](#display-provenance) | last-known不变current，partial不误删，撤权/失效清图形。 | [PW-PROJECTION](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-projection)、PW-RECOVERY，确定性和实际GPU分开。 | 页/DTO/权限、旧命中及恢复。 | GPU-disabled不代签ready后恢复。 |
| [UI层级](viewer-pixel-world-player-readable-rendering.prd.md#42-ui-layering) | [布局](#ui-layout) | 主次、窄屏/输入保持。 | [PW-ACCESSIBILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility)、PW-READABILITY，同候选真实表面。 | 视口截图/焦点/回答。 | DOM/模型不代签真人。 |
| [Feed合同](viewer-pixel-world-player-readable-rendering.prd.md#412-world-feed-v1-implementation-contract) | [Feed](#world-feed-visual-handoff-implemented) | 顺序/精度/去重/恢复和ambient隔离。 | [PW-FRESHNESS](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-freshness)，既有world_feed_state/transport/panel tests，固定fixture/source。 | 序列/cursor/状态/截图。 | recentEvents不替journal。 |
| [产业要求](viewer-pixel-world-player-readable-rendering.prd.md#industry-detail-requirements) | [产业细节](#industry-detail-target) | 权威节点/流/状态/根因。 | [产业验收](viewer-manual.manual.md#industry-detail-acceptance)，受支持projection，同候选headed。 | 原数据/未知/推断与截图。 | 不证明完整数据已提供。 |
| [产业分层](viewer-pixel-world-player-readable-rendering.prd.md#industry-detail-requirements) | [产业分层](#industry-scale-target) | 分层可见集、负载、目标定位。 | [PW-PERFORMANCE](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-performance)及产业验收，真实候选。 | 远中近集合与实测。 | helper不证明semantic zoom。 |
| [UI层级](viewer-pixel-world-player-readable-rendering.prd.md#42-ui-layering) | [Bevy详细视觉交接](#bevy-detailed-visual-handoff) | [Bevy设计状态矩阵](#bevy-design-state-matrix)；本地设计case B01–B13覆盖单主区、A/B/C主体、来源/密度/输入/初始与ready后故障；case非既有测试ID。 | 复用[PW-IDENTITY/PW-PROJECTION/PW-FRESHNESS/PW-RECOVERY/PW-READABILITY等专项手册](../../testing/manual/pixel-world-visualization-validation.manual.md)，分别留Rust、host、raster、GPU与真实输入证据；当前仅静态设计目标。 | 同candidate的viewport/DPR/字体/相位/授权输入、截图及实际步骤；设计图单独标记。 | 未执行不宣称pass；设计图不代签真实Web/native、性能、真人或发行。 |
| [UI层级](viewer-pixel-world-player-readable-rendering.prd.md#42-ui-layering) | [密度](#bevy-density-handoff) | [Bevy设计状态矩阵](#bevy-design-state-matrix)；本地设计case B01–B13覆盖单主区、A/B/C主体、来源/密度/输入/初始与ready后故障；case非既有测试ID。 | 复用[PW-IDENTITY/PW-PROJECTION/PW-FRESHNESS/PW-RECOVERY/PW-READABILITY等专项手册](../../testing/manual/pixel-world-visualization-validation.manual.md)，分别留Rust、host、raster、GPU与真实输入证据；当前仅静态设计目标。 | 同candidate的viewport/DPR/字体/相位/授权输入、截图及实际步骤；设计图单独标记。 | 未执行不宣称pass；设计图不代签真实Web/native、性能、真人或发行。 |
| [来源与降级](viewer-pixel-world-player-readable-rendering.prd.md#render-degradation-requirements) | [恢复](#bevy-recovery-handoff) | [Bevy设计状态矩阵](#bevy-design-state-matrix)；本地设计case B01–B13覆盖单主区、A/B/C主体、来源/密度/输入/初始与ready后故障；case非既有测试ID。 | 复用[PW-IDENTITY/PW-PROJECTION/PW-FRESHNESS/PW-RECOVERY/PW-READABILITY等专项手册](../../testing/manual/pixel-world-visualization-validation.manual.md)，分别留Rust、host、raster、GPU与真实输入证据；当前仅静态设计目标。 | 同candidate的viewport/DPR/字体/相位/授权输入、截图及实际步骤；设计图单独标记。 | 未执行不宣称pass；设计图不代签真实Web/native、性能、真人或发行。 |
| [主体绑定](viewer-pixel-world-player-readable-rendering.prd.md#subject-binding-requirements) | [Bevy详细视觉交接](#bevy-detailed-visual-handoff) | [Bevy设计状态矩阵](#bevy-design-state-matrix)；本地设计case B01–B13覆盖单主区、A/B/C主体、来源/密度/输入/初始与ready后故障；case非既有测试ID。 | 复用[PW-IDENTITY/PW-PROJECTION/PW-FRESHNESS/PW-RECOVERY/PW-READABILITY等专项手册](../../testing/manual/pixel-world-visualization-validation.manual.md)，分别留Rust、host、raster、GPU与真实输入证据；当前仅静态设计目标。 | 同candidate的viewport/DPR/字体/相位/授权输入、截图及实际步骤；设计图单独标记。 | 未执行不宣称pass；设计图不代签真实Web/native、性能、真人或发行。 |

## 12. 决策、风险与未决范围

<a id="follow-up-brainstorm"></a>
### 12.1 后续候选

viewer-pixel-world-player-leverage-production-readability-2026-05-28.brainstorm.md保留command-lens/production-pulse/pixel-moment候选，不是第二Shell authority；新增数据/交互需更新对应合同，不复活旧PM流程。

<a id="边界决策与风险"></a>
### 12.2 边界和剩余风险

世界主舞台、单一Next Move、单一原因果锚点、Rust派生与host消费不变。风险是上游缺原操作关联、旧DTO不能表达时效、DOM/raster误作GPU、历史进度冒充当前通过。实际整改、独立评审和证据在PR/Task跟踪，本文不能声明关闭。

<a id="bevy-detailed-visual-handoff"></a>
## 13. Bevy世界可读性详细视觉交接

本节细化已有视觉、来源、主体与恢复合同，不创建第二Shell authority。源码观察固定于 `nil-studio/oasis7@7a89dbc8d2403d037339186da15135325f4d604f`；它不更新前文历史审读基线、不宣称运行时整改或真实GPU/交互验收完成。

表中 **existing/source baseline** 数值是静态源码观察；**design/review/acceptance** 条款是未来同候选视觉验证输入，两者不能互相代签。不新增schema、ABI、权限、世界规则、地图玩法动作或独立JS世界。Shell布局、品牌token、质量预算和原操作权威继续由原合同拥有。

### 13.1. Reading contract

Fullscreen world first. One **Next Move** primary region owns the next decision, with its inline blocker; Objective and Player Leverage are subordinate context, never three peer cards. On narrow screens Next Move is first. World / Targets / Command remain the dock routes; Search stays within Targets, Diagnostics secondary. World Feed remains a compact ambient edge chip, not another permanent panel or causal receipt.

Map click/tap/keyboard selection is observation, not control. Terrain/fragments, routes and semantic cues cannot move, harvest or build. A selected Agent is not implicitly owned or authorized. Any high-level Agent guidance uses the existing authorized Command/Runtime path and explicit subject. No graphics-only shortcut executes a command.

The first-read sequence is **next decision/blocker → relevant Agent/goal → valid relationship → original Action Receipt → optional context**. This is reading priority, not a demand to raise every sprite above all other sprites or add permanent panels.

<a id="bevy-layer-mark-reference"></a>
### 13.2. Layer and mark reference sheet

Use one enlarged mark sheet alongside actual-scale in-context crops. Do not enlarge screenshots and claim actual-scale readability. Values below are existing render-space constants; record canvas dimensions, CSS dimensions, DPR and camera together before comparing pixels.

| Existing layer/mark | Exact source baseline | Visual handoff obligation |
| --- | --- | --- |
| Grid | z=0; low-alpha blue lines | Orientation only; never a target or selectable tile grid. |
| Fragment material | shadow/base/inset/fleck z=.34/.35/.36/.37 | Keep below relationships and actors. Material color is composition presentation, not inventory, value, ownership or harvestability. |
| Directed links | direction cue z=.62; shown only at supported length >=20px; arms 7px, thickness 1.6px | Use only valid supported relation authority; no link from location_id, adjacency or guessed movement. Arrow is a relationship direction, not movement permission. |
| Location | z=1; default primary size 16px/alpha .72; logic anchor 10px/alpha .32; alpha clamp .16–.84 | Terrain-bearing Location becomes a subdued logical anchor. No false geographical radius semantics. |
| Selected Location | size ×1.35, body z=2, outline z=2.1; 2px stroke/2px padding | Full outline differs from Agent corner selection. Keep name available in Targets/context when crowded. |
| Facilities/module entities | base z=2.55/2.7 | Preserve supported identity silhouettes; do not invent production, stock or throughput data. |
| Agent | z=3; default 12px; selected base at least 15px then ×1.35, z=4 | Actor stays legible above terrain. Marker enlargement expresses relevance, not physical size or capacity. Neutral core .32×body; silhouette .34×body, absent below 6px. |
| Selected Agent | four L corners, 2px thick/6px arms/3px padding; amber RGB 251,191,36 | Selection is shape + text, not color alone. No capability or success meaning. |
| Recommended target | cyan upper bracket, 12px wide/5px sides/2px stroke/5px padding | Display-only recommendation. Below selection/receipt. Suppressed when same Agent has receipt cue; Next Move/context must still explain recommendation. |
| Receipt target | neutral pale cue; pending stem, completed bar, blocked X, rejected slash, unknown dot | These are pointers to the authoritative Action Receipt, not a replacement or proof of progress. No celebratory green applied to pending or completed-with-no-progress. |
| Blocked receipt badge | 20×16px normally; 28×22px at canvas width <=360; selected narrow badge offset −34px x | Preserve shoulder clearance from selected corners and upper-right tooltip. 390px browser width is not the same as 360px canvas width. |
| Derived position | four edge-midpoint ticks, 1px thick/4px length/2px padding | Pair with `position=location_derived` text. Never relabel as snapshot/exact because recently refreshed. |
| Missing position | four hollow corners, 1px thick/3px arms/2px padding, muted | Renderer may place a presentation fallback marker. Explain that it is not a world coordinate; never derive route/control from it. |

The existing blocked-attention z=4.04 and receipt layering protect feedback above selection. Keep the shape distinction in monochrome review. A receipt-target marker can remain on A while the amber selection is on B; that is the correct visual, not a defect to fix by relabeling A's result.

#### Token mapping, without a parallel palette

Reuse `--color-world-background`/`--color-world-depth`, `--color-stage-surface`, `--color-panel-surface`, `--color-text-primary`/`--color-text-muted`, `--color-agent-primary`, `--color-objective-accent`, `--color-route-active`, `--color-warning`, `--color-blocker-danger`, `--color-receipt-success`, `--color-focus-ring`. Their existing aliases remain brand-owned. The inspected shell aliases include background `#0b100f` and objective/focus amber `#d0a85b`; these must not be silently substituted for the separately observed Bevy RGB constants or treated as proof of synchronized token plumbing. Rust RGB constants above are observed implementation values, not proof they already consume CSS variables; document mapping before any future token plumbing. Terrain stays subdued; do not recolor every material cyan or every warning amber in a way that removes meaning.

Retain brand spacing 18px panel gap / 16px stack / 8px controls, tool/stage/panel radii 12/18/20px and feedback token 140ms for shell annotations. These are not new world-unit sizes. Keep current body/display/mono typography roles. Technical provenance may be secondary but cannot be hidden exclusively in raw DTO diagnostics.

<a id="bevy-density-handoff"></a>
### 13.3. Density, labels and overlap

**Existing limits:** one global canvas text budget of 100 visible labels and 128 Unicode scalars per label. Budget priority is selected identity/resource readout → primary goal hotspot → primary blocker hotspot → other hotspots → identities → other labels; stable ID ordering breaks ties. Primary goal/blocker here means lexicographically first supported hotspot of that kind, not an inferred player-business priority. Do not describe this helper as a complete intelligent prioritization system.

Identity labels are 10px, max 24 characters plus ellipsis; Agent offset 12px above marker, Location 14px. Ordinary identity labels require zoom >=1.75. Selected Agent and qualifying active-intent labels bypass that zoom gate; recommended/receipt targets alone do **not**. Agent candidate ordering is selected → active intent submitted/accepted/blocked → recommended or receipt → ordinary. Existing rectangular overlap checks and map-label obstacles suppress labels, with priority exceptions. Location selection bypasses ordinary suppression. Map objective/blocker/route text is 11px, max 28 characters, offsets 22/22/14px.

**Design review rules (not a new algorithm):**

1. Demonstrate the existing suppression at 0.6, 1.75 and 3.5 camera zoom in the same fixture; keep marker identity and full Targets list despite hidden labels. Do not add more text to make a dense sheet appear complete.
2. Treat 100 as a hard existing ceiling, not a target to fill and not proof that 100 labels remain legible. Remove secondary map text/texture before shrinking primary Next Move, blocker or receipt.
3. Show full selected identity, position source and operation ownership in accessible context; a truncated 10px canvas label cannot be the only reading route. Do not shrink CJK to fit the approximate Latin glyph-advance model.
4. Crowded markers need the existing accessible Targets route to individually choose the correct visible authorized object. Deterministic scattering does not prove overlap selection works. No invented radial menu, grouping control or collision algorithm is part of this docs-only handoff.
5. Include same-position Agents A/B/C, selected Location plus Agent, goal and blocker collocation, and a long CJK label. If markers/cues still collide, label it **unverified readability risk** rather than changing position authority or implying hidden objects were deleted.
6. Label-budget hiding must not remove markers, hit regions or complete Targets entries; throughput text backing must disappear with its hidden text, leaving no opaque orphan strip.

Fragment LOD remains **screen footprint <1.5px hidden; <10px background; otherwise detail**. Use screen size computed from bounds/canvas/camera, not a replacement fixed-zoom threshold. Detail is still noninteractive. World/Region/Node semantic hierarchy is an upstream target, not proof of a shipped semantic-zoom mode. No new FPS/capacity promise; performance budgets stay in the external-agent-world-view authority.

### 13.4. Viewport and motion handoff

Existing auto-fit computes authorized rendered points with 180px total axis allowance, minimum remaining extent 120px, minimum content extent 40px and zoom clamp .6–3.5; user override prevents repeated auto-fit. Explicit selected focus uses the existing pending focus target, preserves zoom and marks user override. These constants do not establish HUD-aware safe-area avoidance.

Static sheets must therefore show the **actual unoccluded usable world rectangle**, not simply center the world under an opaque panel. Test 1280×720, 390×844, a tablet, and low-height landscape with Next Move, dock, receipt, selected chip and opened contextual surface. Opening context cannot conceal the sole blocker/receipt explanation. Keep world identity and command access visible; long bilingual content wraps or uses the existing bounded surface rather than growing an always-open sidebar across the world. Do not claim a new camera collision solver or automatic follow behavior.

Show manual exploration without camera snapping back on each refresh. Explicit focus/return controls remain observation. Stage shortcuts cannot consume chat/text/IME input; Escape follows innermost surface and restores invoker focus. Touch must not depend on hover, and target lookup supplies a non-drag alternative to exploration where supported.

Existing reduced-motion path sets animation time to 0; it freezes phase, not necessarily unit scale. Agent pulse amplitude is .12, ordinary Location .08, hotspot .10; logic anchors do not pulse. The target sheet needs normal and reduced-motion stills with identical semantic cues. No pulsing essential error, auto-pan celebration, success confetti or animated flow inferred from activity. Shell feedback uses the existing fast token where appropriate, but reduced motion retains selection, blocker and receipt meaning without animation.

### 13.5. Source/freshness/causality separation

| Input ambiguity | Honest stage/context expression | Forbidden inference |
| --- | --- | --- |
| location_id but no Agent pos | derived marker/ticks plus source text; no confirmed relation if envelope absent | assignment, ownership or route from co-location |
| snapshot coordinate but stale observation | last-known text and supported stale styling; suppress misleading dynamic visual if old DTO cannot distinguish | exact implies current; current implies executable |
| current relation with one missing endpoint | no unsupported line; authorized text explains missing spatial context | draw guessed endpoint |
| invalidated/stale relation with still-valid point | remove line; retain legitimate actor location | delete Agent because line vanished |
| partial/page-limited visible set | current scope/partial label; counts scoped to view | whole-world totals, absent page means deletion |
| no gameplay summary | authorized world observation and explicit unavailable next-action context | default Objective/progress from counts |
| A pending, select B, recommend C | selection on B, recommendation on C if supported, original receipt on A | B/C receives A's effect; camera follows result automatically |
| receipt has authorized player scope, no Agent | keep player-scoped receipt in shell, no forced map marker | attach receipt to selected/first Agent |
| accepted/queued/ack | waiting for world confirmation | completed, green reward, delta fabricated from local time |
| completed with no progress | explicit no-progress reason/next step | unconditional success/progress |
| unknown original outcome | original-operation lookup/recovery | retry as a new command without authority |
| audience/permission revoked | clear old private canvas/hover/search/DOM before new scope | preserve old private frame as last-known |

World Feed remains ambient, even if busy or LIVE. LIVE does not prove causal progress. `position_source`, freshness, relation authority and receipt association are four independent facts, not one green health badge.

<a id="bevy-recovery-handoff"></a>
### 13.6. RendererUnavailable and recovery composition

Production world graphics remain Bevy/WASM only. A static design mockup is not a JS fallback renderer. Failure sheet replaces the world drawing with a quiet unavailable region labeled **Renderer Unavailable / 渲染器不可用**, explains the affected capability and available recovery, retains still-authorized Next Move/context, Targets, original receipt and safe controls. Diagnostics is available but not the dominant card. No decorative fake map behind an error pretending the world remains live.

Distinguish initial GPU/resource absence, renderer loss after genuine ready, query/network unavailability and authorization loss. On post-ready loss stop stale canvas hit targets and animation; if a prior frame is retained it is explicitly non-live and only while still authorized. Retry concerns renderer/session reconstruction, not replay of world action. Recovery reapplies current authorized view/camera/selection, validates render and hit state, and never restores revoked pixels. A successful probe alone is not recovered interaction. Original operation/result identity survives where authorized; a renderer error does not prove world execution stopped.

<a id="bevy-design-state-matrix"></a>
### 13.7. Required design-state/acceptance matrix

B01–B13 below are **new local design-review case IDs**, not existing automated test names or new product requirements. All remain **design targets/unexecuted** for this design revision. PW-* references name existing manual methods, not evidence that those methods have run. Retain source commit, fixture, viewport, CSS/framebuffer/DPR, camera, language, motion preference, renderer state and original operation identity in evidence metadata. Static diagrams prove proposed composition only.

| ID | Static sheet/state | Acceptance question | Implementation evidence needed later |
| --- | --- | --- | --- |
| B01 | sparse, current exact positions | Can player identify Next Move, relevant Agent and honest absent relationship without extra dashboard noise? | PW-READABILITY actual method; real browser screenshot |
| B02 | selected B, pending A, recommended C | Are all three subjects distinct, with original receipt fixed to A? | PW-IDENTITY Rust/host + actual input sequence |
| B03 | all cues on A, blocked receipt | Is amber selection distinct from blocked X and suppressed recommendation explained in context? | cue tests, actual-scale GPU crop |
| B04 | partial/derived/missing/stale | Are geometry source and freshness separately readable, counts scoped and unsupported lines absent? | PW-PROJECTION + PW-FRESHNESS input/DTO/browser |
| B05 | dense same-coordinate objects | Can every authorized target still be chosen via Targets despite label suppression? | real keyboard/touch selection; no markup-only pass |
| B06 | 99/100/101 eligible labels; long CJK | Does global cap preserve priority and full text route without orphan backing or clipping? | label-budget tests + real fonts/browser |
| B07 | footprint 1.49/1.5/9.99/10px | Are terrain states correctly bounded and noninteractive, actors foremost? | LOD helper/ECS/software raster/GPU separately |
| B08 | desktop/mobile/tablet/landscape | Do HUD/dock/context leave useful world space and visible original receipt/blocker? | actual screenshots, focus/overflow/touch evidence |
| B09 | normal/reduced motion; refresh while exploring | Does meaning persist without motion or repeated focus stealing? | motion preference + actual input/browser |
| B10 | initial renderer unavailable | Does legal text/Targets/recovery remain without second map or implied live graphics? | PW-RECOVERY initial-failure branch |
| B11 | genuine ready → GPU loss → rebuilt | Are stale hits gone and authorized view restored without replaying command? | true post-ready failure/recovery, not GPU-disabled startup |
| B12 | audience revoked while selected | Are old private imagery, labels, hover and search cleared rather than tagged stale? | authorization/cleanup trace, privacy-safe screenshots |
| B13 | no receipt/pending/advanced/no-progress/blocked/unknown | Does text state what changed or did not, why and next, with no Feed substitution? | original authority and receipt state evidence |

Stop-ship visual findings for later implementation: terrain louder than relevant actor; actionable-looking fragments; clipped Next Move/blocker/receipt; color-only meaning; fabricated route/progress; hidden provenance; unreachable overlapping target; stale canvas interaction; revoked imagery; or independent JS world fallback. Screenshot/raster/DOM/unit/build evidence remain separate from real GPU, actual interaction, human comprehension and release approval.

<a id="bevy-static-design-references"></a>
### 13.8. 静态设计参照

以下SVG为版本化 **DESIGN TARGET / 设计目标**，仅说明构图、层级与状态区分，不是当前运行截图、Bevy渲染输入或独立JS fallback。示意数据不构成世界事实。缩略图不替代实际显示尺寸、字体、GPU或输入验证。完整Shell页面/模块合同仍见 [详细页面交接](viewer-page-module-design-2026-06-18.design.md#detailed-uiux-handoff)。

| 参照 | 本节阅读义务 | 不证明范围 |
| --- | --- | --- |
| [Desktop world first](assets/ui-ux/01-desktop-world-first.svg) | 单一Next Move、世界主舞台、从属Objective/Leverage | 当前浏览器实现或五秒真人理解 |
| [Narrow world/targets](assets/ui-ux/02-narrow-world-targets.svg) | 窄屏主区优先、可访问Targets入口 | 当前触控/键盘命中 |
| [Panels, IME and focus](assets/ui-ux/03-panels-ime-focus.svg) | 世界观察与Command输入隔离 | 真实IME和焦点顺序 |
| [Receipt/selection identity](assets/ui-ux/04-receipts-selection-identity.svg) | A原操作、B选中、推荐目标独立 | Runtime授权、原结果因果 |
| [Bevy layers, LOD and provenance](assets/ui-ux/05-bevy-layers-lod-provenance.svg) | 层次、屏幕LOD、来源/时效区分 | GPU像素、完整semantic zoom或性能 |
| [Renderer recovery and privacy](assets/ui-ux/06-renderer-recovery-privacy.svg) | 无第二世界、旧命中禁用、撤权清理 | 真实ready后失效/恢复 |
| [Low-height layout](assets/ui-ux/07-low-height-layout.svg) | HUD/dock/receipt不遮唯一解释 | 所有viewport与缩放组合 |
| [Tokens, components and focus](assets/ui-ux/08-tokens-components-focus.svg) | 语义token、状态形状、可见焦点 | Rust/CSS token已统一或a11y认证 |

<a id="bevy-source-reference"></a>
### 13.9. 当前源码参照与未决风险

- [render.rs](../../../crates/pixel_world_bridge/src/render.rs)：z层、尺寸、pulse和reduced-motion相位。
- [render_label_budget.rs](../../../crates/pixel_world_bridge/src/render_label_budget.rs)、[render_agent_labels.rs](../../../crates/pixel_world_bridge/src/render_agent_labels.rs)、[render_location_labels.rs](../../../crates/pixel_world_bridge/src/render_location_labels.rs)、[render_map_labels.rs](../../../crates/pixel_world_bridge/src/render_map_labels.rs)：预算、截断、优先级与碰撞。
- [selected Agent](../../../crates/pixel_world_bridge/src/render_selected_agent_cue.rs)、[selected Location](../../../crates/pixel_world_bridge/src/render_selected_location_cue.rs)、[recommended target](../../../crates/pixel_world_bridge/src/render_recommended_target_cue.rs)、[receipt target](../../../crates/pixel_world_bridge/src/render_receipt_target_cue.rs)：独立主体视觉标记。
- [derived position](../../../crates/pixel_world_bridge/src/render_agent_position_provenance_cue.rs)、[missing position](../../../crates/pixel_world_bridge/src/render_agent_position_missing_cue.rs)：来源表达而非关系权威。
- [camera fit](../../../crates/pixel_world_bridge/src/render_camera_fit.rs)、[camera focus](../../../crates/pixel_world_bridge/src/render_camera_focus.rs)、[canvas resize](../../../crates/pixel_world_bridge/src/render_canvas_resize.rs)：既有视口机制，不证明HUD安全区避让。
- [links](../../../crates/pixel_world_bridge/src/render_links.rs)、[fragment visuals](../../../crates/pixel_world_bridge/src/render_fragment_visuals.rs)：有效关系与非交互terrain。

品牌值沿 [brand system](viewer-brand-system-2026-06-05.design.md)，总体视觉沿 [visual specification](viewer-visual-design-spec-2026-06-05.design.md)，关系权威沿 [semantic positioning](viewer-pixel-world-semantic-positioning.design.md#relation-authority-boundary)，LOD沿 [fragment LOD](viewer-pixel-world-fragment-lod.design.md#terrain-lod-boundary)，分层证据沿 [bridge rendering](viewer-pixel-world-bridge-rendering.design.md#render-reconcile-validation)。

密集场景的真实可选性、CJK实际字体宽度、HUD遮挡、低高度布局、GPU失效恢复与真人理解仍需对应证据；静态图与源码常量不能关闭这些风险。既有原操作关联、权限与source/freshness表达不足时，保留合法文本而不制造图形事实。
