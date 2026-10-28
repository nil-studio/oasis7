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

## 12. 决策、风险与未决范围

<a id="follow-up-brainstorm"></a>
### 12.1 后续候选

viewer-pixel-world-player-leverage-production-readability-2026-05-28.brainstorm.md保留command-lens/production-pulse/pixel-moment候选，不是第二Shell authority；新增数据/交互需更新对应合同，不复活旧PM流程。

<a id="边界决策与风险"></a>
### 12.2 边界和剩余风险

世界主舞台、单一Next Move、单一原因果锚点、Rust派生与host消费不变。风险是上游缺原操作关联、旧DTO不能表达时效、DOM/raster误作GPU、历史进度冒充当前通过。实际整改、独立评审和证据在PR/Task跟踪，本文不能声明关闭。
