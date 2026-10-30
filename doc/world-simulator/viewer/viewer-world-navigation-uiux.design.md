# Viewer 世界、导航、空间与诊断 UI/UX 设计

- Professional owner: `game_visual_interaction_designer`；Integration owner: `tpm`。
- 状态：专业呈现与交互补充；源证据与目标稿分开。尚未完成的图稿关系保持明确缺口，不是功能已发布声明。
- 固定源：PR #5277 `113868200d1f91bef1008babb689bd257f241428`；产品/Viewer源与审读main `5cc5c2b8a84c22f80cfa9ee16d893eaf6deedb08` 对应subtree相同。
- 配套：[派生目录](viewer-full-function-coverage-catalog.md)、[既有页面总览](viewer-page-module-design-2026-06-18.design.md#detailed-uiux-handoff)。目录不是产品authority。

## 1. 问题、目标与非目标

本设计把已有源的字段、动作、条件及未解决的目标合同分开，使玩家能读清观察对象、当前权限、下一步与原结果。具体引用仅承接表中适用义务；组件存在、图稿数量、accepted和设计覆盖都不等于世界效果或运行验收。

不新增玩法/认证/治理能力，不改变后端权限、报价或结算语义；不代替产品PRD、实际服务capability或真实试玩。

## 2. 上游约束与相关角色

产品语义由各行唯一上游声明拥有；Viewer负责呈现、交互与失败/恢复信息，不接管identity、runtime、world或治理authority。现有品牌和终端层级继续适用。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | 锚点导航；打开目标面板并管理返回焦点，不赋予控制权 | [R-NAV-01](viewer-world-navigation-uiux.design.md#r-nav-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | 锚点导航；打开目标面板并管理返回焦点，不赋予控制权 | [R-NAV-01](viewer-world-navigation-uiux.design.md#r-nav-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | 只打开已有 Diagnostics；不是第四个主玩法页 | [R-NAV-02](viewer-world-navigation-uiux.design.md#r-nav-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | 现有入口和 en/zh；不新增世界选择器或设置中心 | [R-NAV-03](viewer-world-navigation-uiux.design.md#r-nav-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | 现有入口和 en/zh；不新增世界选择器或设置中心 | [R-NAV-03](viewer-world-navigation-uiux.design.md#r-nav-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | entity-search 过滤现有获准对象；空快照/同步中/无匹配分开 | [R-TARGET-01](viewer-world-navigation-uiux.design.md#r-target-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-PERFORMANCE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001)；Trace refs: O7-REQ-7e64b369-3c32-4671-a1af-205bc5a75251 | entity-search 过滤现有获准对象；空快照/同步中/无匹配分开 | [R-TARGET-01](viewer-world-navigation-uiux.design.md#r-target-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | applySelection 仅改变观察目标；对象身份不授予控制权 | [R-TARGET-02](viewer-world-navigation-uiux.design.md#r-target-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-PERFORMANCE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001)；Trace refs: O7-REQ-7e64b369-3c32-4671-a1af-205bc5a75251 | 有限挂载条目，scope 变化重置页；分页不删除源对象或改世界事实 | [R-TARGET-03](viewer-world-navigation-uiux.design.md#r-target-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | 连接、hello、snapshot、auth sync 与可见计数分别说明；不制造 Agent | [R-TARGET-04](viewer-world-navigation-uiux.design.md#r-target-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | 连接、hello、snapshot、auth sync 与可见计数分别说明；不制造 Agent | [R-TARGET-04](viewer-world-navigation-uiux.design.md#r-target-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | lazy disclosure；缺明细/授权收窄/lastError 分开，raw JSON 不成为完整字段承诺 | [R-TARGET-05](viewer-world-navigation-uiux.design.md#r-target-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | lazy disclosure；缺明细/授权收窄/lastError 分开，raw JSON 不成为完整字段承诺 | [R-TARGET-05](viewer-world-navigation-uiux.design.md#r-target-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | world/stage、尺度、连接、目标与反馈分别显示；未知位置不伪造 | [R-WORLD-01](viewer-world-navigation-uiux.design.md#r-world-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | world/stage、尺度、连接、目标与反馈分别显示；未知位置不伪造 | [R-WORLD-01](viewer-world-navigation-uiux.design.md#r-world-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | 推荐只提示；direct actions 仅已映射现有种类；控制与busy gate 必须成立 | [R-WORLD-02](viewer-world-navigation-uiux.design.md#r-world-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | 推荐只提示；direct actions 仅已映射现有种类；控制与busy gate 必须成立 | [R-WORLD-02](viewer-world-navigation-uiux.design.md#r-world-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | 保留原主体/阶段/原因/下一步；观察 B 不把 A 结果改给 B | [R-WORLD-03](viewer-world-navigation-uiux.design.md#r-world-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | 保留原主体/阶段/原因/下一步；观察 B 不把 A 结果改给 B | [R-WORLD-03](viewer-world-navigation-uiux.design.md#r-world-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | 图形就绪/真实 hit target 受 renderer contract 限定；不把装饰位置当权威 | [R-WORLD-04](viewer-world-navigation-uiux.design.md#r-world-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | 图形就绪/真实 hit target 受 renderer contract 限定；不把装饰位置当权威 | [R-WORLD-04](viewer-world-navigation-uiux.design.md#r-world-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | 显式 focus mode；保留 Command/Diagnostics/退出与当前决策上下文 | [R-WORLD-05](viewer-world-navigation-uiux.design.md#r-world-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | 显式 focus mode；保留 Command/Diagnostics/退出与当前决策上下文 | [R-WORLD-05](viewer-world-navigation-uiux.design.md#r-world-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | 按需打开；聊天仍服从当前主体与控制边界，不是第二权限模式 | [R-WORLD-06](viewer-world-navigation-uiux.design.md#r-world-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | 按需打开；聊天仍服从当前主体与控制边界，不是第二权限模式 | [R-WORLD-06](viewer-world-navigation-uiux.design.md#r-world-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | keyboard/hover/focus restoration；热点仅描述获准对象 | [R-WORLD-07](viewer-world-navigation-uiux.design.md#r-world-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | keyboard/hover/focus restoration；热点仅描述获准对象 | [R-WORLD-07](viewer-world-navigation-uiux.design.md#r-world-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | authoritative projection 与同锚点偏移；选择不产生世界动作 | [R-WORLD-08](viewer-world-navigation-uiux.design.md#r-world-08) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | 关系必须有当前 runtime authority；fixture overlay 与正式世界绘制分开 | [R-WORLD-09](viewer-world-navigation-uiux.design.md#r-world-09) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | 关系必须有当前 runtime authority；fixture overlay 与正式世界绘制分开 | [R-WORLD-09](viewer-world-navigation-uiux.design.md#r-world-09) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | world_view capability；complete/partial/unknown、budget_limited 和 source_frozen 显示 | [R-SPACE-01](viewer-world-navigation-uiux.design.md#r-space-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | world_view capability；complete/partial/unknown、budget_limited 和 source_frozen 显示 | [R-SPACE-01](viewer-world-navigation-uiux.design.md#r-space-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | 只有当前获准可信位置才能 locate/follow；失去位置自动停止跟随 | [R-SPACE-02](viewer-world-navigation-uiux.design.md#r-space-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | 只有当前获准可信位置才能 locate/follow；失去位置自动停止跟随 | [R-SPACE-02](viewer-world-navigation-uiux.design.md#r-space-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | geometry_source 与 freshness；无可信位置不提供地图定位 | [R-SPACE-03](viewer-world-navigation-uiux.design.md#r-space-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | 现有关系与有效端点；route inspection 不提交移动/转运 | [R-SPACE-04](viewer-world-navigation-uiux.design.md#r-space-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | 授权 aggregate/member_count；范围汇总不代表拥有或可操作 | [R-SPACE-05](viewer-world-navigation-uiux.design.md#r-space-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | event source_cut/world/reorg 与可信位置匹配；非完整历史 | [R-SPACE-06](viewer-world-navigation-uiux.design.md#r-space-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | event source_cut/world/reorg 与可信位置匹配；非完整历史 | [R-SPACE-06](viewer-world-navigation-uiux.design.md#r-space-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | world feed 是环境上下文，不是个人行动结果；状态区分 replay/reorg/gap/unavailable | [R-FEED-01](viewer-world-navigation-uiux.design.md#r-feed-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | world feed 是环境上下文，不是个人行动结果；状态区分 replay/reorg/gap/unavailable | [R-FEED-01](viewer-world-navigation-uiux.design.md#r-feed-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | gap 重载与 unavailable 重试分开；不改 Live/Playback profile、不补造成功 | [R-FEED-02](viewer-world-navigation-uiux.design.md#r-feed-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | 仅 explicit receipt_ref 链接回执；模块需当前授权实体解析成功 | [R-FEED-03](viewer-world-navigation-uiux.design.md#r-feed-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | 仅 explicit receipt_ref 链接回执；模块需当前授权实体解析成功 | [R-FEED-03](viewer-world-navigation-uiux.design.md#r-feed-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-PERFORMANCE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-002)；Trace refs: O7-REQ-25030f5c-c246-4b72-b637-de9dc6b520ae | descriptor/download/verify/compile/prepare/ready 分层；retry graphics 不重发玩法动作 | [R-DIAG-01](viewer-world-navigation-uiux.design.md#r-diag-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | descriptor/download/verify/compile/prepare/ready 分层；retry graphics 不重发玩法动作 | [R-DIAG-01](viewer-world-navigation-uiux.design.md#r-diag-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | 原始诊断次级；DTO 动态字段不声称枚举穷尽；scope 变化清理旧私有图形 | [R-DIAG-02](viewer-world-navigation-uiux.design.md#r-diag-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | 原始诊断次级；DTO 动态字段不声称枚举穷尽；scope 变化清理旧私有图形 | [R-DIAG-02](viewer-world-navigation-uiux.design.md#r-diag-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | phase-1 期望 contract 与 provider actual check 分列 | [R-DIAG-03](viewer-world-navigation-uiux.design.md#r-diag-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | account/session/admission/runtime/control 各自状态，不合并成已完成 | [R-DIAG-04](viewer-world-navigation-uiux.design.md#r-diag-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | action capability/reason 与 prompt/chat feedback 分开；事件不代签本人动作 | [R-DIAG-05](viewer-world-navigation-uiux.design.md#r-diag-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | action capability/reason 与 prompt/chat feedback 分开；事件不代签本人动作 | [R-DIAG-05](viewer-world-navigation-uiux.design.md#r-diag-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | server-validated capability，pending/refused/expired/revoked 分开；失败关闭不变权威 | [R-DIRECTOR-01](viewer-world-navigation-uiux.design.md#r-director-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | server-validated capability，pending/refused/expired/revoked 分开；失败关闭不变权威 | [R-DIRECTOR-01](viewer-world-navigation-uiux.design.md#r-director-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | 显式展开；外点或 Escape 关闭，说明不是可执行世界按钮 | [R-NAV-04](viewer-world-navigation-uiux.design.md#r-nav-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-006](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-006)；Trace refs: O7-REQ-6f348a2d-cb87-4838-8447-21c8eebf6d07 | Escape 先关抽屉再退出；IME composing/229 不截断输入，焦点返回原 invoker | [R-WORLD-10](viewer-world-navigation-uiux.design.md#r-world-10) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | Escape 先关抽屉再退出；IME composing/229 不截断输入，焦点返回原 invoker | [R-WORLD-10](viewer-world-navigation-uiux.design.md#r-world-10) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | 动态生成路径由 finalize-software-safe-build.mjs 将此源复制至 webgl2/pixel_world_bridge.js；pointer/click/drag/wheel 只委托 WASM 图形交互，不提交世界动作；生成产物和运行未验证 | [R-WORLD-11](viewer-world-navigation-uiux.design.md#r-world-11) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | 动态生成路径由 finalize-software-safe-build.mjs 将此源复制至 webgl2/pixel_world_bridge.js；pointer/click/drag/wheel 只委托 WASM 图形交互，不提交世界动作；生成产物和运行未验证 | [R-WORLD-11](viewer-world-navigation-uiux.design.md#r-world-11) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |

## 3. 当前状态、目标状态与差距

`implemented_in_frozen_source` 只表示固定源码有实际消费者；条件显示不是未实现。`target_only` 只表示已有产品义务的设计目标；未确认专用Viewer入口不证明全仓后端不存在。QA与禁用占位不进入普通玩家当前功能分母。图中的示例数值不是真实世界结果。

## 4. 边界与结构

World / Targets / Command 是三个主Dock；Search在Targets内，Diagnostics/More为次级。Bevy是唯一世界renderer；图稿占位不是JS世界回退。当前主体、观察目标、模型连接、账号、session、admission、control分别表述。

## 5. 关键交互流程

先读对象与可用条件，再编辑草稿/请求预估，只有实际capability允许时才显式提交。pending/unknown保留原操作身份；切换观察对象不改写原回执。关闭弹层返回invoker，焦点/IME不得误触提交。具体feature条款优先于此共同顺序。

## 6. 字段、动作与具体表面

<a id="r-nav-01"></a>
### R-NAV-01 World / Targets / Command 路由与返回焦点

- 固定来源：[viewer_navigation.jsx:1–80](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_navigation.jsx#L1-L80)。
- 实现/条件：`current`；锚点导航；打开目标面板并管理返回焦点，不赋予控制权。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg#dock)；区域：dock。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W09-narrow-world.svg#dock)；区域：dock。
  - 已画状态：desktop dock, W09 true narrow。
  - 字段：hash, focus target。
  - 动作：World, Targets, Command。
  - 未证明：Escape focus transitions not runtime verified；真实运行未执行。

<a id="r-nav-02"></a>
### R-NAV-02 More 次级诊断入口

- 固定来源：[viewer_navigation.jsx:81–108](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_navigation.jsx#L81-L108)。
- 实现/条件：`current`；只打开已有 Diagnostics；不是第四个主玩法页。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg#more)；区域：more。
  - 已画状态：More entry。
  - 字段：diagnosticsOpen。
  - 动作：More。
  - 未证明：collapsed diagnostics summary text-only；真实运行未执行。

<a id="r-nav-03"></a>
### R-NAV-03 入口菜单、locale 与账号安全披露


**源核对修正**：当前Entry为native details/summary；没有任意关闭后的显式焦点恢复实现，不将该提议当当前事实。

- 固定来源：[viewer_app.jsx:854–904](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L854-L904)。
- 实现/条件：`current`；现有入口和 en/zh；不新增世界选择器或设置中心。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/10-entry-gate-focus.svg#entry-menu)；精确区域 `entry-menu`。
  - entry-menu：字段 locale zh/en；localeCode badge；softwareSafeUrl text；account security details；动作 native details toggle；setViewerLocale；状态规范 current locale button disabled；account details only hosted_public_join and auth.available。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-target-01"></a>
### R-TARGET-01 授权 Targets 搜索与分类计数

- 固定来源：[viewer_app.jsx:2160–2261](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2160-L2261)。
- 实现/条件：`current`；entity-search 过滤现有获准对象；空快照/同步中/无匹配分开。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg#targets)；区域：targets。
  - 已画状态：location selected。
  - 字段：getSelectedSearch, lists.agents, lists.locations。
  - 动作：filter。
  - 未证明：Agent display owned by Agent author；真实运行未执行。

<a id="r-target-02"></a>
### R-TARGET-02 Agent、地点和模块选择

- 固定来源：[viewer_app.jsx:2262–2339](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2262-L2339)。
- 实现/条件：`current`；applySelection 仅改变观察目标；对象身份不授予控制权。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg#targets)；区域：targets。
  - 已画状态：selected location。
  - 字段：location.name/id, profile.radius_cm, resources, selectedKind/id。
  - 动作：applySelection。
  - 未证明：module existing details in W04; Agent display delegated；真实运行未执行。

<a id="r-target-03"></a>
### R-TARGET-03 分页窗口与完整授权搜索

- 固定来源：[target_list_window.jsx:1–52](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/target_list_window.jsx#L1-L52)。
- 实现/条件：`current`；有限挂载条目，scope 变化重置页；分页不删除源对象或改世界事实。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg#targets)；区域：targets。
  - 已画状态：first of multiple pages。
  - 字段：PAGE_SIZE=50, filterKey, scopeKey, page, items.length。
  - 动作：Previous, Next。
  - 未证明：last-page and scope revocation text-only；真实运行未执行。

<a id="r-target-04"></a>
### R-TARGET-04 空实体与同步恢复指引

- 固定来源：[viewer_app.jsx:55–140](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L55-L140)。
- 实现/条件：`conditional_current`；连接、hello、snapshot、auth sync 与可见计数分别说明；不制造 Agent。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W18-targets-inspection.svg#pending)；精确区域 `pending`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W18-targets-inspection.svg#empty)；精确区域 `empty`。
  - 字段：hasSnapshot；loading progress；blockerKind。
  - 动作：runtime-published recovery only。
  - 已画状态：pending；empty。
  - 未证明：All unpictured input/state permutations；Runtime and 200% accessibility verification。

<a id="r-target-05"></a>
### R-TARGET-05 原始对象、模块与快照明细

- 固定来源：[viewer_app.jsx:3678–3846](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L3678-L3846)。
- 实现/条件：`current`；lazy disclosure；缺明细/授权收窄/lastError 分开，raw JSON 不成为完整字段承诺。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`text_spec_only`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W04-nonagent-inspectors.svg#module)；区域：module；仅参考图，不计此义务视觉覆盖。
  - 已画状态：module/Location schema。
  - 字段：module_id, kind, label, anchor, snapshot.config, counts, metrics, hostedAccess。
  - 动作：Expand raw object, Expand raw snapshot diagnostics。
  - 未证明：raw JSON open and world-scale diagnostic fields not fully drawn；真实运行未执行。

<a id="r-world-01"></a>
### R-WORLD-01 世界舞台身份、尺度与连接摘要

- 固定来源：[viewer_app.jsx:1951–2159](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L1951-L2159)。
- 实现/条件：`current`；world/stage、尺度、连接、目标与反馈分别显示；未知位置不伪造。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg#stage)；区域：stage。
  - 已画状态：illustrative selection。
  - 字段：selectedKind, selectedId, connectionStatus, worldScaleSurface。
  - 动作：select, return to target。
  - 未证明：full identity/connection variants text-only；真实运行未执行。

<a id="r-world-02"></a>
### R-WORLD-02 单一 Next Move / Objective / Leverage HUD

- 固定来源：[pixel_world_host.jsx:303–444](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L303-L444)。
- 实现/条件：`current`；推荐只提示；direct actions 仅已映射现有种类；控制与busy gate 必须成立。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W16-hud-gates.svg#available)；精确区域 `available`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W16-hud-gates.svg#pending)；精确区域 `pending`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W16-hud-gates.svg#navigation)；精确区域 `navigation`。
  - 字段：commercial_surface.objective；next_action；blocker。
  - 已画状态：available；pending；navigation。
  - 未证明：All unpictured input/state permutations；Runtime and 200% accessibility verification。

<a id="r-world-03"></a>
### R-WORLD-03 原行动独立回执


**源核对修正**：当前Receipt无独立返回原目标按钮，也无next_step字段；只映射真实已显示字段与主体/阶段，不按旧摘要造控件。

- 固定来源：[pixel_world_host.jsx:218–273](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L218-L273)。
- 实现/条件：`current`；保留原主体/阶段/原因/下一步；观察 B 不把 A 结果改给 B。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W17-original-receipt.svg#observation)；精确区域 `observation`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W17-original-receipt.svg#waiting)；精确区域 `waiting`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W17-original-receipt.svg#accepted)；精确区域 `accepted`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W17-original-receipt.svg#effect)；精确区域 `effect`。
  - 字段：action_receipt。
  - 已画状态：observation；waiting；accepted；effect。
  - 未证明：All unpictured input/state permutations；Runtime and 200% accessibility verification。

<a id="r-world-04"></a>
### R-WORLD-04 Bevy canvas、命中与输入桥接

- 固定来源：[pixel_world_host.jsx:39–217](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L39-L217)。
- 实现/条件：`current`；图形就绪/真实 hit target 受 renderer contract 限定；不把装饰位置当权威。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W10-renderer-states.svg#ready)；区域：ready。
  - 已画状态：ready region。
  - 字段：renderState, cameraState, selection。
  - 动作：canvas input。
  - 未证明：real Bevy GPU/hit behavior not executed；真实运行未执行。

<a id="r-world-05"></a>
### R-WORLD-05 电影视图、最大化与退出

- 固定来源：[pixel_world_host.jsx:445–591](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L445-L591)。
- 实现/条件：`current`；显式 focus mode；保留 Command/Diagnostics/退出与当前决策上下文。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W12-cinematic.svg#hud)；区域：hud。
  - 已画状态：expanded control fragment。
  - 字段：objective, progress_percent, world_read.tick, blocker。
  - 动作：Command & Target, Exit Cinematic, World Status, Maximize/Restore alternate。
  - 未证明：opening banner and each cinematic breakpoint not fully drawn；真实运行未执行。

<a id="r-world-06"></a>
### R-WORLD-06 聚焦小地图、Command 与诊断抽屉

- 固定来源：[pixel_world_host.jsx:592–831](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L592-L831)。
- 实现/条件：`current`；按需打开；聊天仍服从当前主体与控制边界，不是第二权限模式。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W12-cinematic.svg#mission-map)；区域：mission-map。
  - 已画状态：drawer open, minimap illustrative。
  - 字段：selection, active_agent_id, locations。
  - 动作：drawer disclosure。
  - 未证明：World Status drawer body text-only；真实运行未执行。

<a id="r-world-07"></a>
### R-WORLD-07 热点焦点、键盘与 tooltip

- 固定来源：[pixel_world_hotspot.jsx:1–184](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_hotspot.jsx#L1-L184)。
- 实现/条件：`current`；keyboard/hover/focus restoration；热点仅描述获准对象。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W13-hotspots.svg#tooltip)；区域：tooltip。
  - 已画状态：tooltip open, candidate buttons。
  - 字段：kind, label, hotspot identity。
  - 动作：inspect, close tooltip, Escape。
  - 未证明：all kinds and collision placements not fully drawn；真实运行未执行。

<a id="r-world-08"></a>
### R-WORLD-08 Renderer 原生对象命中目标

- 固定来源：[pixel_world_renderer_targets.jsx:1–125](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_renderer_targets.jsx#L1-L125)。
- 实现/条件：`current`；authoritative projection 与同锚点偏移；选择不产生世界动作。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W13-hotspots.svg#overlap)；区域：overlap。
  - 已画状态：candidate selection。
  - 字段：kind, id, pos, aria-pressed。
  - 动作：select authorized object。
  - 未证明：all native target hit states runtime unknown；真实运行未执行。

<a id="r-world-09"></a>
### R-WORLD-09 视觉图层、图例及稀疏世界说明

- 固定来源：[pixel_world_visual_clarity.jsx:1–219](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_visual_clarity.jsx#L1-L219)。
- 实现/条件：`current`；关系必须有当前 runtime authority；fixture overlay 与正式世界绘制分开。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W20-legend-sparse.svg#legend)；精确区域 `legend`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W20-legend-sparse.svg#sparse)；精确区域 `sparse`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W20-legend-sparse.svg#layers)；精确区域 `layers`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W20-legend-sparse.svg#association)；精确区域 `association`。
  - 字段：fragment_terrain；links；goal_highlight；blocker_highlight。
  - 已画状态：legend；sparse；layers；association。
  - 未证明：All unpictured input/state permutations；Runtime and 200% accessibility verification。

<a id="r-space-01"></a>
### R-SPACE-01 获准空间搜索、总览和返回主目标

- 固定来源：[pixel_world_world_view_explorer.jsx:6–52](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_world_view_explorer.jsx#L6-L52)。
- 实现/条件：`conditional_current`；world_view capability；complete/partial/unknown、budget_limited 和 source_frozen 显示。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W02-spatial-states.svg#loading)；区域：loading。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W02-spatial-states.svg#partial)；区域：partial。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W14-search-empty.svg#authorized-search)；区域：authorized-search。
  - 已画状态：W02 loading and partial, search form。
  - 字段：state.view, loading, stale, completeness, budget_limited, source_frozen, notice。
  - 动作：Search, World overview, Return to target。
  - 未证明：target unavailable notice text-only; full stale query replacement not drawn；真实运行未执行。

<a id="r-space-02"></a>
### R-SPACE-02 对象检查、定位、区域探索和跟随

- 固定来源：[pixel_world_world_view_explorer.jsx:53–68](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_world_view_explorer.jsx#L53-L68)。
- 实现/条件：`conditional_current`；只有当前获准可信位置才能 locate/follow；失去位置自动停止跟随。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W02-spatial-states.svg#current)；区域：current。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W02-spatial-states.svg#derived)；区域：derived。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W02-spatial-states.svg#stale)；区域：stale。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W02-spatial-states.svg#conflict)；区域：conflict。
  - 已画状态：current Agent, derived Facility, stale, conflict, loading, partial。
  - 字段：entity.kind, entity_id, provenance, position, world_bounds。
  - 动作：select, Locate, Explore region, Follow。
  - 未证明：following pressed and expired variants specified but not separately drawn；真实运行未执行。

<a id="r-space-03"></a>
### R-SPACE-03 只读对象位置与来源检查

- 固定来源：[pixel_world_world_view_inspector.jsx:1–15](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_world_view_inspector.jsx#L1-L15)。
- 实现/条件：`conditional_current`；geometry_source 与 freshness；无可信位置不提供地图定位。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W18-targets-inspection.svg#trusted)；精确区域 `trusted`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W18-targets-inspection.svg#derived)；精确区域 `derived`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W18-targets-inspection.svg#unknown)；精确区域 `unknown`。
  - 字段：label；entity_id；provenance.geometry_source；provenance.freshness；x_cm；y_cm；z_cm。
  - 动作：read-only inspect。
  - 已画状态：trusted；derived；unknown。
  - 未证明：All unpictured input/state permutations；Runtime and 200% accessibility verification。

<a id="r-space-04"></a>
### R-SPACE-04 授权路线与端点检查

- 固定来源：[pixel_world_world_view_inspector.jsx:16–40](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_world_view_inspector.jsx#L16-L40)。
- 实现/条件：`conditional_current`；现有关系与有效端点；route inspection 不提交移动/转运。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W03-routes-events.svg#route)；区域：route。
  - 已画状态：one current endpoint, one unavailable endpoint。
  - 字段：relation_id, kind, status, freshness, route_count, from, to。
  - 动作：Inspect endpoint, Locate endpoint。
  - 未证明：both-available/both-unavailable and all relation kinds text-only；真实运行未执行。

<a id="r-space-05"></a>
### R-SPACE-05 工业区域聚合与下钻

- 固定来源：[pixel_world_world_view_evolution.jsx:1–19](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_world_view_evolution.jsx#L1-L19)。
- 实现/条件：`conditional_current`；授权 aggregate/member_count；范围汇总不代表拥有或可操作。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W03-routes-events.svg#aggregate)；区域：aggregate。
  - 已画状态：area summary。
  - 字段：aggregate_id, label, member_count, provenance.freshness。
  - 动作：select, Explore area。
  - 未证明：aggregate missing/expired text-only；真实运行未执行。

<a id="r-space-06"></a>
### R-SPACE-06 近期空间事件与 Feed 定位

- 固定来源：[pixel_world_world_view_evolution.jsx:20–36](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_world_view_evolution.jsx#L20-L36)。
- 实现/条件：`conditional_current`；event source_cut/world/reorg 与可信位置匹配；非完整历史。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W03-routes-events.svg#event)；区域：event。
  - 已画状态：untrusted location disabled。
  - 字段：event_id, label, logical_time, freshness, position, source_class, source_cut, identity.world_id, identity.reorg_epoch。
  - 动作：Locate event, View world feed。
  - 未证明：trusted Locate-enabled event and specific absent-event notice text-only；真实运行未执行。

<a id="r-feed-01"></a>
### R-FEED-01 World Feed 分类、新鲜度与事件披露

- 固定来源：[world_feed_panel.jsx:1–174](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/world_feed_panel.jsx#L1-L174)。
- 实现/条件：`current`；world feed 是环境上下文，不是个人行动结果；状态区分 replay/reorg/gap/unavailable。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg#loading)；区域：loading。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg#empty)；区域：empty。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg#ready)；区域：ready。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg#replay)；区域：replay。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg#gap)；区域：gap。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg#unavailable)；区域：unavailable。
  - 已画状态：loading, empty, ready, replay, gap, unavailable。
  - 字段：status, events, worldId, reorgEpoch, event_seq, summary, kind, major_event。
  - 动作：fold/unfold。
  - 未证明：major-event category/lifecycle/severity permutations not separately drawn；真实运行未执行。

<a id="r-feed-02"></a>
### R-FEED-02 重载权威快照与重试 Feed

- 固定来源：[world_feed_panel.jsx:175–197](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/world_feed_panel.jsx#L175-L197)。
- 实现/条件：`conditional_current`；gap 重载与 unavailable 重试分开；不改 Live/Playback profile、不补造成功。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg#gap)；区域：gap。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg#unavailable)；区域：unavailable。
  - 已画状态：gap reload, unavailable retry。
  - 字段：snapshotReloadRequired, gapReason, unavailableReason。
  - 动作：Reload authoritative snapshot, Retry World Feed。
  - 未证明：other snapshotReloadRequired combinations text-only；真实运行未执行。

<a id="r-feed-03"></a>
### R-FEED-03 Feed 事件回执引用和模块定位

- 固定来源：[world_feed_panel.jsx:198–272](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/world_feed_panel.jsx#L198-L272)。
- 实现/条件：`conditional_current`；仅 explicit receipt_ref 链接回执；模块需当前授权实体解析成功。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg#ready)；区域：ready。
  - 已画状态：both conditional actions supplied。
  - 字段：receipt_ref, resolved module.id。
  - 动作：View explicit receipt, Locate module。
  - 未证明：absent receipt/module hide states text-only；真实运行未执行。

<a id="r-diag-01"></a>
### R-DIAG-01 资源下载/校验/准备与 renderer 恢复

- 固定来源：[pixel_world_host.jsx:1197–1324](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L1197-L1324)。
- 实现/条件：`current`；descriptor/download/verify/compile/prepare/ready 分层；retry graphics 不重发玩法动作。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W10-renderer-states.svg#loading)；区域：loading。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W10-renderer-states.svg#ready)；区域：ready。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W10-renderer-states.svg#unavailable)；区域：unavailable。
  - 已画状态：loading, ready, unavailable。
  - 字段：rendererStatus, rendererFatal.code/message, asset state。
  - 动作：Retry Renderer。
  - 未证明：download/check progress and retry race text-only；真实运行未执行。

<a id="r-diag-02"></a>
### R-DIAG-02 Renderer 诊断与按需 Render DTO

- 固定来源：[pixel_world_host.jsx:1347–1438](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L1347-L1438)。
- 实现/条件：`current`；原始诊断次级；DTO 动态字段不声称枚举穷尽；scope 变化清理旧私有图形。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W10-renderer-states.svg#renderer-diagnostic)；区域：renderer-diagnostic。
  - 已画状态：disclosure expanded。
  - 字段：locations, fragments, agents, tick, links, hotspots, derived_positions, world_bounds, renderer, runtime, zoom, pan, hover。
  - 动作：Reattach Embedded Renderer, Simulate Renderer Fatal。
  - 未证明：raw Render DTO open and fatal-code variants text-only；真实运行未执行。

<a id="r-diag-03"></a>
### R-DIAG-03 执行通道期望与 provider 实测探测

- 固定来源：[viewer_app.jsx:2941–3000](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2941-L3000)。
- 实现/条件：`current`；phase-1 期望 contract 与 provider actual check 分列。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W19-diagnostic-data.svg#lane)；精确区域 `lane`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W19-diagnostic-data.svg#no-agent)；精确区域 `no-agent`。
  - 字段：provider_mode；execution_mode；environment_class；observation_schema_version；action_schema_version；agent_profile；fallback_reason；provider_check_status；provider_check_source；provider_check_fallback_reason；provider_check_error；provider_reported_capabilities；provider_reported_supported_action_sets。
  - 动作：expand raw diagnostics。
  - 已画状态：lane；no-agent。
  - 未证明：All unpictured input/state permutations；Runtime and 200% accessibility verification。

<a id="r-diag-04"></a>
### R-DIAG-04 认证恢复、准入探针和会话阶梯

- 固定来源：[viewer_app.jsx:3001–3119](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L3001-L3119)。
- 实现/条件：`conditional_current`；account/session/admission/runtime/control 各自状态，不合并成已完成。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W07-diagnostics.svg#auth)；区域：auth。
  - 已画状态：guest superseded/player active hosted/strong separate upgrade plane。
  - 字段：guest_session, player_session, strong_auth, registrationStatus, source, runtimeStatus。
  - 动作：Release Player Session。
  - 未证明：auth recovery, admission, rebind owned partly by account author; not full matrix here；真实运行未执行。

<a id="r-diag-05"></a>
### R-DIAG-05 托管动作矩阵、反馈与最近事件


**源核对修正**：当前metric row不保证单列主体ID；反馈与事件分区可读，但不补造源未发布身份。

- 固定来源：[viewer_app.jsx:3120–3188](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L3120-L3188)。
- 实现/条件：`current`；action capability/reason 与 prompt/chat feedback 分开；事件不代签本人动作。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W19-diagnostic-data.svg#matrix)；精确区域 `matrix`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W19-diagnostic-data.svg#feedback)；精确区域 `feedback`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W19-diagnostic-data.svg#events-empty)；精确区域 `events-empty`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W19-diagnostic-data.svg#events-present)；精确区域 `events-present`。
  - 字段：actionId；enabled；code；requiredAuth；availability；reason；capabilityReason。
  - 已画状态：matrix；feedback；events-empty；events-present。
  - 未证明：All unpictured input/state permutations；Runtime and 200% accessibility verification。

<a id="r-director-01"></a>
### R-DIRECTOR-01 申请/退出 Director

- 固定来源：[director_surface.jsx:1–152](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/director_surface.jsx#L1-L152)。
- 实现/条件：`conditional_current`；server-validated capability，pending/refused/expired/revoked 分开；失败关闭不变权威。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W08-director-states.svg#blocked)；区域：blocked。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W08-director-states.svg#pending)；区域：pending。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W08-director-states.svg#denied)；区域：denied。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W08-director-states.svg#unavailable)；区域：unavailable。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W08-director-states.svg#expired)；区域：expired。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W08-director-states.svg#active)；区域：active。
  - 已画状态：production blocked, pending disabled, denied, unavailable, expired/revoked/reconnect combined, validated hypothetical。
  - 字段：mode, status, reason, capability.issuer, expiresAtUnixMs, worldId, logicalTime, eventSeq, selected, agents, locations, events。
  - 动作：Open Director, Exit Director。
  - 未证明：active production issuer absent; distinct loss reasons combined in one fragment；真实运行未执行。

<a id="r-nav-04"></a>
### R-NAV-04 帮助气泡、外点关闭与 Escape


**源核对修正**：InlineHelpTip支持外点与Escape关闭；无显式focus-restoration，不将通用目标合同当当前已实现。

- 固定来源：[viewer_app.jsx:440–487](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L440-L487)。
- 实现/条件：`current`；显式展开；外点或 Escape 关闭，说明不是可执行世界按钮。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W18-targets-inspection.svg#help)；精确区域 `help`。
  - 动作：outside click；Escape。
  - 已画状态：help。
  - 未证明：All unpictured input/state permutations；Runtime and 200% accessibility verification。

<a id="r-world-10"></a>
### R-WORLD-10 电影模式键盘退出与抽屉焦点返回

- 固定来源：[pixel_world_focus_controller.js:1–101](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_focus_controller.js#L1-L101)。
- 实现/条件：`current`；Escape 先关抽屉再退出；IME composing/229 不截断输入，焦点返回原 invoker。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`state_snippet`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W12-cinematic.svg#drawer)；区域：drawer。
  - 已画状态：drawer open。
  - 字段：focusMode, drawerOpen。
  - 动作：Escape, IME ignore, return focus。
  - 未证明：keyboard transitions require runtime verification；真实运行未执行。

<a id="r-world-11"></a>
### R-WORLD-11 画布相机移动、缩放与交互命令边界

- 固定来源：[pixel_world_runtime_module_wasm.js:404–499](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/pixel_world_runtime_module_wasm.js#L404-L499)。
- 实现/条件：`conditional_current`；动态生成路径由 finalize-software-safe-build.mjs 将此源复制至 webgl2/pixel_world_bridge.js；pointer/click/drag/wheel 只委托 WASM 图形交互，不提交世界动作；生成产物和运行未验证。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/world-diagnostics/W12-cinematic.svg#stage)；区域：stage。
  - 已画状态：gesture region reserved。
  - 字段：camera bounds, pointer, wheel。
  - 动作：pan, zoom。
  - 未证明：gesture temporal states text-only; generated WASM not executed；真实运行未执行。


## 7. 状态、事务与持久化

loading、empty、unavailable、permission denied、pending、unknown与明确失败不得混成一个状态。模型/查询成功不等于世界生效。原结果按原主体和请求身份回读，不能因刷新/切换/重连重放在途意图。本设计不增加持久化协议；动态DTO/服务capability未提供的字段或枚举保留未知。

## 8. 安全、权限与隐私

以当前授权为准；权限丢失先停止可执行入口并清理旧主体私有上下文，不把旧图形/反馈留给新账号。凭据只以空字段/掩码示意，图稿不包含真实key、OTP、Cookie或完整私有响应。通用raw JSON不作为获得更多数据的许可。

## 9. 质量、响应式与可访问性

遵循既有brand与visual规范。窄屏/低高度、触控、键盘、焦点回返、读屏语义、IME/reduced-motion须按实际候选验证；静态SVG与文字不证明这些运行行为。错误近字段，busy和禁用原因可读，重要结果不只靠颜色。

## 10. 兼容、迁移与回滚

沿用原14张概览与兼容入口，不移除旧authority锚点。新稿先按具体source/条件接入，不将target-only按钮启用为当前。文档/资产回退只恢复对应呈现规范，不撤销任何外部世界效果；本设计不改变运行合同。

## 11. 验证设计与可追溯性

固定source与静态SVG只支持结构和视觉审读；真实交互、权限执行、跨入口连续性与世界结果仍需按下表验证。测试/手册引用是验证方法，不代表相应场景已经通过。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | [R-NAV-01](viewer-world-navigation-uiux.design.md#r-nav-01) | 锚点导航；打开目标面板并管理返回焦点，不赋予控制权 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-NAV-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | [R-NAV-01](viewer-world-navigation-uiux.design.md#r-nav-01) | 锚点导航；打开目标面板并管理返回焦点，不赋予控制权 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-NAV-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | [R-NAV-02](viewer-world-navigation-uiux.design.md#r-nav-02) | 只打开已有 Diagnostics；不是第四个主玩法页 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-NAV-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | [R-NAV-03](viewer-world-navigation-uiux.design.md#r-nav-03) | 现有入口和 en/zh；不新增世界选择器或设置中心 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-NAV-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | [R-NAV-03](viewer-world-navigation-uiux.design.md#r-nav-03) | 现有入口和 en/zh；不新增世界选择器或设置中心 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-NAV-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | [R-TARGET-01](viewer-world-navigation-uiux.design.md#r-target-01) | entity-search 过滤现有获准对象；空快照/同步中/无匹配分开 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-TARGET-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-PERFORMANCE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001)；Trace refs: O7-REQ-7e64b369-3c32-4671-a1af-205bc5a75251 | [R-TARGET-01](viewer-world-navigation-uiux.design.md#r-target-01) | entity-search 过滤现有获准对象；空快照/同步中/无匹配分开 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-TARGET-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | [R-TARGET-02](viewer-world-navigation-uiux.design.md#r-target-02) | applySelection 仅改变观察目标；对象身份不授予控制权 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-TARGET-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-PERFORMANCE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001)；Trace refs: O7-REQ-7e64b369-3c32-4671-a1af-205bc5a75251 | [R-TARGET-03](viewer-world-navigation-uiux.design.md#r-target-03) | 有限挂载条目，scope 变化重置页；分页不删除源对象或改世界事实 | 对照 [target_list_window.test.jsx](../../../crates/oasis7_viewer/software_safe_src/target_list_window.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-TARGET-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-TARGET-04](viewer-world-navigation-uiux.design.md#r-target-04) | 连接、hello、snapshot、auth sync 与可见计数分别说明；不制造 Agent | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-TARGET-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | [R-TARGET-04](viewer-world-navigation-uiux.design.md#r-target-04) | 连接、hello、snapshot、auth sync 与可见计数分别说明；不制造 Agent | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-TARGET-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [R-TARGET-05](viewer-world-navigation-uiux.design.md#r-target-05) | lazy disclosure；缺明细/授权收窄/lastError 分开，raw JSON 不成为完整字段承诺 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-TARGET-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | [R-TARGET-05](viewer-world-navigation-uiux.design.md#r-target-05) | lazy disclosure；缺明细/授权收窄/lastError 分开，raw JSON 不成为完整字段承诺 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-TARGET-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | [R-WORLD-01](viewer-world-navigation-uiux.design.md#r-world-01) | world/stage、尺度、连接、目标与反馈分别显示；未知位置不伪造 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [R-WORLD-01](viewer-world-navigation-uiux.design.md#r-world-01) | world/stage、尺度、连接、目标与反馈分别显示；未知位置不伪造 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | [R-WORLD-02](viewer-world-navigation-uiux.design.md#r-world-02) | 推荐只提示；direct actions 仅已映射现有种类；控制与busy gate 必须成立 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | [R-WORLD-02](viewer-world-navigation-uiux.design.md#r-world-02) | 推荐只提示；direct actions 仅已映射现有种类；控制与busy gate 必须成立 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | [R-WORLD-03](viewer-world-navigation-uiux.design.md#r-world-03) | 保留原主体/阶段/原因/下一步；观察 B 不把 A 结果改给 B | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | [R-WORLD-03](viewer-world-navigation-uiux.design.md#r-world-03) | 保留原主体/阶段/原因/下一步；观察 B 不把 A 结果改给 B | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | [R-WORLD-04](viewer-world-navigation-uiux.design.md#r-world-04) | 图形就绪/真实 hit target 受 renderer contract 限定；不把装饰位置当权威 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | [R-WORLD-04](viewer-world-navigation-uiux.design.md#r-world-04) | 图形就绪/真实 hit target 受 renderer contract 限定；不把装饰位置当权威 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | [R-WORLD-05](viewer-world-navigation-uiux.design.md#r-world-05) | 显式 focus mode；保留 Command/Diagnostics/退出与当前决策上下文 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | [R-WORLD-05](viewer-world-navigation-uiux.design.md#r-world-05) | 显式 focus mode；保留 Command/Diagnostics/退出与当前决策上下文 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | [R-WORLD-06](viewer-world-navigation-uiux.design.md#r-world-06) | 按需打开；聊天仍服从当前主体与控制边界，不是第二权限模式 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | [R-WORLD-06](viewer-world-navigation-uiux.design.md#r-world-06) | 按需打开；聊天仍服从当前主体与控制边界，不是第二权限模式 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | [R-WORLD-07](viewer-world-navigation-uiux.design.md#r-world-07) | keyboard/hover/focus restoration；热点仅描述获准对象 | 对照 [pixel_world_hotspot.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_hotspot.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | [R-WORLD-07](viewer-world-navigation-uiux.design.md#r-world-07) | keyboard/hover/focus restoration；热点仅描述获准对象 | 对照 [pixel_world_hotspot.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_hotspot.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | [R-WORLD-08](viewer-world-navigation-uiux.design.md#r-world-08) | authoritative projection 与同锚点偏移；选择不产生世界动作 | 对照 [pixel_world_renderer_targets.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_renderer_targets.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-08；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [R-WORLD-09](viewer-world-navigation-uiux.design.md#r-world-09) | 关系必须有当前 runtime authority；fixture overlay 与正式世界绘制分开 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-09；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | [R-WORLD-09](viewer-world-navigation-uiux.design.md#r-world-09) | 关系必须有当前 runtime authority；fixture overlay 与正式世界绘制分开 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-09；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | [R-SPACE-01](viewer-world-navigation-uiux.design.md#r-space-01) | world_view capability；complete/partial/unknown、budget_limited 和 source_frozen 显示 | 对照 [pixel_world_world_view_explorer.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_world_view_explorer.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-SPACE-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [R-SPACE-01](viewer-world-navigation-uiux.design.md#r-space-01) | world_view capability；complete/partial/unknown、budget_limited 和 source_frozen 显示 | 对照 [pixel_world_world_view_explorer.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_world_view_explorer.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-SPACE-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | [R-SPACE-02](viewer-world-navigation-uiux.design.md#r-space-02) | 只有当前获准可信位置才能 locate/follow；失去位置自动停止跟随 | 对照 [pixel_world_world_view_explorer.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_world_view_explorer.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-SPACE-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | [R-SPACE-02](viewer-world-navigation-uiux.design.md#r-space-02) | 只有当前获准可信位置才能 locate/follow；失去位置自动停止跟随 | 对照 [pixel_world_world_view_explorer.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_world_view_explorer.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-SPACE-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [R-SPACE-03](viewer-world-navigation-uiux.design.md#r-space-03) | geometry_source 与 freshness；无可信位置不提供地图定位 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-SPACE-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | [R-SPACE-04](viewer-world-navigation-uiux.design.md#r-space-04) | 现有关系与有效端点；route inspection 不提交移动/转运 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-SPACE-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | [R-SPACE-05](viewer-world-navigation-uiux.design.md#r-space-05) | 授权 aggregate/member_count；范围汇总不代表拥有或可操作 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-SPACE-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [R-SPACE-06](viewer-world-navigation-uiux.design.md#r-space-06) | event source_cut/world/reorg 与可信位置匹配；非完整历史 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-SPACE-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | [R-SPACE-06](viewer-world-navigation-uiux.design.md#r-space-06) | event source_cut/world/reorg 与可信位置匹配；非完整历史 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-SPACE-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [R-FEED-01](viewer-world-navigation-uiux.design.md#r-feed-01) | world feed 是环境上下文，不是个人行动结果；状态区分 replay/reorg/gap/unavailable | 对照 [world_feed_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/world_feed_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-FEED-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-FEED-01](viewer-world-navigation-uiux.design.md#r-feed-01) | world feed 是环境上下文，不是个人行动结果；状态区分 replay/reorg/gap/unavailable | 对照 [world_feed_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/world_feed_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-FEED-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-FEED-02](viewer-world-navigation-uiux.design.md#r-feed-02) | gap 重载与 unavailable 重试分开；不改 Live/Playback profile、不补造成功 | 对照 [world_feed_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/world_feed_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-FEED-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [R-FEED-03](viewer-world-navigation-uiux.design.md#r-feed-03) | 仅 explicit receipt_ref 链接回执；模块需当前授权实体解析成功 | 对照 [world_feed_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/world_feed_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-FEED-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | [R-FEED-03](viewer-world-navigation-uiux.design.md#r-feed-03) | 仅 explicit receipt_ref 链接回执；模块需当前授权实体解析成功 | 对照 [world_feed_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/world_feed_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-FEED-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-PERFORMANCE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-002)；Trace refs: O7-REQ-25030f5c-c246-4b72-b637-de9dc6b520ae | [R-DIAG-01](viewer-world-navigation-uiux.design.md#r-diag-01) | descriptor/download/verify/compile/prepare/ready 分层；retry graphics 不重发玩法动作 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-DIAG-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | [R-DIAG-01](viewer-world-navigation-uiux.design.md#r-diag-01) | descriptor/download/verify/compile/prepare/ready 分层；retry graphics 不重发玩法动作 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-DIAG-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [R-DIAG-02](viewer-world-navigation-uiux.design.md#r-diag-02) | 原始诊断次级；DTO 动态字段不声称枚举穷尽；scope 变化清理旧私有图形 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-DIAG-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | [R-DIAG-02](viewer-world-navigation-uiux.design.md#r-diag-02) | 原始诊断次级；DTO 动态字段不声称枚举穷尽；scope 变化清理旧私有图形 | 对照 [pixel_world_host.test.jsx](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-DIAG-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-DIAG-03](viewer-world-navigation-uiux.design.md#r-diag-03) | phase-1 期望 contract 与 provider actual check 分列 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-DIAG-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-DIAG-04](viewer-world-navigation-uiux.design.md#r-diag-04) | account/session/admission/runtime/control 各自状态，不合并成已完成 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-DIAG-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-DIAG-05](viewer-world-navigation-uiux.design.md#r-diag-05) | action capability/reason 与 prompt/chat feedback 分开；事件不代签本人动作 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-DIAG-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | [R-DIAG-05](viewer-world-navigation-uiux.design.md#r-diag-05) | action capability/reason 与 prompt/chat feedback 分开；事件不代签本人动作 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-DIAG-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | [R-DIRECTOR-01](viewer-world-navigation-uiux.design.md#r-director-01) | server-validated capability，pending/refused/expired/revoked 分开；失败关闭不变权威 | 对照 [director_capability_controller.test.js](../../../crates/oasis7_viewer/software_safe_src/director_capability_controller.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-DIRECTOR-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-DIRECTOR-01](viewer-world-navigation-uiux.design.md#r-director-01) | server-validated capability，pending/refused/expired/revoked 分开；失败关闭不变权威 | 对照 [director_capability_controller.test.js](../../../crates/oasis7_viewer/software_safe_src/director_capability_controller.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-DIRECTOR-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | [R-NAV-04](viewer-world-navigation-uiux.design.md#r-nav-04) | 显式展开；外点或 Escape 关闭，说明不是可执行世界按钮 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-NAV-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-006](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-006)；Trace refs: O7-REQ-6f348a2d-cb87-4838-8447-21c8eebf6d07 | [R-WORLD-10](viewer-world-navigation-uiux.design.md#r-world-10) | Escape 先关抽屉再退出；IME composing/229 不截断输入，焦点返回原 invoker | 对照 [pixel_world_focus_controller.test.js](../../../crates/oasis7_viewer/software_safe_src/pixel_world_focus_controller.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-10；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | [R-WORLD-10](viewer-world-navigation-uiux.design.md#r-world-10) | Escape 先关抽屉再退出；IME composing/229 不截断输入，焦点返回原 invoker | 对照 [pixel_world_focus_controller.test.js](../../../crates/oasis7_viewer/software_safe_src/pixel_world_focus_controller.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-10；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)；Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf | [R-WORLD-11](viewer-world-navigation-uiux.design.md#r-world-11) | 动态生成路径由 finalize-software-safe-build.mjs 将此源复制至 webgl2/pixel_world_bridge.js；pointer/click/drag/wheel 只委托 WASM 图形交互，不提交世界动作；生成产物和运行未验证 | 对照 [pixel_world_runtime_module_wasm.test.js](../../../crates/oasis7_viewer/software_safe_src/pixel_world_runtime_module_wasm.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-11；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | [R-WORLD-11](viewer-world-navigation-uiux.design.md#r-world-11) | 动态生成路径由 finalize-software-safe-build.mjs 将此源复制至 webgl2/pixel_world_bridge.js；pointer/click/drag/wheel 只委托 WASM 图形交互，不提交世界动作；生成产物和运行未验证 | 对照 [pixel_world_runtime_module_wasm.test.js](../../../crates/oasis7_viewer/software_safe_src/pixel_world_runtime_module_wasm.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-WORLD-11；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |

## 12. 决策、长期风险与未决问题

- 图稿仅代表实际映射区域/状态；没有对应frame的项保持text_spec_only，未公布提交合同保持contract_blocked。图稿完备性不得用页面数判定。
- 本主题的具体frame及字段/动作/状态关系见随稿覆盖目录；未画状态明确保留text_spec_only，不能从概览推导。
- 动态DTO、未知action集合、部署能力、原生桌面及真实世界结果链仍待各专业owner在获准环境验证。
- 需求设计关系只覆盖表中适用范围，不自动将产品状态升为已覆盖；未核实的专业协议或目标CRUD保持明确缺口。

