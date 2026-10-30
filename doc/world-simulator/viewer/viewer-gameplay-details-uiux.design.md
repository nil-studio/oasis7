# Viewer 当前玩法明细与动态动作 UI/UX 设计

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
| [REQ-FIRST-INDUSTRIAL-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)；Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | 初始 OC、绑定和经济认领分开；pending/credited/首聊可用分别显示，避免重复领取 | [R-GAME-01](viewer-gameplay-details-uiux.design.md#r-game-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | 初始 OC、绑定和经济认领分开；pending/credited/首聊可用分别显示，避免重复领取 | [R-GAME-01](viewer-gameplay-details-uiux.design.md#r-game-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)；Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | 正 liquid OC 或已有 starter claim 记录通过该门禁；defer 不制造/清除经济认领义务 | [R-GAME-02](viewer-gameplay-details-uiux.design.md#r-game-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)；Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf | published rationale、upfront、eligible balance、upkeep runway；理由缺失时不推荐候选 | [R-GAME-03](viewer-gameplay-details-uiux.design.md#r-game-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)；Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf | quote/readability 不等于经济 claim；释放按已有 action 与当前合同 | [R-GAME-04](viewer-gameplay-details-uiux.design.md#r-game-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-AOS-002](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-002)；Trace refs: O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287 | quote/readability 不等于经济 claim；释放按已有 action 与当前合同 | [R-GAME-04](viewer-gameplay-details-uiux.design.md#r-game-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)；Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf | 绑定/认领/runtime session 分开；候选不替换未知原 Agent | [R-GAME-05](viewer-gameplay-details-uiux.design.md#r-game-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | 绑定/认领/runtime session 分开；候选不替换未知原 Agent | [R-GAME-05](viewer-gameplay-details-uiux.design.md#r-game-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce | 只渲染已公布 branch card；机会/成本/承诺与可执行 action 分离 | [R-GAME-06](viewer-gameplay-details-uiux.design.md#r-game-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-GR-001](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001)；Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8 | 只渲染已公布 branch card；机会/成本/承诺与可执行 action 分离 | [R-GAME-06](viewer-gameplay-details-uiux.design.md#r-game-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)；Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3 | what I caused/new option/why continue/waiting cost/recovery 来自 gameplay projection | [R-GAME-07](viewer-gameplay-details-uiux.design.md#r-game-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)；Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | interrupt/reprioritize/correction/handoff 可读，不将意图当 world effect | [R-GAME-08](viewer-gameplay-details-uiux.design.md#r-game-08) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)；Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | interrupt/reprioritize/correction/handoff 可读，不将意图当 world effect | [R-GAME-08](viewer-gameplay-details-uiux.design.md#r-game-08) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)；Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | 首胜目标、玩家动作、世界变化与 leverage 分列 | [R-GAME-09](viewer-gameplay-details-uiux.design.md#r-game-09) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)；Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3 | 首胜目标、玩家动作、世界变化与 leverage 分列 | [R-GAME-09](viewer-gameplay-details-uiux.design.md#r-game-09) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce | 依赖、恢复路径和分享回放是现有摘要，不新建分享/迁移操作 | [R-GAME-10](viewer-gameplay-details-uiux.design.md#r-game-10) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | 依赖、恢复路径和分享回放是现有摘要，不新建分享/迁移操作 | [R-GAME-10](viewer-gameplay-details-uiux.design.md#r-game-10) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)；Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | accepted intent 与 resume anchor 保留，accepted 不等于 applied | [R-GAME-11](viewer-gameplay-details-uiux.design.md#r-game-11) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-IA-002](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)；Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | accepted intent 与 resume anchor 保留，accepted 不等于 applied | [R-GAME-11](viewer-gameplay-details-uiux.design.md#r-game-11) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)；Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | 阶段、cause、当前执行状态来自权威投影，非本地推导成功 | [R-GAME-12](viewer-gameplay-details-uiux.design.md#r-game-12) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-IA-002](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)；Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | blocker/supplement/branch/entityCounts 分层；缺失值不补造 | [R-GAME-13](viewer-gameplay-details-uiux.design.md#r-game-13) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | blocker/supplement/branch/entityCounts 分层；缺失值不补造 | [R-GAME-13](viewer-gameplay-details-uiux.design.md#r-game-13) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-IA-002](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)；Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | wait quote 是机会/风险说明，不自动等待或承诺下个 tick 生效 | [R-GAME-14](viewer-gameplay-details-uiux.design.md#r-game-14) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | wait quote 是机会/风险说明，不自动等待或承诺下个 tick 生效 | [R-GAME-14](viewer-gameplay-details-uiux.design.md#r-game-14) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)；Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | 选项值/风险/受限原因只读；不把普通聊天变成确认替代 | [R-GAME-15](viewer-gameplay-details-uiux.design.md#r-game-15) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-010](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-010)；Trace refs: O7-REQ-3bbcade1-3f73-4899-b324-f857fd27b245 | 选项值/风险/受限原因只读；不把普通聊天变成确认替代 | [R-GAME-15](viewer-gameplay-details-uiux.design.md#r-game-15) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 预览与 actual validated / unlocked 分离 | [R-GAME-16](viewer-gameplay-details-uiux.design.md#r-game-16) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)；Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | 只承接已有经济摘要与 next value，不创建资产交易 | [R-GAME-17](viewer-gameplay-details-uiux.design.md#r-game-17) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008)；Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | 已完成成果、当前能力、后继任务与阻塞各有真值边界 | [R-GAME-18](viewer-gameplay-details-uiux.design.md#r-game-18) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-GR-001](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001)；Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8 | ledger/库存、状态、可用时长依当前 projection；hash 技术证据按需披露 | [R-GAME-19](viewer-gameplay-details-uiux.design.md#r-game-19) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | ledger/库存、状态、可用时长依当前 projection；hash 技术证据按需披露 | [R-GAME-19](viewer-gameplay-details-uiux.design.md#r-game-19) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | 已具备/仍缺输入、生产与交付不混同 | [R-GAME-20](viewer-gameplay-details-uiux.design.md#r-game-20) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-004](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-004)；Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2 | 已具备/仍缺输入、生产与交付不混同 | [R-GAME-20](viewer-gameplay-details-uiux.design.md#r-game-20) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | 支持配方/替代材料/缺口基于 model，不自动排产 | [R-GAME-21](viewer-gameplay-details-uiux.design.md#r-game-21) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 失败原因、投入/电量与恢复 action；按钮仅使用已公布 action | [R-GAME-22](viewer-gameplay-details-uiux.design.md#r-game-22) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | 失败原因、投入/电量与恢复 action；按钮仅使用已公布 action | [R-GAME-22](viewer-gameplay-details-uiux.design.md#r-game-22) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)；Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | 玩家指令、actual event、可归因后果分离；fixture 安装器不是实际证据 | [R-GAME-23](viewer-gameplay-details-uiux.design.md#r-game-23) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)；Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3 | 玩家指令、actual event、可归因后果分离；fixture 安装器不是实际证据 | [R-GAME-23](viewer-gameplay-details-uiux.design.md#r-game-23) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | 仅已公布选项、代价、风险和后继；没有凭 UI 增加恢复协议 | [R-GAME-24](viewer-gameplay-details-uiux.design.md#r-game-24) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | 仅已公布选项、代价、风险和后继；没有凭 UI 增加恢复协议 | [R-GAME-24](viewer-gameplay-details-uiux.design.md#r-game-24) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)；Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | 显式打开/草稿/取消/提交；替换目标不保证下一步执行，in-flight 禁止重复 | [R-GAME-25](viewer-gameplay-details-uiux.design.md#r-game-25) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)；Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | 显式打开/草稿/取消/提交；替换目标不保证下一步执行，in-flight 禁止重复 | [R-GAME-25](viewer-gameplay-details-uiux.design.md#r-game-25) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | 动作来自 current gameplay projection，permission/identity/unknown/pending gate；动态 kind 集合未知 | [R-ACTION-01](viewer-gameplay-details-uiux.design.md#r-action-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | 动作来自 current gameplay projection，permission/identity/unknown/pending gate；动态 kind 集合未知 | [R-ACTION-01](viewer-gameplay-details-uiux.design.md#r-action-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-005)；Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d | 动作来自 current gameplay projection，permission/identity/unknown/pending gate；动态 kind 集合未知 | [R-ACTION-01](viewer-gameplay-details-uiux.design.md#r-action-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | 只为当前可执行 action 显示真实控件；不将后端所有 action 推导成网页功能 | [R-ACTION-02](viewer-gameplay-details-uiux.design.md#r-action-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | 首次 OC guide 与折叠玩法详情分开；子卡必须依其数据 gate，摘要不代替全经济面板 | [R-GAME-26](viewer-gameplay-details-uiux.design.md#r-game-26) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |

## 3. 当前状态、目标状态与差距

`implemented_in_frozen_source` 只表示固定源码有实际消费者；条件显示不是未实现。`target_only` 只表示已有产品义务的设计目标；未确认专用Viewer入口不证明全仓后端不存在。QA与禁用占位不进入普通玩家当前功能分母。图中的示例数值不是真实世界结果。

## 4. 边界与结构

World / Targets / Command 是三个主Dock；Search在Targets内，Diagnostics/More为次级。Bevy是唯一世界renderer；图稿占位不是JS世界回退。当前主体、观察目标、模型连接、账号、session、admission、control分别表述。

## 5. 关键交互流程

先读对象与可用条件，再编辑草稿/请求预估，只有实际capability允许时才显式提交。pending/unknown保留原操作身份；切换观察对象不改写原回执。关闭弹层返回invoker，焦点/IME不得误触提交。具体feature条款优先于此共同顺序。

## 6. 字段、动作与具体表面

<a id="r-game-01"></a>
### R-GAME-01 初始 OC 引导与领取 gate

- 固定来源：[viewer_app.jsx:1182–1752](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L1182-L1752)。
- 实现/条件：`conditional_current`；初始 OC、绑定和经济认领分开；pending/credited/首聊可用分别显示，避免重复领取。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/first-oc-pending.svg)；区域：named visible cards: 新手启动资金 / 等待 committed 快照。
  - 已画状态：已发送 ≠ 已入账。
  - 区域「新手启动资金」：入账后解锁第一次 Agent 聊天；和早期玩法操作。；无需外部付费入场或模型充值。
  - 区域「等待 committed 快照」：Agent：示例 A；反馈：queued；余额尚未确认；不要重复领取。；可先阅读：指挥 Agent 恢复产线
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/first-oc-ready.svg)；区域：named visible cards: 已入账 / 下一步。
  - 已画状态：启动资格已满足。
  - 区域「已入账」：第一笔 OC 已在当前 Agent 快照可见。；可以进入第一次 Agent 聊天与早期操作。
  - 区域「下一步」：告诉 Agent 当前目标；观察 blocker。；接受意图不等于世界效果完成。；按钮只有正式动作发布后出现。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-02"></a>
### R-GAME-02 首聊解锁预览

- 固定来源：[first_chat_unlock_preview.jsx:1–83](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/first_chat_unlock_preview.jsx#L1-L83)。
- 实现/条件：`conditional_current`；正 liquid OC 或已有 starter claim 记录通过该门禁；defer 不制造/清除经济认领义务。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/first-chat-preview.svg)；区域：named visible cards: 目的 / 即时帮助 / 先试试 / 资源边界 / 如果等待 / 建议操作。
  - 已画状态：只读预览 · 绑定不等于经济认领。
  - 区域「目的 / 即时帮助 / 先试试」：与当前有控制权的 Agent 开始第一次对话。；询问 Agent 为当前玩法目标下一步能做什么。；试着问：我们第一步该做什么？
  - 区域「资源边界」：初始 OC 提供初始可用 OC 与一次性首聊解锁。；可用 OC > 0，或已有领取记录，即通过首聊 OC 门禁。；已有领取记录时，余额用尽也保持这道门禁通过。；它与经济 slot-1 认领及维护资金分开。
  - 区域「如果等待 / 建议操作」：经济认领义务未知：刷新权威状态。；暂缓不会创建或清除义务，也不推断已有认领。；建议操作：领取初始 OC（此组件只显示文字）。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-03"></a>
### R-GAME-03 经济 Agent 候选理由与承诺比较

- 固定来源：[agent_claim_choice_card.jsx:1–142](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/agent_claim_choice_card.jsx#L1-L142)。
- 实现/条件：`conditional_current`；published rationale、upfront、eligible balance、upkeep runway；理由缺失时不推荐候选。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/claim-defer.svg)；区域：named visible cards: 当前条件 / 现在不推荐任何候选。
  - 已画状态：暂缓 · 无推荐理由。
  - 区域「当前条件」：Eligible = Upfront：12 OC；认领后 upkeep runway：0 epochs；候选路线理由尚未发布
  - 区域「现在不推荐任何候选」：补足资金不等于被推荐。；等待 canonical 理由与额外 upkeep 余额。；目标 Agent：候选 A ▾
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-04"></a>
### R-GAME-04 经济认领明细、确认与释放边界

- 固定来源：[viewer_app.jsx:295–369](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L295-L369)。
- 实现/条件：`conditional_current`；quote/readability 不等于经济 claim；释放按已有 action 与当前合同。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/claim-ready.svg)；区域：named visible cards: 目标 Agent / 认领前核对。
  - 已画状态：可提交 · 示例数据。
  - 区域「目标 Agent」：候选 A ▾；位置 120, 0, 40 cm；Body / frame：已发布类型；专长：以 canonical 路线理由为准
  - 区域「认领前核对」：Slot 1 · Owned / cap 0 / 1；Eligible 20 OC · Upfront 12 OC；Activation fee 4 OC · Bond 8 OC；Upkeep 2 OC / epoch；数字为设计示例，不是运行时事实
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/claim-lifecycle.svg)；区域：named visible cards: 费用构成 · OC / 时间与风险 / 释放动作未发布。
  - 已画状态：只读生命周期。
  - 区域「费用构成 · OC」：Upfront 12 = Activation 4 + Bond 8；Eligible 20 · Liquid 8 · Restricted 12；Auto starter 0；Bond locked：restricted 8 / liquid 0
  - 区域「时间与风险」：Upkeep 2 OC / epoch · Paid through 20；Release ready in 2 epochs；Grace 3 · Idle warning 5 epochs；Forced reclaim 10 epochs · Penalty 500 bps；Reclaim terms：以已发布条款为准
  - 区域「释放动作未发布」：当前仅状态指导，不显示释放按钮。；已发布但阻塞时，应从正式动作列表查看原因。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-05"></a>
### R-GAME-05 认领目标选择与原会话边界

- 固定来源：[viewer_app.jsx:1783–1898](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L1783-L1898)。
- 实现/条件：`conditional_current`；绑定/认领/runtime session 分开；候选不替换未知原 Agent。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/claim-ready.svg)；区域：named visible cards: 目标 Agent / 认领前核对。
  - 已画状态：可提交 · 示例数据。
  - 区域「目标 Agent」：候选 A ▾；位置 120, 0, 40 cm；Body / frame：已发布类型；专长：以 canonical 路线理由为准
  - 区域「认领前核对」：Slot 1 · Owned / cap 0 / 1；Eligible 20 OC · Upfront 12 OC；Activation fee 4 OC · Bond 8 OC；Upkeep 2 OC / epoch；数字为设计示例，不是运行时事实
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-06"></a>
### R-GAME-06 扩张分支取舍

- 固定来源：[viewer_app.jsx:370–439](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L370-L439)。
- 实现/条件：`conditional_current`；只渲染已公布 branch card；机会/成本/承诺与可执行 action 分离。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/expansion-incomplete.svg)；区域：named visible cards: 路线 A / 动作未发布。
  - 已画状态：只读 · 推荐信息不完整。
  - 区域「路线 A」：即时收益：已发布说明；后续变化 1：已发布说明；后续变化 2：未发布；风险或锁定：未发布；下次会话目标：未发布
  - 区域「动作未发布」：不根据动作文案补造收益或未来变化。；完整推荐要求两项不同的后续变化。；此卡不提供执行按钮。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-07"></a>
### R-GAME-07 吸引力证明与下一选择

- 固定来源：[viewer_app.jsx:2433–2467](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2433-L2467)。
- 实现/条件：`conditional_current`；what I caused/new option/why continue/waiting cost/recovery 来自 gameplay projection。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/attraction-agency-unknown.svg)；区域：named visible cards: 吸引力证明 · unverified / 玩家能动性。
  - 已画状态：证据未验证，不编造玩家影响。
  - 区域「吸引力证明 · unverified」：What I caused：等待玩家导致的世界变化；New option：等待新选择；Why continue：等待下一分支；Waiting cost：等待 / 未验证；Recovery：等待恢复路径
  - 区域「玩家能动性」：Interrupt：未验证；Reprioritize：未验证；Correction：等待替代意图；Handoff：等待新旧意图交接；此读面不新增打断或纠偏按钮。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-08"></a>
### R-GAME-08 间接能动性动词

- 固定来源：[viewer_app.jsx:2468–2495](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2468-L2495)。
- 实现/条件：`conditional_current`；interrupt/reprioritize/correction/handoff 可读，不将意图当 world effect。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/attraction-agency-unknown.svg)；区域：named visible cards: 吸引力证明 · unverified / 玩家能动性。
  - 已画状态：证据未验证，不编造玩家影响。
  - 区域「吸引力证明 · unverified」：What I caused：等待玩家导致的世界变化；New option：等待新选择；Why continue：等待下一分支；Waiting cost：等待 / 未验证；Recovery：等待恢复路径
  - 区域「玩家能动性」：Interrupt：未验证；Reprioritize：未验证；Correction：等待替代意图；Handoff：等待新旧意图交接；此读面不新增打断或纠偏按钮。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-09"></a>
### R-GAME-09 首胜与反刷解释

- 固定来源：[viewer_app.jsx:2496–2524](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2496-L2524)。
- 实现/条件：`conditional_current`；首胜目标、玩家动作、世界变化与 leverage 分列。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/firstwin-continuation.svg)；区域：named visible cards: 首胜与反刷 / 成熟世界承接。
  - 已画状态：证据等待 / 成熟世界承接。
  - 区域「首胜与反刷」：First win goal：待发布；Player action：待提交；World change：待回执；Anti-grind leverage：待验证；Leverage verdict：未发布
  - 区域「成熟世界承接」：Dependency status：未验证；Recovery path：等待运行时指引；Share replay snippet：等待可复盘片段；Summary：等待成熟世界承接证据；修复 / 重建 / 转向比较仍需独立读取。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-10"></a>
### R-GAME-10 成熟世界承接

- 固定来源：[viewer_app.jsx:2525–2554](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2525-L2554)。
- 实现/条件：`conditional_current`；依赖、恢复路径和分享回放是现有摘要，不新建分享/迁移操作。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/firstwin-continuation.svg)；区域：named visible cards: 首胜与反刷 / 成熟世界承接。
  - 已画状态：证据等待 / 成熟世界承接。
  - 区域「首胜与反刷」：First win goal：待发布；Player action：待提交；World change：待回执；Anti-grind leverage：待验证；Leverage verdict：未发布
  - 区域「成熟世界承接」：Dependency status：未验证；Recovery path：等待运行时指引；Share replay snippet：等待可复盘片段；Summary：等待成熟世界承接证据；修复 / 重建 / 转向比较仍需独立读取。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-11"></a>
### R-GAME-11 已接受意图与恢复锚点

- 固定来源：[viewer_app.jsx:2555–2574](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2555-L2574)。
- 实现/条件：`conditional_current`；accepted intent 与 resume anchor 保留，accepted 不等于 applied。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/goal-execution-blocked.svg)；区域：named visible cards: 已接受意图 / 目标执行状态 / 当前目标与下一步。
  - 已画状态：Accepted → Executing → Blocked。
  - 区域「已接受意图」：Scope：已发布范围；Target：示例 Agent A；Summary / detail：以原意图为准；Resume anchor：未发布
  - 区域「目标执行状态」：当前：Blocked；其他分支：Completed / Rejected；Cause label：已发布原因；Cause detail：等待资源条件，不重写目标
  - 区域「当前目标与下一步」：Objective：恢复当前产线；Progress：n/a（未知不是 0%）；Blocker / supplemental detail：已发布说明；Next step：等待下一次运行时指引；Branch hint / agent-location counts：未发布
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-12"></a>
### R-GAME-12 执行状态机与原因

- 固定来源：[viewer_app.jsx:2575–2602](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2575-L2602)。
- 实现/条件：`conditional_current`；阶段、cause、当前执行状态来自权威投影，非本地推导成功。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/goal-execution-blocked.svg)；区域：named visible cards: 已接受意图 / 目标执行状态 / 当前目标与下一步。
  - 已画状态：Accepted → Executing → Blocked。
  - 区域「已接受意图」：Scope：已发布范围；Target：示例 Agent A；Summary / detail：以原意图为准；Resume anchor：未发布
  - 区域「目标执行状态」：当前：Blocked；其他分支：Completed / Rejected；Cause label：已发布原因；Cause detail：等待资源条件，不重写目标
  - 区域「当前目标与下一步」：Objective：恢复当前产线；Progress：n/a（未知不是 0%）；Blocker / supplemental detail：已发布说明；Next step：等待下一次运行时指引；Branch hint / agent-location counts：未发布
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-13"></a>
### R-GAME-13 当前目标、进度、阻塞与下一步

- 固定来源：[viewer_app.jsx:2603–2639](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2603-L2639)。
- 实现/条件：`current`；blocker/supplement/branch/entityCounts 分层；缺失值不补造。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/goal-execution-blocked.svg)；区域：named visible cards: 已接受意图 / 目标执行状态 / 当前目标与下一步。
  - 已画状态：Accepted → Executing → Blocked。
  - 区域「已接受意图」：Scope：已发布范围；Target：示例 Agent A；Summary / detail：以原意图为准；Resume anchor：未发布
  - 区域「目标执行状态」：当前：Blocked；其他分支：Completed / Rejected；Cause label：已发布原因；Cause detail：等待资源条件，不重写目标
  - 区域「当前目标与下一步」：Objective：恢复当前产线；Progress：n/a（未知不是 0%）；Blocker / supplemental detail：已发布说明；Next step：等待下一次运行时指引；Branch hint / agent-location counts：未发布
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-14"></a>
### R-GAME-14 等待解决预估

- 固定来源：[wait_resolution_quote_card.jsx:1–44](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/wait_resolution_quote_card.jsx#L1-L44)。
- 实现/条件：`conditional_current`；wait quote 是机会/风险说明，不自动等待或承诺下个 tick 生效。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/wait-resolution-safe.svg)；区域：named visible cards: 运行时等待判断 / 五项决定依据。
  - 已画状态：可以等待 · safe_to_wait = true。
  - 区域「运行时等待判断」：运行时说明等待是安全的。；到复查点仍需核实预期变化，不保证结果完成。
  - 区域「五项决定依据」：触发条件：等待指定输入抵达；复查点：已发布事件或 tick；预期变化：输入可供当前计划使用；未解决风险：仍需核对后续容量；替代解锁条件：以已发布条件为准
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/wait-resolution-unsafe.svg)；区域：named visible cards: 先比较恢复选项 / 五项决定依据。
  - 已画状态：不要等待 · 未明确 true 也走此分支。
  - 区域「先比较恢复选项」：safe_to_wait / safeToWait 未明确 true。；当前组件采用保守“不要等待”显示。；不提供“等待”执行按钮。
  - 区域「五项决定依据」：触发条件：—；复查点：—；预期变化：—；未解决风险：资源条件未解决；替代解锁条件：—；缺少 quote 时整个组件不渲染。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-15"></a>
### R-GAME-15 Fallback 取舍与无安全方案交接

- 固定来源：[fallback_tradeoff_panel.jsx:1–93](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/fallback_tradeoff_panel.jsx#L1-L93)。
- 实现/条件：`conditional_current`；选项值/风险/受限原因只读；不把普通聊天变成确认替代。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/fallback-choices.svg)；区域：named visible cards: 等待 safe_wait · 不可用 / 修复 repair_now · 可用 / 推荐 / 改道 reroute_now · 可用。
  - 已画状态：只读 · 所有已发布选项保留。
  - 区域「等待 safe_wait · 不可用」：原因：当前条件不能安全等待；保留：已提交进度；成本：已发布成本；机会成本：错过当前窗口
  - 区域「修复 repair_now · 可用 / 推荐」：原因：运行时推荐的恢复路径；保留：已发布保留进度；成本：已发布本地投入；机会成本：延后其他计划
  - 区域「改道 reroute_now · 可用」：原因：运行时发布的替代路径；保留 / 成本 / 机会成本：以各字段为准；可用与推荐是标签；此组件没有执行按钮。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/fallback-no-safe.svg)；区域：named visible cards: 无安全方案交接 / 停在真实边界。
  - 已画状态：需要新的决定。
  - 区域「无安全方案交接」：Reason：已发布阻塞原因；Required next decision：已发布 action ID 的可读名称；若无 action ID，则读取 decision class；两者缺失时不合成决定。
  - 区域「停在真实边界」：既无选项又无 handoff 时组件不显示。；已有不可用选项仍保留比较信息。；不自动等待、自动重试或伪造执行入口。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-16"></a>
### R-GAME-16 产品验证解锁预览

- 固定来源：[viewer_app.jsx:2642–2661](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2642-L2661)。
- 实现/条件：`conditional_current`；预览与 actual validated / unlocked 分离。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/validation-preview-available.svg)；区域：named visible cards: 产品与角色 / 阶段与下一步。
  - 已画状态：available · 只读能力说明。
  - 区域「产品与角色」：Product：示例 P；Role：已发布角色；Tradable：可交易；Value summary：已发布能力摘要
  - 区域「阶段与下一步」：Current stage：已发布当前阶段；Required stage：已发布要求阶段；Next step：已发布本地化提示；这里只是解锁预览，不是验证提交表单。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/validation-preview-unknown.svg)；区域：named visible cards: 未发布值的当前回退 / 阶段。
  - 已画状态：unknown · 无新增能力声明。
  - 区域「未发布值的当前回退」：Product：未知产品；Role：未知；Tradable：不可交易（false / 缺失回退）；验证结果未声明新的能力。
  - 区域「阶段」：Current stage：未知 / Required stage：未知；缺少下一步提示时不渲染提示段。；无 validationUnlockPreview 时整卡不显示。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-17"></a>
### R-GAME-17 能力经济投入、产出、新用途

- 固定来源：[viewer_app.jsx:2662–2692](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2662-L2692)。
- 实现/条件：`conditional_current`；只承接已有经济摘要与 next value，不创建资产交易。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/capability-economics.svg)；区域：named visible cards: 投入与产出 / 恢复指引与价值。
  - 已画状态：能力经济 · 未发布保持未知。
  - 区域「投入与产出」：Input：待发布；Output：待发布；New use / unlockedValue：待发布
  - 区域「恢复指引与价值」：Repair action：待发布；Blocker class：待发布；Next value：待发布；说明文字不是报价，不授权执行。；材料、电力与市场 quote 另行核对。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-18"></a>
### R-GAME-18 工业起步结果与阶段承接

- 固定来源：[industrial_starter_outcome_card.jsx:1–25](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/industrial_starter_outcome_card.jsx#L1-L25)。
- 实现/条件：`conditional_current`；已完成成果、当前能力、后继任务与阻塞各有真值边界。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/industrial-settled.svg)；区域：named visible cards: 已结算产出 / 下一候选。
  - 已画状态：production_only。
  - 区域「已结算产出」：iron_ingot × 36；投入：iron_ore × 48；配方 powerRequired：24（源未指定单位）；Accepted batches 12 · Job 5 · Tick 27；产出归属：示例 Agent A
  - 区域「下一候选」：Assembler MK1 候选资格已满足。；建造仍需当前材料、电力与授权。；稳定运行与交付必须分别验证。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-19"></a>
### R-GAME-19 微型仓库设施、库存与技术证据

- 固定来源：[micro_depot_facilities_panel.jsx:1–117](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/micro_depot_facilities_panel.jsx#L1-L117)。
- 实现/条件：`conditional_current`；ledger/库存、状态、可用时长依当前 projection；hash 技术证据按需披露。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/depot-empty.svg)；区域：named visible cards: 区域设施 / 服务能力 / 技术证据。
  - 已画状态：库存为空 · upkeep 未付。
  - 区域「区域设施」：Depot 示例 D · Location 示例 L；Claim 示例 C · Service radius 800 cm；Inventory revision 3；库存：空，不显示虚构资源
  - 区域「服务能力」：吞吐剩余 / 上限：0 / 20 units per epoch；Epoch 7 · Upkeep：未付；服务可用性可能受限。；当前快照未发布可用 depot 动作。
  - 区域「技术证据」：Module / version / wasm hash：未发布；Receipt / proposal：未发布
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-20"></a>
### R-GAME-20 首个交付预览

- 固定来源：[first_delivery_preview.jsx:1–50](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/first_delivery_preview.jsx#L1-L50)。
- 实现/条件：`conditional_current`；已具备/仍缺输入、生产与交付不混同。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/first-delivery-preview.svg)；区域：named visible cards: 需求与预计产出 / 价值与后续。
  - 已画状态：扩张卡内嵌只读预览。
  - 区域「需求与预计产出」：Local need：已发布本地需求；Expected output：已发布预计产出；Required inputs：逐条已发布输入；输入 A：示例材料 × 示例数量；输入 B：示例电力需求（单位依来源）
  - 区域「价值与后续」：Value timing：已发布价值时机；Leverage unlocked：已发布杠杆类别；Return visit hook：已发布回访钩子；缺失可选字段不显示；空输入数组不显示输入段。；预计产出不是交付结算，也没有执行按钮。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-21"></a>
### R-GAME-21 配方替代方案

- 固定来源：[recipe_substitution_panel.jsx:1–41](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/recipe_substitution_panel.jsx#L1-L41)。
- 实现/条件：`conditional_current`；支持配方/替代材料/缺口基于 model，不自动排产。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/recipe-known.svg)；区域：named visible cards: 同料换来源 / 显式异料替代 / 成本与风险 / 下一步。
  - 已画状态：只读 · 不扣料不加进度。
  - 区域「同料换来源 / 显式异料替代」：Slot input-1 · Legal candidates available；来源 source-A → target-B；投入 3 source units / 产出 2 target units；比例 3 / 2 · Round up；Loss 0 · Residual 0 / 1
  - 区域「成本与风险」：Conversion power：未发布；Terminal / logistics / buffer：未发布；Held / consumed / expected valuation：未发布；风险：以已发布条款为准；因果保持：决定本身不增加 work
  - 区域「下一步」：明确确认所选候选：仅发布的说明文本。；此只读面没有确认或转换按钮。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/recipe-unknown.svg)；区域：named visible cards: 缺少权威证据 / 无转换回执。
  - 已画状态：unknown / blocked。
  - 区域「缺少权威证据」：兼容性未知不能作为安全选项。；Recipe / work：未发布；Next recheck：未发布；补齐证据或资源后重新预览。
  - 区域「无转换回执」：decision_only：仅记录选择。；没有 settlement，不能宣称已转换。；超出读取上限时按准确 work identity 继续查询。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/recipe-settled.svg)；区域：named visible cards: 已确认决定 / 实际转换已结算。
  - 已画状态：只读回执对账。
  - 区域「已确认决定」：Decision 示例 D；Scope：conversion_authorized；只授权精确转换，不等于已经发生。
  - 区域「实际转换已结算」：独立 settlement：示例 S；Consumed 3 · Loss 0；以独立回执核对实际效果。；因果改变的候选必须从零 work 开始。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-22"></a>
### R-GAME-22 生产失败处置

- 固定来源：[factory_production_failure_disposition_card.jsx:1–97](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/factory_production_failure_disposition_card.jsx#L1-L97)。
- 实现/条件：`conditional_current`；失败原因、投入/电量与恢复 action；按钮仅使用已公布 action。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/failure-blocked.svg)；区域：named visible cards: 本次损失 / 恢复动作。
  - 已画状态：安全门提案 · 非现有行为。
  - 区域「本次损失」：Factory 示例 F · Recipe 示例 R；Blocker：validation failed；Consumed：iron_ore × 4 · Lost：× 1；Consumed power 2 · Lost power 1；电力单位源未指定
  - 区域「恢复动作」：recoveryAction：示例已发布动作；disabledReason：等待权威条件；下一次复查：next committed snapshot；当前源码按钮缺少 disabled；本稿明确补门。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-23"></a>
### R-GAME-23 控制影响证明

- 固定来源：[control_proof_panel.jsx:1–168](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/control_proof_panel.jsx#L1-L168)。
- 实现/条件：`conditional_current`；玩家指令、actual event、可归因后果分离；fixture 安装器不是实际证据。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/proof-awaiting.svg)；区域：named visible cards: 玩家意图 / 实际世界结果：未提供。
  - 已画状态：等待权威回执。
  - 区域「玩家意图」：Intent 示例 I：恢复产线；Agent 预测（未经验证）：产出可能增加；利害说明与替代方案：未经验证
  - 区域「实际世界结果：未提供」：等待 committed runtime receipt。；身份不匹配的 receipt 不归属本意图。；下一步：读取运行时发布的恢复提示。；不以 accepted、聊天文本或预测代签成功。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/proof-applied.svg)；区域：named visible cards: 实际回执 / 解释层分离。
  - 已画状态：只读 · applied。
  - 区域「实际回执」：Receipt 示例 R · Commit 示例 C；Action 19 · move agent；Domain event refs：42；Actual：agent moved；Effect intent ref：示例 I
  - 区域「解释层分离」：预测（未经验证）：到达后可能生产；Actual 仅证明移动，不证明生产成功。；Evidence / correction / interruption refs；Owner-control refs：按原始来源读取；Dissent / override / hard boundary：未提供
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/proof-grant-revoked.svg)；区域：named visible cards: 授权范围 / 电力额度 / 无撤销 UI。
  - 已画状态：只读 · 已撤销。
  - 区域「授权范围」：Grant 示例 G · revision 1；Source owner-grant · Issuer 示例 issuer；Object Agent A · Action move_agent；Period 示例 P · Tick 0–20
  - 区域「电力额度」：Spent 2 / Remaining 8 / Limit 10；单位：electricity units；本回执成本：2 electricity units；状态：已撤销；额度余额不意味着撤销后仍可使用。
  - 区域「无撤销 UI」：这是授权证据，不是授权管理表单。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/proof-memory-prepared.svg)；区域：named visible cards: 来源与新鲜度 / 纠正状态。
  - 已画状态：只读 · accepted。
  - 区域「来源与新鲜度」：Scope session_private · Revision 3；Source private_memory_retrieval_context；Origin receipt：示例 R；Freshness：当前版本
  - 区域「纠正状态」：Correction 示例 C：accepted；Earliest prepared request / digest：示例 P / D；Committed request / digest：未提供；Runtime receipt / action：未提供；仅进入准备中的请求，不等于实际生效。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/proof-memory-stale.svg)；区域：named visible cards: 保持真实状态 / 不补造提交证据。
  - 已画状态：只读 · stale / ignored。
  - 区域「保持真实状态」：Memory revision 2 · Stale；Correction：stale；刷新后按当前版本处理纠正提示。
  - 区域「不补造提交证据」：Ignored：未用于提交结果；Applied 才说明用于 committed decision。；此面没有编辑记忆或提交纠正按钮。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-24"></a>
### R-GAME-24 repair / rebuild / pivot 恢复比较

- 固定来源：[recovery_option_comparison_panel.jsx:1–82](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/recovery_option_comparison_panel.jsx#L1-L82)。
- 实现/条件：`conditional_current`；仅已公布选项、代价、风险和后继；没有凭 UI 增加恢复协议。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/recovery-compare.svg)；区域：named visible cards: 修复 Repair / 重建 Rebuild / 转向 Pivot。
  - 已画状态：只读方案比较。
  - 区域「修复 Repair」：时间：短期；资源：集中本地投入；风险：低；保留收益：已发布收益；原因：运行时推荐理由
  - 区域「重建 Rebuild / 转向 Pivot」：时间：中期 / 未发布；资源：更广泛本地再投入 / 转向本地投入；风险：中等 / 权衡；保留收益与推荐原因分别读取。；没有正式动作时不提供执行控件。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-25"></a>
### R-GAME-25 短期目标重排表单

- 固定来源：[reprioritize_action_form.jsx:1–78](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/reprioritize_action_form.jsx#L1-L78)。
- 实现/条件：`conditional_current`；显式打开/草稿/取消/提交；替换目标不保证下一步执行，in-flight 禁止重复。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/reprioritize-keyboard.svg)；区域：named visible cards: 替代短期目标 * / 不会保证下一步必定执行。
  - 已画状态：输入中 · 键盘安全区。
  - 区域「替代短期目标 *」：先恢复本地供电，；再评估当前产线的材料缺口。
  - 区域「不会保证下一步必定执行」：空输入时“应用新目标”禁用。；IME 组合输入未完成时不提交。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/reprioritize-error.svg)；区域：named visible cards: 替代短期目标 * / 提交失败。
  - 已画状态：错误 · 保留草稿。
  - 区域「替代短期目标 *」：先恢复供电，再评估产线。
  - 区域「提交失败」：权限已过期。；按当前页面授权恢复流程处理后重试。；不自动重放；目标尚未替换。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/reprioritize-pending.svg)；区域：named visible cards: 替代短期目标 * / 正在认证并提交。
  - 已画状态：signing / sent。
  - 区域「替代短期目标 *」：先恢复供电，再评估产线。
  - 区域「正在认证并提交」：等待 apply_ack。；此时不代表 Agent 必定执行，；也不代表世界结果已提交。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-action-01"></a>
### R-ACTION-01 动态已公布 gameplay action 执行

- 固定来源：[viewer_app.jsx:945–1181](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L945-L1181)。
- 实现/条件：`conditional_current`；动作来自 current gameplay projection，permission/identity/unknown/pending gate；动态 kind 集合未知。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/build-blocked.svg)；区域：named visible cards: 动作身份 / 阻塞与下一步。
  - 已画状态：受阻 · 正式动作示例。
  - 区域「动作身份」：build_factory_smelter_mk1；作用对象：示例 Agent A；executeKind：gameplay_action
  - 区域「阻塞与下一步」：材料条件尚未满足。；按当前已发布 blocker 查看物料来源；；不得从未知库存推断可建造。；数量与用电若未发布则显示未发布。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/build-pending.svg)；区域：named visible cards: 原操作 / 等待权威结果。
  - 已画状态：pending · 非建造成功。
  - 区域「原操作」：build_factory_smelter_mk1；请求对象：Agent A；当前观察对象：Agent B；结果仍归原操作与 Agent A
  - 区域「等待权威结果」：反馈：sent / accepted；工厂创建尚未得到 committed 证据。；保留操作身份，禁止重复点击。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/actions-unpublished.svg)；区域：named visible cards: 当前快照 / 可恢复路径。
  - 已画状态：未知，不是无事可做。
  - 区域「当前快照」：available_actions：未发布；下一步提示：未发布；不要合成建造、转账、释放或重试动作。
  - 区域「可恢复路径」：保留 World / Targets / Command 导航。；快照刷新仅在现有能力允许时提供。；动态动作列表不是固定有限功能目录。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-action-02"></a>
### R-ACTION-02 可用动作列表及未暴露动作说明

- 固定来源：[viewer_app.jsx:2716–2872](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2716-L2872)。
- 实现/条件：`conditional_current`；只为当前可执行 action 显示真实控件；不将后端所有 action 推导成网页功能。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/build-blocked.svg)；区域：named visible cards: 动作身份 / 阻塞与下一步。
  - 已画状态：受阻 · 正式动作示例。
  - 区域「动作身份」：build_factory_smelter_mk1；作用对象：示例 Agent A；executeKind：gameplay_action
  - 区域「阻塞与下一步」：材料条件尚未满足。；按当前已发布 blocker 查看物料来源；；不得从未知库存推断可建造。；数量与用电若未发布则显示未发布。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/build-pending.svg)；区域：named visible cards: 原操作 / 等待权威结果。
  - 已画状态：pending · 非建造成功。
  - 区域「原操作」：build_factory_smelter_mk1；请求对象：Agent A；当前观察对象：Agent B；结果仍归原操作与 Agent A
  - 区域「等待权威结果」：反馈：sent / accepted；工厂创建尚未得到 committed 证据。；保留操作身份，禁止重复点击。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/actions-unpublished.svg)；区域：named visible cards: 当前快照 / 可恢复路径。
  - 已画状态：未知，不是无事可做。
  - 区域「当前快照」：available_actions：未发布；下一步提示：未发布；不要合成建造、转账、释放或重试动作。
  - 区域「可恢复路径」：保留 World / Targets / Command 导航。；快照刷新仅在现有能力允许时提供。；动态动作列表不是固定有限功能目录。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。

<a id="r-game-26"></a>
### R-GAME-26 Gameplay Details 按需展开

- 固定来源：[viewer_app.jsx:2340–2432](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2340-L2432)。
- 实现/条件：`current`；首次 OC guide 与折叠玩法详情分开；子卡必须依其数据 gate，摘要不代替全经济面板。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/gameplay-disclosure-closed.svg)；区域：named visible cards: ▸ 玩法明细 / 摘要之外的世界上下文。
  - 已画状态：当前 native details · closed。
  - 区域「▸ 玩法明细」：世界棋盘上方已保留目标、下一步和回执；；这里展开看完整状态机与经济明细。；当前收起：不展示内部状态机/经济卡。
  - 区域「摘要之外的世界上下文」：World：保留世界舞台。；目标、下一步、原回执继续在舞台上方阅读。；此展开动作不提交玩法，也不更换 Agent。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。
- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/gameplay/frames/gameplay-disclosure-open.svg)；区域：named visible cards: ▾ 玩法明细 / Formal Gameplay Summary / 展开后仍可能等待。
  - 已画状态：当前 native details · open。
  - 区域「▾ 玩法明细」：先读目标、阻塞与下一步，再决定是否进入 Command。；正式玩法摘要已展开。
  - 区域「Formal Gameplay Summary」：Stage status：已发布阶段；Progress：n/a；内容继续包含配方/失败/ControlProof 与后续经济面。
  - 区域「展开后仍可能等待」：尚无规范玩法摘要时：等待首条规范玩法快照…；本稿只证明展开结构，不代签全部内嵌内容状态。
  - 未证明：Only the explicit rendered state/fields; narrative mentions are not additional state screenshots.；真实运行未执行。


## 7. 状态、事务与持久化

loading、empty、unavailable、permission denied、pending、unknown与明确失败不得混成一个状态。模型/查询成功不等于世界生效。原结果按原主体和请求身份回读，不能因刷新/切换/重连重放在途意图。本设计不增加持久化协议；动态DTO/服务capability未提供的字段或枚举保留未知。

## 8. 安全、权限与隐私

FactoryFailure当前source存在disabledReason说明但按钮无disabled属性；图中禁用恢复按钮属于安全修正提案，不宣称源码已阻止点击。RecipeSubstitution为只读，decision与实际settlement分开；没有确认/转换按钮。

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
| [REQ-FIRST-INDUSTRIAL-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)；Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | [R-GAME-01](viewer-gameplay-details-uiux.design.md#r-game-01) | 初始 OC、绑定和经济认领分开；pending/credited/首聊可用分别显示，避免重复领取 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-GAME-01](viewer-gameplay-details-uiux.design.md#r-game-01) | 初始 OC、绑定和经济认领分开；pending/credited/首聊可用分别显示，避免重复领取 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)；Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | [R-GAME-02](viewer-gameplay-details-uiux.design.md#r-game-02) | 正 liquid OC 或已有 starter claim 记录通过该门禁；defer 不制造/清除经济认领义务 | 对照 [first_chat_unlock_preview.test.jsx](../../../crates/oasis7_viewer/software_safe_src/first_chat_unlock_preview.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)；Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf | [R-GAME-03](viewer-gameplay-details-uiux.design.md#r-game-03) | published rationale、upfront、eligible balance、upkeep runway；理由缺失时不推荐候选 | 对照 [agent_claim_choice_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/agent_claim_choice_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)；Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf | [R-GAME-04](viewer-gameplay-details-uiux.design.md#r-game-04) | quote/readability 不等于经济 claim；释放按已有 action 与当前合同 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-AOS-002](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-002)；Trace refs: O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287 | [R-GAME-04](viewer-gameplay-details-uiux.design.md#r-game-04) | quote/readability 不等于经济 claim；释放按已有 action 与当前合同 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)；Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf | [R-GAME-05](viewer-gameplay-details-uiux.design.md#r-game-05) | 绑定/认领/runtime session 分开；候选不替换未知原 Agent | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-GAME-05](viewer-gameplay-details-uiux.design.md#r-game-05) | 绑定/认领/runtime session 分开；候选不替换未知原 Agent | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce | [R-GAME-06](viewer-gameplay-details-uiux.design.md#r-game-06) | 只渲染已公布 branch card；机会/成本/承诺与可执行 action 分离 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-GR-001](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001)；Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8 | [R-GAME-06](viewer-gameplay-details-uiux.design.md#r-game-06) | 只渲染已公布 branch card；机会/成本/承诺与可执行 action 分离 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)；Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3 | [R-GAME-07](viewer-gameplay-details-uiux.design.md#r-game-07) | what I caused/new option/why continue/waiting cost/recovery 来自 gameplay projection | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)；Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [R-GAME-08](viewer-gameplay-details-uiux.design.md#r-game-08) | interrupt/reprioritize/correction/handoff 可读，不将意图当 world effect | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-08；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)；Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | [R-GAME-08](viewer-gameplay-details-uiux.design.md#r-game-08) | interrupt/reprioritize/correction/handoff 可读，不将意图当 world effect | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-08；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)；Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | [R-GAME-09](viewer-gameplay-details-uiux.design.md#r-game-09) | 首胜目标、玩家动作、世界变化与 leverage 分列 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-09；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)；Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3 | [R-GAME-09](viewer-gameplay-details-uiux.design.md#r-game-09) | 首胜目标、玩家动作、世界变化与 leverage 分列 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-09；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce | [R-GAME-10](viewer-gameplay-details-uiux.design.md#r-game-10) | 依赖、恢复路径和分享回放是现有摘要，不新建分享/迁移操作 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-10；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | [R-GAME-10](viewer-gameplay-details-uiux.design.md#r-game-10) | 依赖、恢复路径和分享回放是现有摘要，不新建分享/迁移操作 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-10；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)；Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [R-GAME-11](viewer-gameplay-details-uiux.design.md#r-game-11) | accepted intent 与 resume anchor 保留，accepted 不等于 applied | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-11；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-IA-002](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)；Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | [R-GAME-11](viewer-gameplay-details-uiux.design.md#r-game-11) | accepted intent 与 resume anchor 保留，accepted 不等于 applied | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-11；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)；Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [R-GAME-12](viewer-gameplay-details-uiux.design.md#r-game-12) | 阶段、cause、当前执行状态来自权威投影，非本地推导成功 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-12；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-IA-002](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)；Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | [R-GAME-13](viewer-gameplay-details-uiux.design.md#r-game-13) | blocker/supplement/branch/entityCounts 分层；缺失值不补造 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-13；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | [R-GAME-13](viewer-gameplay-details-uiux.design.md#r-game-13) | blocker/supplement/branch/entityCounts 分层；缺失值不补造 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-13；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-IA-002](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)；Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | [R-GAME-14](viewer-gameplay-details-uiux.design.md#r-game-14) | wait quote 是机会/风险说明，不自动等待或承诺下个 tick 生效 | 对照 [wait_resolution_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/wait_resolution_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-14；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [R-GAME-14](viewer-gameplay-details-uiux.design.md#r-game-14) | wait quote 是机会/风险说明，不自动等待或承诺下个 tick 生效 | 对照 [wait_resolution_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/wait_resolution_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-14；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)；Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | [R-GAME-15](viewer-gameplay-details-uiux.design.md#r-game-15) | 选项值/风险/受限原因只读；不把普通聊天变成确认替代 | 对照 [fallback_tradeoff_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/fallback_tradeoff_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-15；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-010](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-010)；Trace refs: O7-REQ-3bbcade1-3f73-4899-b324-f857fd27b245 | [R-GAME-15](viewer-gameplay-details-uiux.design.md#r-game-15) | 选项值/风险/受限原因只读；不把普通聊天变成确认替代 | 对照 [fallback_tradeoff_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/fallback_tradeoff_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-15；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [R-GAME-16](viewer-gameplay-details-uiux.design.md#r-game-16) | 预览与 actual validated / unlocked 分离 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-16；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)；Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | [R-GAME-17](viewer-gameplay-details-uiux.design.md#r-game-17) | 只承接已有经济摘要与 next value，不创建资产交易 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-17；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008)；Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | [R-GAME-18](viewer-gameplay-details-uiux.design.md#r-game-18) | 已完成成果、当前能力、后继任务与阻塞各有真值边界 | 对照 [industrial_starter_outcome_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/industrial_starter_outcome_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-18；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-GR-001](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001)；Trace refs: O7-REQ-d34e2dec-9ad4-44a8-9ba7-7931956e39b8 | [R-GAME-19](viewer-gameplay-details-uiux.design.md#r-game-19) | ledger/库存、状态、可用时长依当前 projection；hash 技术证据按需披露 | 对照 [micro_depot_facilities_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/micro_depot_facilities_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-19；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | [R-GAME-19](viewer-gameplay-details-uiux.design.md#r-game-19) | ledger/库存、状态、可用时长依当前 projection；hash 技术证据按需披露 | 对照 [micro_depot_facilities_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/micro_depot_facilities_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-19；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | [R-GAME-20](viewer-gameplay-details-uiux.design.md#r-game-20) | 已具备/仍缺输入、生产与交付不混同 | 对照 [first_delivery_preview.test.jsx](../../../crates/oasis7_viewer/software_safe_src/first_delivery_preview.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-20；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-004](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-004)；Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2 | [R-GAME-20](viewer-gameplay-details-uiux.design.md#r-game-20) | 已具备/仍缺输入、生产与交付不混同 | 对照 [first_delivery_preview.test.jsx](../../../crates/oasis7_viewer/software_safe_src/first_delivery_preview.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-20；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | [R-GAME-21](viewer-gameplay-details-uiux.design.md#r-game-21) | 支持配方/替代材料/缺口基于 model，不自动排产 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-21；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [R-GAME-22](viewer-gameplay-details-uiux.design.md#r-game-22) | 失败原因、投入/电量与恢复 action；按钮仅使用已公布 action | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-22；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | [R-GAME-22](viewer-gameplay-details-uiux.design.md#r-game-22) | 失败原因、投入/电量与恢复 action；按钮仅使用已公布 action | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-22；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)；Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | [R-GAME-23](viewer-gameplay-details-uiux.design.md#r-game-23) | 玩家指令、actual event、可归因后果分离；fixture 安装器不是实际证据 | 对照 [control_proof_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/control_proof_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-23；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)；Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3 | [R-GAME-23](viewer-gameplay-details-uiux.design.md#r-game-23) | 玩家指令、actual event、可归因后果分离；fixture 安装器不是实际证据 | 对照 [control_proof_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/control_proof_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-23；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | [R-GAME-24](viewer-gameplay-details-uiux.design.md#r-game-24) | 仅已公布选项、代价、风险和后继；没有凭 UI 增加恢复协议 | 对照 [recovery_option_comparison_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/recovery_option_comparison_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-24；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | [R-GAME-24](viewer-gameplay-details-uiux.design.md#r-game-24) | 仅已公布选项、代价、风险和后继；没有凭 UI 增加恢复协议 | 对照 [recovery_option_comparison_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/recovery_option_comparison_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-24；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)；Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [R-GAME-25](viewer-gameplay-details-uiux.design.md#r-game-25) | 显式打开/草稿/取消/提交；替换目标不保证下一步执行，in-flight 禁止重复 | 对照 [reprioritize_action_form.test.jsx](../../../crates/oasis7_viewer/software_safe_src/reprioritize_action_form.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-25；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)；Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | [R-GAME-25](viewer-gameplay-details-uiux.design.md#r-game-25) | 显式打开/草稿/取消/提交；替换目标不保证下一步执行，in-flight 禁止重复 | 对照 [reprioritize_action_form.test.jsx](../../../crates/oasis7_viewer/software_safe_src/reprioritize_action_form.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-25；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-ACTION-01](viewer-gameplay-details-uiux.design.md#r-action-01) | 动作来自 current gameplay projection，permission/identity/unknown/pending gate；动态 kind 集合未知 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-ACTION-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | [R-ACTION-01](viewer-gameplay-details-uiux.design.md#r-action-01) | 动作来自 current gameplay projection，permission/identity/unknown/pending gate；动态 kind 集合未知 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-ACTION-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-005)；Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d | [R-ACTION-01](viewer-gameplay-details-uiux.design.md#r-action-01) | 动作来自 current gameplay projection，permission/identity/unknown/pending gate；动态 kind 集合未知 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-ACTION-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-ACTION-02](viewer-gameplay-details-uiux.design.md#r-action-02) | 只为当前可执行 action 显示真实控件；不将后端所有 action 推导成网页功能 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-ACTION-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | [R-GAME-26](viewer-gameplay-details-uiux.design.md#r-game-26) | 首次 OC guide 与折叠玩法详情分开；子卡必须依其数据 gate，摘要不代替全经济面板 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-GAME-26；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |

## 12. 决策、长期风险与未决问题

- 图稿仅代表实际映射区域/状态；没有对应frame的项保持text_spec_only，未公布提交合同保持contract_blocked。图稿完备性不得用页面数判定。
- 本主题的具体frame及字段/动作/状态关系见随稿覆盖目录；未画状态明确保留text_spec_only，不能从概览推导。
- 动态DTO、未知action集合、部署能力、原生桌面及真实世界结果链仍待各专业owner在获准环境验证。
- 需求设计关系只覆盖表中适用范围，不自动将产品状态升为已覆盖；未核实的专业协议或目标CRUD保持明确缺口。
