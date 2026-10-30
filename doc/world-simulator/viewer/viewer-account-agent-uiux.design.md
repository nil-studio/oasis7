# Viewer 账户、Agent 指导与模型连接 UI/UX 设计

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
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | registration_enabled 与 passkey.available；创建账号不等于世界准入 | [R-AUTH-01](viewer-account-agent-uiux.design.md#r-auth-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | registration_enabled 与 passkey.available；创建账号不等于世界准入 | [R-AUTH-01](viewer-account-agent-uiux.design.md#r-auth-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | 用户显式点击启动认证；已登记凭证与当前 session 分开 | [R-AUTH-02](viewer-account-agent-uiux.design.md#r-auth-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | 用户显式点击启动认证；已登记凭证与当前 session 分开 | [R-AUTH-02](viewer-account-agent-uiux.design.md#r-auth-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | 已登录账号；control_elsewhere 时保留原控制或按 expected epoch 明确接管 | [R-AUTH-03](viewer-account-agent-uiux.design.md#r-auth-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | 已登录账号；control_elsewhere 时保留原控制或按 expected epoch 明确接管 | [R-AUTH-03](viewer-account-agent-uiux.design.md#r-auth-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | 列表失败局部重试；凭证不等于设备会话；安全操作由当前 authority 验证 | [R-AUTH-04](viewer-account-agent-uiux.design.md#r-auth-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | session.current 与其他会话区分；失效时清除私有上下文和导入 signer | [R-AUTH-05](viewer-account-agent-uiux.design.md#r-auth-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | session.current 与其他会话区分；失效时清除私有上下文和导入 signer | [R-AUTH-05](viewer-account-agent-uiux.design.md#r-auth-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | operation_pending 保留 operation ID；核对原操作而非新建账号/替换候选 | [R-AUTH-06](viewer-account-agent-uiux.design.md#r-auth-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | operation_pending 保留 operation ID；核对原操作而非新建账号/替换候选 | [R-AUTH-06](viewer-account-agent-uiux.design.md#r-auth-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | native_key capability；公钥和私钥种子在显式点击后导入，原输入清空，只在页内存签名，不是钱包密钥 | [R-AUTH-07](viewer-account-agent-uiux.design.md#r-auth-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | native_key capability；公钥和私钥种子在显式点击后导入，原输入清空，只在页内存签名，不是钱包密钥 | [R-AUTH-07](viewer-account-agent-uiux.design.md#r-auth-07) | identity native-key；技术依赖 [Flow-KEYAUTH-008](../../p2p/blockchain/hosted-public-join-key-based-onboarding.prd.md#imported-key-login) | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | email capability；OTP 按指定用途；账号/邮箱/通行密钥状态分开 | [R-AUTH-08](viewer-account-agent-uiux.design.md#r-auth-08) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | email_recovery capability；原绑定邮箱资格、候选 digest、等待/到期时间；完成再验证邮箱与候选 | [R-AUTH-09](viewer-account-agent-uiux.design.md#r-auth-09) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | 仅 HTTP 404 回退 legacyForm；其他错误保持能力检查失败 | [R-AUTH-10](viewer-account-agent-uiux.design.md#r-auth-10) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | 仅 HTTP 404 回退 legacyForm；其他错误保持能力检查失败 | [R-AUTH-10](viewer-account-agent-uiux.design.md#r-auth-10) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | 仅既有兼容部署；不是普通入口唯一默认 | [R-AUTH-11](viewer-account-agent-uiux.design.md#r-auth-11) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | hosted public join 需要登录时；背景不可交互，登录与世界进入分阶段 | [R-AUTH-12](viewer-account-agent-uiux.design.md#r-auth-12) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | hosted public join 需要登录时；背景不可交互，登录与世界进入分阶段 | [R-AUTH-12](viewer-account-agent-uiux.design.md#r-auth-12) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | showPlayerSessionSurface；session registration/control truth 分离 | [R-AUTH-13](viewer-account-agent-uiux.design.md#r-auth-13) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | 显式测试 capability/适用环境；排除普通 Player 功能分母 | [R-AUTH-QA](viewer-account-agent-uiux.design.md#r-auth-qa) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-001](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001)；Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8 | browser game client 前置；owner mismatch 停止；unsent 保留原件；目标 UTF-8 字节上限 8192 | [R-BYOK-01](viewer-account-agent-uiux.design.md#r-byok-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-COLLAB-001](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#req-collab-001)；Trace refs: O7-REQ-62007da5-b12a-4b8c-98bb-82ab53a35069 | browser game client 前置；owner mismatch 停止；unsent 保留原件；目标 UTF-8 字节上限 8192 | [R-BYOK-01](viewer-account-agent-uiux.design.md#r-byok-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003)；Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464 | 先只读 inspect 再显式 takeover；核对 fence，不因保存 Key 获得游戏控制 | [R-BYOK-02](viewer-account-agent-uiux.design.md#r-byok-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-COLLAB-001](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#req-collab-001)；Trace refs: O7-REQ-62007da5-b12a-4b8c-98bb-82ab53a35069 | 先只读 inspect 再显式 takeover；核对 fence，不因保存 Key 获得游戏控制 | [R-BYOK-02](viewer-account-agent-uiux.design.md#r-byok-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-001](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001)；Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8 | 当前 DeepSeek；localStorage 告知、账号隔离与 explicit consent；不自动开始 | [R-BYOK-03](viewer-account-agent-uiux.design.md#r-byok-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002)；Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca | 当前 DeepSeek；localStorage 告知、账号隔离与 explicit consent；不自动开始 | [R-BYOK-03](viewer-account-agent-uiux.design.md#r-byok-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-005](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-005)；Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d | 当前 DeepSeek；localStorage 告知、账号隔离与 explicit consent；不自动开始 | [R-BYOK-03](viewer-account-agent-uiux.design.md#r-byok-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002)；Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca | hasKey 与独立 probe consent；有限两请求工具检查，不写世界 | [R-BYOK-04](viewer-account-agent-uiux.design.md#r-byok-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-005](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-005)；Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d | hasKey 与独立 probe consent；有限两请求工具检查，不写世界 | [R-BYOK-04](viewer-account-agent-uiux.design.md#r-byok-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003)；Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464 | run consent + ready + model ready；实际 Agent/任务、预算和 phase 明确；停止不撤销已受理世界动作 | [R-BYOK-05](viewer-account-agent-uiux.design.md#r-byok-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004)；Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34 | run consent + ready + model ready；实际 Agent/任务、预算和 phase 明确；停止不撤销已受理世界动作 | [R-BYOK-05](viewer-account-agent-uiux.design.md#r-byok-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002)；Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca | 删除本站 Key 不撤销供应商 Key、不保证已发请求退款；删除失败不能报成功 | [R-BYOK-06](viewer-account-agent-uiux.design.md#r-byok-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004)；Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34 | 删除本站 Key 不撤销供应商 Key、不保证已发请求退款；删除失败不能报成功 | [R-BYOK-06](viewer-account-agent-uiux.design.md#r-byok-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-001](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001)；Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8 | legacy management/capability 分支；配置 preset、连接状态与执行授权分离，不宣传浏览器之外新托管能力 | [R-BYOK-07](viewer-account-agent-uiux.design.md#r-byok-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003)；Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464 | legacy management/capability 分支；配置 preset、连接状态与执行授权分离，不宣传浏览器之外新托管能力 | [R-BYOK-07](viewer-account-agent-uiux.design.md#r-byok-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004)；Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34 | legacy management/capability 分支；配置 preset、连接状态与执行授权分离，不宣传浏览器之外新托管能力 | [R-BYOK-07](viewer-account-agent-uiux.design.md#r-byok-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-001](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-001)；Trace refs: O7-REQ-14ddd9ba-2d94-48e8-b2e4-0dbae26ddc36 | 选中对象不等于拥有；无 Agent/地点/模块/只读视角分别呈现 | [R-CMD-01](viewer-account-agent-uiux.design.md#r-cmd-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | 选中对象不等于拥有；无 Agent/地点/模块/只读视角分别呈现 | [R-CMD-01](viewer-account-agent-uiux.design.md#r-cmd-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-002](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002)；Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293 | 当前选中可控 Agent + 首聊门禁；draft/send/pending/result 分离 | [R-CMD-02](viewer-account-agent-uiux.design.md#r-cmd-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004)；Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111 | 当前选中可控 Agent + 首聊门禁；draft/send/pending/result 分离 | [R-CMD-02](viewer-account-agent-uiux.design.md#r-cmd-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-COLLAB-002](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#req-collab-002)；Trace refs: O7-REQ-edd52061-73c0-433c-9531-30d86b1db459 | 只显示 authenticatedCanonicalGoal；pending 提交不写成应用 | [R-CMD-03](viewer-account-agent-uiux.design.md#r-cmd-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004)；Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111 | 只显示 authenticatedCanonicalGoal；pending 提交不写成应用 | [R-CMD-03](viewer-account-agent-uiux.design.md#r-cmd-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | 活动 kind、target、operation、blocked reason 来自显示模型；unknown/idle 不捏造工作 | [R-CMD-04](viewer-account-agent-uiux.design.md#r-cmd-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004)；Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111 | read_only/control_lost/unauthorized/unavailable 隐去私有详情，反馈仍属原主体 | [R-CMD-05](viewer-account-agent-uiux.design.md#r-cmd-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007)；Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd | read_only/control_lost/unauthorized/unavailable 隐去私有详情，反馈仍属原主体 | [R-CMD-05](viewer-account-agent-uiux.design.md#r-cmd-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | read_only/control_lost/unauthorized/unavailable 隐去私有详情，反馈仍属原主体 | [R-CMD-05](viewer-account-agent-uiux.design.md#r-cmd-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | 只读当前 Agent 上下文，字段缺失保持 unavailable，不推导执行成功 | [R-CMD-06](viewer-account-agent-uiux.design.md#r-cmd-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | 只读当前 Agent 上下文，字段缺失保持 unavailable，不推导执行成功 | [R-CMD-06](viewer-account-agent-uiux.design.md#r-cmd-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-010](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-010)；Trace refs: O7-REQ-3bbcade1-3f73-4899-b324-f857fd27b245 | 当前 authenticated owner；waypoint/timed request 不支持；preview/confirmation/execution 分离；unknown 只恢复原签名命令 | [R-CMD-07](viewer-account-agent-uiux.design.md#r-cmd-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-011](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-011)；Trace refs: O7-REQ-36cc9e9d-eaa7-436f-b16b-6f6af089b04c | 当前 authenticated owner；waypoint/timed request 不支持；preview/confirmation/execution 分离；unknown 只恢复原签名命令 | [R-CMD-07](viewer-account-agent-uiux.design.md#r-cmd-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | 当前 authenticated owner；waypoint/timed request 不支持；preview/confirmation/execution 分离；unknown 只恢复原签名命令 | [R-CMD-07](viewer-account-agent-uiux.design.md#r-cmd-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007)；Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd | operator/strong auth 条件由后端裁定，不把本地填写视为授权 | [R-CMD-08](viewer-account-agent-uiux.design.md#r-cmd-08) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-002](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002)；Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293 | system/short/long 与聊天草稿分开；preview 不是 applied | [R-CMD-09](viewer-account-agent-uiux.design.md#r-cmd-09) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-003](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-003)；Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d | system/short/long 与聊天草稿分开；preview 不是 applied | [R-CMD-09](viewer-account-agent-uiux.design.md#r-cmd-09) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-005](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-005)；Trace refs: O7-REQ-b6140b6c-ca2c-4734-84ce-7963f9385fff | system/short/long 与聊天草稿分开；preview 不是 applied | [R-CMD-09](viewer-account-agent-uiux.design.md#r-cmd-09) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004)；Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111 | control lost 安全停止，refresh binding 不自动重发原修改 | [R-CMD-10](viewer-account-agent-uiux.design.md#r-cmd-10) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007)；Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd | control lost 安全停止，refresh binding 不自动重发原修改 | [R-CMD-10](viewer-account-agent-uiux.design.md#r-cmd-10) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-003](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-003)；Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d | 只回滚 Prompt 版本，不回滚世界、资产或回执 | [R-CMD-11](viewer-account-agent-uiux.design.md#r-cmd-11) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-008](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-008)；Trace refs: O7-REQ-a9797fdb-39f7-48fb-9202-9700d9d86159 | 只回滚 Prompt 版本，不回滚世界、资产或回执 | [R-CMD-11](viewer-account-agent-uiux.design.md#r-cmd-11) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-009](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-009)；Trace refs: O7-REQ-92124092-6021-49e4-8def-33038e20b637 | 只回滚 Prompt 版本，不回滚世界、资产或回执 | [R-CMD-11](viewer-account-agent-uiux.design.md#r-cmd-11) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | 现有按钮禁用，仅可读边界；不计为资产转账/治理提交已实现 | [R-CMD-12](viewer-account-agent-uiux.design.md#r-cmd-12) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007)；Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd | 仅 feedback.code=control_lost 且提供真实 recoveryAction；恢复不隐式重发 | [R-CMD-13](viewer-account-agent-uiux.design.md#r-cmd-13) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | 仅 feedback.code=control_lost 且提供真实 recoveryAction；恢复不隐式重发 | [R-CMD-13](viewer-account-agent-uiux.design.md#r-cmd-13) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |

## 3. 当前状态、目标状态与差距

`implemented_in_frozen_source` 只表示固定源码有实际消费者；条件显示不是未实现。`target_only` 只表示已有产品义务的设计目标；未确认专用Viewer入口不证明全仓后端不存在。QA与禁用占位不进入普通玩家当前功能分母。图中的示例数值不是真实世界结果。

## 4. 边界与结构

World / Targets / Command 是三个主Dock；Search在Targets内，Diagnostics/More为次级。Bevy是唯一世界renderer；图稿占位不是JS世界回退。当前主体、观察目标、模型连接、账号、session、admission、control分别表述。

## 5. 关键交互流程

先读对象与可用条件，再编辑草稿/请求预估，只有实际capability允许时才显式提交。pending/unknown保留原操作身份；切换观察对象不改写原回执。关闭弹层返回invoker，焦点/IME不得误触提交。具体feature条款优先于此共同顺序。

## 6. 字段、动作与具体表面

<a id="r-auth-01"></a>
### R-AUTH-01 Passkey 免邮箱注册

- 固定来源：[viewer_passkey_account_module.js:438–446](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.js#L438-L446)。
- 实现/条件：`conditional_current`；registration_enabled 与 passkey.available；创建账号不等于世界准入。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/01-account.svg#entry)；精确区域 `entry`。
  - account-entry：字段 无输入；动作 create passkey account；existing passkey login；状态规范 registration closed；passkey unavailable；signed out。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-02"></a>
### R-AUTH-02 Passkey 登录

- 固定来源：[viewer_passkey_account_module.js:447–454](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.js#L447-L454)。
- 实现/条件：`conditional_current`；用户显式点击启动认证；已登记凭证与当前 session 分开。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/01-account.svg#entry)；精确区域 `entry`。
  - account-entry：字段 无输入；动作 create passkey account；existing passkey login；状态规范 registration closed；passkey unavailable；signed out。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-03"></a>
### R-AUTH-03 世界进入与显式接管

- 固定来源：[viewer_passkey_account_module.js:460–469](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.js#L460-L469)。
- 实现/条件：`conditional_current`；已登录账号；control_elsewhere 时保留原控制或按 expected epoch 明确接管。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/01-account.svg#world)；精确区域 `world`。
  - account-world：字段 expected epoch；动作 enter world；explicit takeover；retain previous control；状态规范 signed in not admitted；control elsewhere；control pending。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-04"></a>
### R-AUTH-04 凭证列表、移除与备用通行密钥

- 固定来源：[viewer_passkey_account_module.js:470–487](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.js#L470-L487)。
- 实现/条件：`conditional_current`；列表失败局部重试；凭证不等于设备会话；安全操作由当前 authority 验证。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/01-account.svg#management)；精确区域 `management`。
  - account-management：字段 credential label；session current/device；动作 add passkey；remove credential；revoke other/current session；logout；retry independent lists；状态规范 signed in；list failure；last credential protected；reauth required。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-05"></a>
### R-AUTH-05 会话列表、单会话撤销和退出

- 固定来源：[viewer_passkey_account_module.js:488–501](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.js#L488-L501)。
- 实现/条件：`conditional_current`；session.current 与其他会话区分；失效时清除私有上下文和导入 signer。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/01-account.svg#management)；精确区域 `management`。
  - account-management：字段 credential label；session current/device；动作 add passkey；remove credential；revoke other/current session；logout；retry independent lists；状态规范 signed in；list failure；last credential protected；reauth required。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-06"></a>
### R-AUTH-06 未知认证原操作只读核对

- 固定来源：[viewer_passkey_account_module.js:405–437](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.js#L405-L437)。
- 实现/条件：`conditional_current`；operation_pending 保留 operation ID；核对原操作而非新建账号/替换候选。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/02-recovery.svg#original)；精确区域 `original`。
  - account-original：字段 operation ID；动作 refresh session；lookup original；状态规范 pending；committed requires fresh authentication；cancelled/expired/exhausted。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-07"></a>
### R-AUTH-07 导入 Ed25519 账号密钥登录、绑定及确认


技术依赖：[Flow-KEYAUTH-008](../../p2p/blockchain/hosted-public-join-key-based-onboarding.prd.md#imported-key-login)（原专业声明 O7-REQ-45c9a6e2-d624-4a57-bced-b96b6aa78f69）；默认次级披露、安全告知、显式gesture、输入清空与内存signer范围仍完整适用。此专业依赖不冒称拥有独立产品声明；产品关系由本节ENTRY-MODE两行承接，无产品N/A。

- 固定来源：[viewer_imported_key.js:49–73](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_imported_key.js#L49-L73)。
- 实现/条件：`conditional_current`；native_key capability；公钥和私钥种子在显式点击后导入，原输入清空，只在页内存签名，不是钱包密钥。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/01-account.svg#import)；精确区域 `import`。
  - account-import：字段 public key 64 hex；private seed password 64 hex；动作 login existing key；bind key when signed in；load key for security confirmation；状态规范 invalid；mismatch；unsupported；conflict。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-08"></a>
### R-AUTH-08 邮箱登录、绑定/替换与首个 Passkey 升级

- 固定来源：[viewer_account_email_recovery.js:1–52](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_account_email_recovery.js#L1-L52)。
- 实现/条件：`conditional_current`；email capability；OTP 按指定用途；账号/邮箱/通行密钥状态分开。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/02-recovery.svg#eligibility)；精确区域 `eligibility`。
  - [具体frame](assets/ui-ux/full-function/account-agent/02-recovery.svg#code)；精确区域 `code`。
  - email-address：字段 email；动作 send operation OTP；状态规范 login_address；binding_address。规范并非全部已画状态。
  - email-code：字段 OTP max6 numeric；动作 confirm operation；状态规范 login_code；binding_code；upgrade_code。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-09"></a>
### R-AUTH-09 固定候选恢复申请、查询、批准/取消及完成

- 固定来源：[viewer_account_email_recovery.js:17–87](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_account_email_recovery.js#L17-L87)。
- 实现/条件：`conditional_current`；email_recovery capability；原绑定邮箱资格、候选 digest、等待/到期时间；完成再验证邮箱与候选。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/11-recovery-lookup.svg#lookup)；精确区域 `lookup`。
  - [具体frame](assets/ui-ux/full-function/account-agent/02-recovery.svg#eligibility)；精确区域 `eligibility`。
  - [具体frame](assets/ui-ux/full-function/account-agent/02-recovery.svg#candidate)；精确区域 `candidate`。
  - [具体frame](assets/ui-ux/full-function/account-agent/02-recovery.svg#status)；精确区域 `status`。
  - [具体frame](assets/ui-ux/full-function/account-agent/02-recovery.svg#complete)；精确区域 `complete`。
  - recovery-lookup：字段 recoveryId input; no client required attribute；动作 view this request using recoveryStatus；close flow；状态规范 signed-in recovery_lookup；then recovery_status。规范并非全部已画状态。
  - recovery-address：字段 bound email；optional recovery ID；动作 send eligibility OTP；状态规范 recovery_address；recovery_code。规范并非全部已画状态。
  - recovery-candidate：字段 无输入；动作 create fixed candidate；状态规范 recovery_candidate。规范并非全部已画状态。
  - recovery-status：字段 request ID；state；not_before；expires_at；candidate digest；动作 refresh；cancel fixed candidate；authenticated approve；signed-out completion OTP；状态规范 waiting；approved；cancelled；completed；unknown label。规范并非全部已画状态。
  - recovery-complete：字段 OTP；candidate passkey ceremony；动作 verify email and candidate；close flow；状态规范 completion_code。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-10"></a>
### R-AUTH-10 能力探测与 legacy 404 回退

- 固定来源：[viewer_passkey_account_surface.jsx:1–56](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_passkey_account_surface.jsx#L1-L56)。
- 实现/条件：`conditional_current`；仅 HTTP 404 回退 legacyForm；其他错误保持能力检查失败。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/01-account.svg#capability)；精确区域 `capability`。
  - account-capability：字段 无输入；动作 404 only: legacy form；状态规范 loading；404 legacy；other failure。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-11"></a>
### R-AUTH-11 Legacy 托管邮箱登录表单

- 固定来源：[viewer_app.jsx:613–705](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L613-L705)。
- 实现/条件：`conditional_current`；仅既有兼容部署；不是普通入口唯一默认。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/07-legacy-session.svg#legacy-address)；精确区域 `legacy-address`。
  - [具体frame](assets/ui-ux/full-function/account-agent/07-legacy-session.svg#legacy-code)；精确区域 `legacy-code`。
  - legacy-address：字段 email autocomplete=email；动作 request login code；状态规范 HTTP404 only；startInFlight；retry_after。规范并非全部已画状态。
  - legacy-code：字段 challenge；masked target；delivery；existing/new；code autocomplete off；动作 sign in and acquire player session；状态规范 completeInFlight；issueInFlight；error。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-12"></a>
### R-AUTH-12 登录强制 gate 与焦点隔离


**源核对修正**：当前HostedLoginGate有Tab循环、focusin guard与cleanup焦点返回；没有login-gate专用inert绑定，AppShell inert只由starterOcGateOpen控制。背景隔离完整行为仍待运行验收。

- 固定来源：[viewer_app.jsx:706–808](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L706-L808)。
- 实现/条件：`conditional_current`；hosted public join 需要登录时；背景不可交互，登录与世界进入分阶段。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/10-entry-gate-focus.svg#focus-boundary)；精确区域 `focus-boundary`。
  - [具体frame](assets/ui-ux/full-function/account-agent/10-entry-gate-focus.svg#gate-world-pending)；精确区域 `gate-world-pending`。
  - account-focus：字段 signed-in / not-signed-in gate copy；动作 Tab focus trap；restore prior focus on cleanup；状态规范 no auth + hosted_public_join；v2 surface ready autofocus。规范并非全部已画状态。
  - gate-stage：字段 signed-in flag gate title/badge/summary；动作 login or create；enter world after account login；状态规范 no auth and hosted_public_join gate；account signed in world pending；no close/escape bypass。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-13"></a>
### R-AUTH-13 进入会话摘要、原资格恢复及释放

- 固定来源：[viewer_app.jsx:2873–2914](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2873-L2914)。
- 实现/条件：`conditional_current`；showPlayerSessionSurface；session registration/control truth 分离。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/07-legacy-session.svg#player-session)；精确区域 `player-session`。
  - player-session：字段 auth；tier；player；boundAgent；rebindNotice；动作 release player session；状态规范 missing；rebinding；recovery；nonlegacy available。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-auth-qa"></a>
### R-AUTH-QA 受限免邮箱 QA 测试登录

- 固定来源：[viewer_hosted_test_login_view.jsx:1–38](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_hosted_test_login_view.jsx#L1-L38)。
- 实现/条件：`qa_only`；显式测试 capability/适用环境；排除普通 Player 功能分母。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`text_spec_only`。
  - test-login：字段 无输入；动作 explicit test login；状态规范 URL opt-in plus supported test deployment；disabled in-flight。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-byok-01"></a>
### R-BYOK-01 准备本机玩家、既有 Agent 与任务

- 固定来源：[browser_byok_surface.jsx:9–157](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/browser_byok_surface.jsx#L9-L157)。
- 实现/条件：`conditional_current`；browser game client 前置；owner mismatch 停止；unsent 保留原件；目标 UTF-8 字节上限 8192。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/04-byok-entry.svg#entry)；精确区域 `entry`。
  - [具体frame](assets/ui-ux/full-function/account-agent/04-byok-entry.svg#original)；精确区域 `original`。
  - byok-entry：字段 goal UTF8 bytes <=8192；entry consent；canonical Agent/Task；动作 prepare missing entry；lookup original；stop preparation；reopen read-only；状态规范 needs_participant；needs_task；needs_goal；ready；pending；unsent；owner_mismatch；failed；stopped。规范并非全部已画状态。
  - byok-unsent：字段 retained original；frozen goal；动作 explicit continue same proven-unsent original；状态规范 durably unsent only；possibly sent lookup only。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-byok-02"></a>
### R-BYOK-02 核对任务与登记浏览器执行身份

- 固定来源：[browser_byok_surface.jsx:158–164](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/browser_byok_surface.jsx#L158-L164)。
- 实现/条件：`conditional_current`；先只读 inspect 再显式 takeover；核对 fence，不因保存 Key 获得游戏控制。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/04-byok-entry.svg#executor)；精确区域 `executor`。
  - byok-executor：字段 world/owner/Agent/Task/goal/executor fence；takeover consent；动作 inspect current task；register browser executor；状态规范 fence absent；preparing；ready；other executor needs explicit switch。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-byok-03"></a>
### R-BYOK-03 浏览器 Key 保存与存储同意

- 固定来源：[browser_model_connection_panel.jsx:5–88](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.jsx#L5-L88)。
- 实现/条件：`conditional_current`；当前 DeepSeek；localStorage 告知、账号隔离与 explicit consent；不自动开始。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/05-byok-model.svg#save)；精确区域 `save`。
  - byok-key：字段 password API key；local save consent；动作 save Key；状态规范 deployment absent no collection；executor not ready；key saved not running。规范并非全部已画状态。
  - byok-storage：字段 account-scoped localStorage record；Web Lock management key；动作 save atomic record；stop retains key；forget key；状态规范 storage corrupt；storage unavailable；lock unavailable。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-byok-04"></a>
### R-BYOK-04 有限付费模型检查

- 固定来源：[browser_model_connection_panel.jsx:89–95](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.jsx#L89-L95)。
- 实现/条件：`conditional_current`；hasKey 与独立 probe consent；有限两请求工具检查，不写世界。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/05-byok-model.svg#probe)；精确区域 `probe`。
  - byok-probe：字段 billable check consent；4 calls/2048 token probe allowance；8 calls/131072 total；动作 test connection；状态规范 no key；checked；auth failure；CORS/network/protocol unknown；budget exhausted。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-byok-05"></a>
### R-BYOK-05 确认运行、停止新决策与原件查询

- 固定来源：[browser_byok_surface.jsx:165–178](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/browser_byok_surface.jsx#L165-L178)。
- 实现/条件：`conditional_current`；run consent + ready + model ready；实际 Agent/任务、预算和 phase 明确；停止不撤销已受理世界动作。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/05-byok-model.svg#run)；精确区域 `run`。
  - byok-start：字段 actual task/goal；run consent；512 output/run call；120sec；动作 confirm start；stop new decisions；lookup originals；forget local Key；状态规范 ready + checked required；running；budget exhausted；authority changed stopped。规范并非全部已画状态。
  - byok-diagnostics：字段 safe category/stage/code/HTTP；original identity；动作 read-only reconcile；状态规范 network；timeout；storage；proof；permission；deployment；history unavailable；pending。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-byok-06"></a>
### R-BYOK-06 断开并忘记本机 Key

- 固定来源：[browser_model_connection_panel.jsx:93–99](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.jsx#L93-L99)。
- 实现/条件：`conditional_current`；删除本站 Key 不撤销供应商 Key、不保证已发请求退款；删除失败不能报成功。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/05-byok-model.svg#stop)；精确区域 `stop`。
  - byok-stop-forget：字段 无输入；动作 stop retains Key；forget site Key；状态规范 not provider revocation；not refund；not world job cancellation。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-byok-07"></a>
### R-BYOK-07 旧托管模型连接管理

- 固定来源：[model_connection_panel.jsx:59–237](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/model_connection_panel.jsx#L59-L237)。
- 实现/条件：`conditional_current`；legacy management/capability 分支；配置 preset、连接状态与执行授权分离，不宣传浏览器之外新托管能力。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/05-byok-model.svg#legacy)；精确区域 `legacy`。
  - byok-legacy：字段 legacy connection selector；动作 select；refresh；stop；forget；状态规范 no create；no probe；no start；no renew。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-01"></a>
### R-CMD-01 当前目标、可控性与连续性摘要

- 固定来源：[viewer_app.jsx:3189–3443](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L3189-L3443)。
- 实现/条件：`current`；选中对象不等于拥有；无 Agent/地点/模块/只读视角分别呈现。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/03-command.svg#chat)；精确区域 `chat`。
  - chat-authority：字段 selected Agent；capability；binding；in-flight；动作 send only when capability and control valid；状态规范 unselected；location selected；binding pending；other Agent；in-flight。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-02"></a>
### R-CMD-02 多行 Agent Chat 与反馈

- 固定来源：[viewer_app.jsx:3444–3497](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L3444-L3497)。
- 实现/条件：`conditional_current`；当前选中可控 Agent + 首聊门禁；draft/send/pending/result 分离。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/03-command.svg#chat)；精确区域 `chat`。
  - [具体frame](assets/ui-ux/full-function/account-agent/03-command.svg#history)；精确区域 `history`。
  - chat-compose：字段 message multiline；动作 explicit Send Chat；状态规范 draft；no feedback；pending；blocked；rejected；accepted/applied as returned。规范并非全部已画状态。
  - chat-history：字段 Agent-filtered history max12；tick；feedback；动作 expand diagnostics；refresh authority and binding；状态规范 empty；history available。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-03"></a>
### R-CMD-03 已认证 canonical goal 回读

- 固定来源：[canonical_agent_goal.jsx:1–13](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/canonical_agent_goal.jsx#L1-L13)。
- 实现/条件：`conditional_current`；只显示 authenticatedCanonicalGoal；pending 提交不写成应用。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/08-command-context.svg#goal)；精确区域 `goal`。
  - canonical-goal：字段 authenticated current goal message；动作 只读；状态规范 available；hidden if unauthenticated；canonical chat pending。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-04"></a>
### R-CMD-04 Agent 当前活动

- 固定来源：[agent_activity_surface.jsx:1–42](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/agent_activity_surface.jsx#L1-L42)。
- 实现/条件：`current`；活动 kind、target、operation、blocked reason 来自显示模型；unknown/idle 不捏造工作。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/08-command-context.svg#activity)；精确区域 `activity`。
  - agent-activity：字段 state；blocked operation；target or unavailable；blocked reason；动作 只读；状态规范 idle；executing；blocked；waiting；unavailable。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-05"></a>
### R-CMD-05 Agent 意图、理由与回执状态

- 固定来源：[agent_intent_surface.jsx:1–35](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/agent_intent_surface.jsx#L1-L35)。
- 实现/条件：`current`；read_only/control_lost/unauthorized/unavailable 隐去私有详情，反馈仍属原主体。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/08-command-context.svg#intent)；精确区域 `intent`。
  - agent-intent：字段 status label；safe message；receipt label；reason summary；lifecycle；next step；动作 只读；状态规范 proposed；submitted；accepted；blocked；completed；rejected；expired；cancelled；superseded；control_lost/read_only/unauthorized/unavailable hide details。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-06"></a>
### R-CMD-06 Agent Context Lite

- 固定来源：[agent_context_lite.jsx:1–155](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/agent_context_lite.jsx#L1-L155)。
- 实现/条件：`current`；只读当前 Agent 上下文，字段缺失保持 unavailable，不推导执行成功。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/08-command-context.svg#context)；精确区域 `context`。
  - agent-context：字段 identity name/id/label；location；state/freshness；objective；nextMove；blocker；playerLeverage；feedback；动作 只读；状态规范 published；unavailable；non-Agent fallback；matched intent；receipt confirmation suppressed。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-07"></a>
### R-CMD-07 安全替代方案比较、确认、执行及恢复

- 固定来源：[safe_alternative_panel.jsx:1–56](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/safe_alternative_panel.jsx#L1-L56)。
- 实现/条件：`conditional_current`；当前 authenticated owner；waypoint/timed request 不支持；preview/confirmation/execution 分离；unknown 只恢复原签名命令。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/09-safe-alternative.svg#compare)；精确区域 `compare`。
  - [具体frame](assets/ui-ux/full-function/account-agent/09-safe-alternative.svg#candidate)；精确区域 `candidate`。
  - [具体frame](assets/ui-ux/full-function/account-agent/09-safe-alternative.svg#execute)；精确区域 `execute`。
  - safe-compare：字段 original goal；waypoint JSON 1-8；optional arrival_tick；动作 compare；recover original；retry exact signed original；状态规范 unknown prevents new compare；JSON invalid error。规范并非全部已画状态。
  - safe-candidate：字段 target；scope；cost upper bound；risk；consequences；retractability；recommendation；required authority；generation/digest/expiry；动作 confirm exact candidate；状态规范 awaiting_confirmation；no_safe_alternative；evaluation_unavailable；unknown disabled。规范并非全部已画状态。
  - safe-execute：字段 authorization ID；actual outcome/status/action/event；动作 execute confirmed；reevaluate；read；decline/cancel；状态规范 authorization available；confirmed；execution_bound；applied；rejected；declined；expired；superseded；unknown disabled。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-08"></a>
### R-CMD-08 强认证批准码

- 固定来源：[viewer_app.jsx:3534–3548](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L3534-L3548)。
- 实现/条件：`conditional_current`；operator/strong auth 条件由后端裁定，不把本地填写视为授权。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/03-command.svg#prompt)；精确区域 `prompt`。
  - prompt-overrides：字段 system override；short term goal；long term goal；approval code when eligible；rollback version；动作 show/hide overrides；preview；apply；rollback；状态规范 hidden default；no authority；preview not apply；accepted not applied；stale/blocked。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-09"></a>
### R-CMD-09 Prompt override 草稿与预览

- 固定来源：[viewer_app.jsx:3498–3600](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L3498-L3600)。
- 实现/条件：`conditional_current`；system/short/long 与聊天草稿分开；preview 不是 applied。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/03-command.svg#prompt)；精确区域 `prompt`。
  - prompt-overrides：字段 system override；short term goal；long term goal；approval code when eligible；rollback version；动作 show/hide overrides；preview；apply；rollback；状态规范 hidden default；no authority；preview not apply；accepted not applied；stale/blocked。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-10"></a>
### R-CMD-10 Prompt apply 与反馈恢复

- 固定来源：[viewer_app.jsx:3588–3651](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L3588-L3651)。
- 实现/条件：`conditional_current`；control lost 安全停止，refresh binding 不自动重发原修改。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/03-command.svg#prompt)；精确区域 `prompt`。
  - prompt-overrides：字段 system override；short term goal；long term goal；approval code when eligible；rollback version；动作 show/hide overrides；preview；apply；rollback；状态规范 hidden default；no authority；preview not apply；accepted not applied；stale/blocked。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-11"></a>
### R-CMD-11 Prompt 版本回滚

- 固定来源：[viewer_app.jsx:3601–3626](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L3601-L3626)。
- 实现/条件：`conditional_current`；只回滚 Prompt 版本，不回滚世界、资产或回执。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/03-command.svg#prompt)；精确区域 `prompt`。
  - prompt-overrides：字段 system override；short term goal；long term goal；approval code when eligible；rollback version；动作 show/hide overrides；preview；apply；rollback；状态规范 hidden default；no authority；preview not apply；accepted not applied；stale/blocked。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-12"></a>
### R-CMD-12 资产 / 治理后置通道占位

- 固定来源：[viewer_app.jsx:3653–3674](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L3653-L3674)。
- 实现/条件：`unimplemented_surface`；现有按钮禁用，仅可读边界；不计为资产转账/治理提交已实现。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 表示：`contract_blocked`（禁用占位）

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/08-command-context.svg#asset)；精确区域 `asset`。
  - asset-lane：字段 main_token_transfer；required_auth；availability；policy reason；动作 disabled transfer placeholder；状态规范 preview_only；blocked；not exposed。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。

<a id="r-cmd-13"></a>
### R-CMD-13 控制丢失反馈恢复按钮

- 固定来源：[viewer_app.jsx:488–528](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L488-L528)。
- 实现/条件：`conditional_current`；仅 feedback.code=control_lost 且提供真实 recoveryAction；恢复不隐式重发。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿的具体关系如下；每个未画状态仍保留text_spec_only，不推定全部组合已覆盖。

#### 具体稿字段与状态映射

- 表示：`normal_frame`。
  - [具体frame](assets/ui-ux/full-function/account-agent/03-command.svg#history)；精确区域 `history`。
  - chat-history：字段 Agent-filtered history max12；tick；feedback；动作 expand diagnostics；refresh authority and binding；状态规范 empty；history available。规范并非全部已画状态。
  - 未证明：Shown form/state examples only; all remaining source combinations and runtime behavior unverified.。


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
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | [R-AUTH-01](viewer-account-agent-uiux.design.md#r-auth-01) | registration_enabled 与 passkey.available；创建账号不等于世界准入 | 对照 [viewer_passkey_account_module.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-AUTH-01](viewer-account-agent-uiux.design.md#r-auth-01) | registration_enabled 与 passkey.available；创建账号不等于世界准入 | 对照 [viewer_passkey_account_module.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | [R-AUTH-02](viewer-account-agent-uiux.design.md#r-auth-02) | 用户显式点击启动认证；已登记凭证与当前 session 分开 | 对照 [viewer_passkey_account_module.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-AUTH-02](viewer-account-agent-uiux.design.md#r-auth-02) | 用户显式点击启动认证；已登记凭证与当前 session 分开 | 对照 [viewer_passkey_account_module.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-AUTH-03](viewer-account-agent-uiux.design.md#r-auth-03) | 已登录账号；control_elsewhere 时保留原控制或按 expected epoch 明确接管 | 对照 [viewer_passkey_account_module.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | [R-AUTH-03](viewer-account-agent-uiux.design.md#r-auth-03) | 已登录账号；control_elsewhere 时保留原控制或按 expected epoch 明确接管 | 对照 [viewer_passkey_account_module.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-AUTH-04](viewer-account-agent-uiux.design.md#r-auth-04) | 列表失败局部重试；凭证不等于设备会话；安全操作由当前 authority 验证 | 对照 [viewer_passkey_account_module.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-AUTH-05](viewer-account-agent-uiux.design.md#r-auth-05) | session.current 与其他会话区分；失效时清除私有上下文和导入 signer | 对照 [viewer_passkey_account_module.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | [R-AUTH-05](viewer-account-agent-uiux.design.md#r-auth-05) | session.current 与其他会话区分；失效时清除私有上下文和导入 signer | 对照 [viewer_passkey_account_module.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-AUTH-06](viewer-account-agent-uiux.design.md#r-auth-06) | operation_pending 保留 operation ID；核对原操作而非新建账号/替换候选 | 对照 [viewer_passkey_account_module.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-AUTH-06](viewer-account-agent-uiux.design.md#r-auth-06) | operation_pending 保留 operation ID；核对原操作而非新建账号/替换候选 | 对照 [viewer_passkey_account_module.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | [R-AUTH-07](viewer-account-agent-uiux.design.md#r-auth-07) | native_key capability；公钥和私钥种子在显式点击后导入，原输入清空，只在页内存签名，不是钱包密钥 | 对照 [viewer_imported_key.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_imported_key.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-AUTH-07](viewer-account-agent-uiux.design.md#r-auth-07) | native_key capability；公钥和私钥种子在显式点击后导入，原输入清空，只在页内存签名，不是钱包密钥 | [网页导入密钥登录](../../testing/manual/hosted-passkey-account.manual.md#网页导入密钥登录)；同候选具native_key能力的获准环境核对错误key/原owner/输入清空/storage边界；静态源关系不代替真实环境验收 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-AUTH-08](viewer-account-agent-uiux.design.md#r-auth-08) | email capability；OTP 按指定用途；账号/邮箱/通行密钥状态分开 | 对照 [viewer_account_email_recovery.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_account_email_recovery.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-08；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-AUTH-09](viewer-account-agent-uiux.design.md#r-auth-09) | email_recovery capability；原绑定邮箱资格、候选 digest、等待/到期时间；完成再验证邮箱与候选 | 对照 [viewer_account_email_recovery.test.js](../../../crates/oasis7_viewer/software_safe_src/viewer_account_email_recovery.test.js) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-09；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | [R-AUTH-10](viewer-account-agent-uiux.design.md#r-auth-10) | 仅 HTTP 404 回退 legacyForm；其他错误保持能力检查失败 | 对照 [viewer_passkey_account_surface.test.jsx](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_surface.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-10；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-AUTH-10](viewer-account-agent-uiux.design.md#r-auth-10) | 仅 HTTP 404 回退 legacyForm；其他错误保持能力检查失败 | 对照 [viewer_passkey_account_surface.test.jsx](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_surface.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-10；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | [R-AUTH-11](viewer-account-agent-uiux.design.md#r-auth-11) | 仅既有兼容部署；不是普通入口唯一默认 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-11；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-AUTH-12](viewer-account-agent-uiux.design.md#r-auth-12) | hosted public join 需要登录时；背景不可交互，登录与世界进入分阶段 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-12；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004)；Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a | [R-AUTH-12](viewer-account-agent-uiux.design.md#r-auth-12) | hosted public join 需要登录时；背景不可交互，登录与世界进入分阶段 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-12；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002)；Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270 | [R-AUTH-13](viewer-account-agent-uiux.design.md#r-auth-13) | showPlayerSessionSurface；session registration/control truth 分离 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-13；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)；Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264 | [R-AUTH-QA](viewer-account-agent-uiux.design.md#r-auth-qa) | 显式测试 capability/适用环境；排除普通 Player 功能分母 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-AUTH-QA；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-001](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001)；Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8 | [R-BYOK-01](viewer-account-agent-uiux.design.md#r-byok-01) | browser game client 前置；owner mismatch 停止；unsent 保留原件；目标 UTF-8 字节上限 8192 | 对照 [browser_byok_surface.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_byok_surface.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-COLLAB-001](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#req-collab-001)；Trace refs: O7-REQ-62007da5-b12a-4b8c-98bb-82ab53a35069 | [R-BYOK-01](viewer-account-agent-uiux.design.md#r-byok-01) | browser game client 前置；owner mismatch 停止；unsent 保留原件；目标 UTF-8 字节上限 8192 | 对照 [browser_byok_surface.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_byok_surface.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003)；Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464 | [R-BYOK-02](viewer-account-agent-uiux.design.md#r-byok-02) | 先只读 inspect 再显式 takeover；核对 fence，不因保存 Key 获得游戏控制 | 对照 [browser_byok_surface.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_byok_surface.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-COLLAB-001](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#req-collab-001)；Trace refs: O7-REQ-62007da5-b12a-4b8c-98bb-82ab53a35069 | [R-BYOK-02](viewer-account-agent-uiux.design.md#r-byok-02) | 先只读 inspect 再显式 takeover；核对 fence，不因保存 Key 获得游戏控制 | 对照 [browser_byok_surface.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_byok_surface.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-001](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001)；Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8 | [R-BYOK-03](viewer-account-agent-uiux.design.md#r-byok-03) | 当前 DeepSeek；localStorage 告知、账号隔离与 explicit consent；不自动开始 | 对照 [browser_model_connection_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002)；Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca | [R-BYOK-03](viewer-account-agent-uiux.design.md#r-byok-03) | 当前 DeepSeek；localStorage 告知、账号隔离与 explicit consent；不自动开始 | 对照 [browser_model_connection_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-005](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-005)；Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d | [R-BYOK-03](viewer-account-agent-uiux.design.md#r-byok-03) | 当前 DeepSeek；localStorage 告知、账号隔离与 explicit consent；不自动开始 | 对照 [browser_model_connection_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002)；Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca | [R-BYOK-04](viewer-account-agent-uiux.design.md#r-byok-04) | hasKey 与独立 probe consent；有限两请求工具检查，不写世界 | 对照 [browser_model_connection_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-005](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-005)；Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d | [R-BYOK-04](viewer-account-agent-uiux.design.md#r-byok-04) | hasKey 与独立 probe consent；有限两请求工具检查，不写世界 | 对照 [browser_model_connection_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003)；Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464 | [R-BYOK-05](viewer-account-agent-uiux.design.md#r-byok-05) | run consent + ready + model ready；实际 Agent/任务、预算和 phase 明确；停止不撤销已受理世界动作 | 对照 [browser_byok_surface.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_byok_surface.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004)；Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34 | [R-BYOK-05](viewer-account-agent-uiux.design.md#r-byok-05) | run consent + ready + model ready；实际 Agent/任务、预算和 phase 明确；停止不撤销已受理世界动作 | 对照 [browser_byok_surface.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_byok_surface.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002)；Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca | [R-BYOK-06](viewer-account-agent-uiux.design.md#r-byok-06) | 删除本站 Key 不撤销供应商 Key、不保证已发请求退款；删除失败不能报成功 | 对照 [browser_model_connection_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004)；Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34 | [R-BYOK-06](viewer-account-agent-uiux.design.md#r-byok-06) | 删除本站 Key 不撤销供应商 Key、不保证已发请求退款；删除失败不能报成功 | 对照 [browser_model_connection_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-001](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001)；Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8 | [R-BYOK-07](viewer-account-agent-uiux.design.md#r-byok-07) | legacy management/capability 分支；配置 preset、连接状态与执行授权分离，不宣传浏览器之外新托管能力 | 对照 [model_connection_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/model_connection_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003)；Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464 | [R-BYOK-07](viewer-account-agent-uiux.design.md#r-byok-07) | legacy management/capability 分支；配置 preset、连接状态与执行授权分离，不宣传浏览器之外新托管能力 | 对照 [model_connection_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/model_connection_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004)；Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34 | [R-BYOK-07](viewer-account-agent-uiux.design.md#r-byok-07) | legacy management/capability 分支；配置 preset、连接状态与执行授权分离，不宣传浏览器之外新托管能力 | 对照 [model_connection_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/model_connection_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-BYOK-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-001](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-001)；Trace refs: O7-REQ-14ddd9ba-2d94-48e8-b2e4-0dbae26ddc36 | [R-CMD-01](viewer-account-agent-uiux.design.md#r-cmd-01) | 选中对象不等于拥有；无 Agent/地点/模块/只读视角分别呈现 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | [R-CMD-01](viewer-account-agent-uiux.design.md#r-cmd-01) | 选中对象不等于拥有；无 Agent/地点/模块/只读视角分别呈现 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-002](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002)；Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293 | [R-CMD-02](viewer-account-agent-uiux.design.md#r-cmd-02) | 当前选中可控 Agent + 首聊门禁；draft/send/pending/result 分离 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004)；Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111 | [R-CMD-02](viewer-account-agent-uiux.design.md#r-cmd-02) | 当前选中可控 Agent + 首聊门禁；draft/send/pending/result 分离 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-COLLAB-002](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#req-collab-002)；Trace refs: O7-REQ-edd52061-73c0-433c-9531-30d86b1db459 | [R-CMD-03](viewer-account-agent-uiux.design.md#r-cmd-03) | 只显示 authenticatedCanonicalGoal；pending 提交不写成应用 | 对照 [canonical_agent_goal.test.jsx](../../../crates/oasis7_viewer/software_safe_src/canonical_agent_goal.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004)；Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111 | [R-CMD-03](viewer-account-agent-uiux.design.md#r-cmd-03) | 只显示 authenticatedCanonicalGoal；pending 提交不写成应用 | 对照 [canonical_agent_goal.test.jsx](../../../crates/oasis7_viewer/software_safe_src/canonical_agent_goal.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [R-CMD-04](viewer-account-agent-uiux.design.md#r-cmd-04) | 活动 kind、target、operation、blocked reason 来自显示模型；unknown/idle 不捏造工作 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004)；Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111 | [R-CMD-05](viewer-account-agent-uiux.design.md#r-cmd-05) | read_only/control_lost/unauthorized/unavailable 隐去私有详情，反馈仍属原主体 | 对照 [agent_intent_surface.test.jsx](../../../crates/oasis7_viewer/software_safe_src/agent_intent_surface.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007)；Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd | [R-CMD-05](viewer-account-agent-uiux.design.md#r-cmd-05) | read_only/control_lost/unauthorized/unavailable 隐去私有详情，反馈仍属原主体 | 对照 [agent_intent_surface.test.jsx](../../../crates/oasis7_viewer/software_safe_src/agent_intent_surface.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | [R-CMD-05](viewer-account-agent-uiux.design.md#r-cmd-05) | read_only/control_lost/unauthorized/unavailable 隐去私有详情，反馈仍属原主体 | 对照 [agent_intent_surface.test.jsx](../../../crates/oasis7_viewer/software_safe_src/agent_intent_surface.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | [R-CMD-06](viewer-account-agent-uiux.design.md#r-cmd-06) | 只读当前 Agent 上下文，字段缺失保持 unavailable，不推导执行成功 | 对照 [agent_context_lite.test.jsx](../../../crates/oasis7_viewer/software_safe_src/agent_context_lite.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [R-CMD-06](viewer-account-agent-uiux.design.md#r-cmd-06) | 只读当前 Agent 上下文，字段缺失保持 unavailable，不推导执行成功 | 对照 [agent_context_lite.test.jsx](../../../crates/oasis7_viewer/software_safe_src/agent_context_lite.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-010](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-010)；Trace refs: O7-REQ-3bbcade1-3f73-4899-b324-f857fd27b245 | [R-CMD-07](viewer-account-agent-uiux.design.md#r-cmd-07) | 当前 authenticated owner；waypoint/timed request 不支持；preview/confirmation/execution 分离；unknown 只恢复原签名命令 | 对照 [safe_alternative_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/safe_alternative_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-011](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-011)；Trace refs: O7-REQ-36cc9e9d-eaa7-436f-b16b-6f6af089b04c | [R-CMD-07](viewer-account-agent-uiux.design.md#r-cmd-07) | 当前 authenticated owner；waypoint/timed request 不支持；preview/confirmation/execution 分离；unknown 只恢复原签名命令 | 对照 [safe_alternative_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/safe_alternative_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-CMD-07](viewer-account-agent-uiux.design.md#r-cmd-07) | 当前 authenticated owner；waypoint/timed request 不支持；preview/confirmation/execution 分离；unknown 只恢复原签名命令 | 对照 [safe_alternative_panel.test.jsx](../../../crates/oasis7_viewer/software_safe_src/safe_alternative_panel.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007)；Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd | [R-CMD-08](viewer-account-agent-uiux.design.md#r-cmd-08) | operator/strong auth 条件由后端裁定，不把本地填写视为授权 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-08；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-002](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002)；Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293 | [R-CMD-09](viewer-account-agent-uiux.design.md#r-cmd-09) | system/short/long 与聊天草稿分开；preview 不是 applied | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-09；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-003](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-003)；Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d | [R-CMD-09](viewer-account-agent-uiux.design.md#r-cmd-09) | system/short/long 与聊天草稿分开；preview 不是 applied | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-09；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-005](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-005)；Trace refs: O7-REQ-b6140b6c-ca2c-4734-84ce-7963f9385fff | [R-CMD-09](viewer-account-agent-uiux.design.md#r-cmd-09) | system/short/long 与聊天草稿分开；preview 不是 applied | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-09；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004)；Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111 | [R-CMD-10](viewer-account-agent-uiux.design.md#r-cmd-10) | control lost 安全停止，refresh binding 不自动重发原修改 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-10；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007)；Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd | [R-CMD-10](viewer-account-agent-uiux.design.md#r-cmd-10) | control lost 安全停止，refresh binding 不自动重发原修改 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-10；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-003](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-003)；Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d | [R-CMD-11](viewer-account-agent-uiux.design.md#r-cmd-11) | 只回滚 Prompt 版本，不回滚世界、资产或回执 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-11；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-008](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-008)；Trace refs: O7-REQ-a9797fdb-39f7-48fb-9202-9700d9d86159 | [R-CMD-11](viewer-account-agent-uiux.design.md#r-cmd-11) | 只回滚 Prompt 版本，不回滚世界、资产或回执 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-11；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-009](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-009)；Trace refs: O7-REQ-92124092-6021-49e4-8def-33038e20b637 | [R-CMD-11](viewer-account-agent-uiux.design.md#r-cmd-11) | 只回滚 Prompt 版本，不回滚世界、资产或回执 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-11；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-CMD-12](viewer-account-agent-uiux.design.md#r-cmd-12) | 现有按钮禁用，仅可读边界；不计为资产转账/治理提交已实现 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-12；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007)；Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd | [R-CMD-13](viewer-account-agent-uiux.design.md#r-cmd-13) | 仅 feedback.code=control_lost 且提供真实 recoveryAction；恢复不隐式重发 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-13；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | [R-CMD-13](viewer-account-agent-uiux.design.md#r-cmd-13) | 仅 feedback.code=control_lost 且提供真实 recoveryAction；恢复不隐式重发 | 按 [Viewer Target QA](viewer-manual.manual.md#target-qa-evidence-checklist) 核对对应交互/状态；源与设计静态审读不代替真实入口、权限与平台的独立验证 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-CMD-13；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |

## 12. 决策、长期风险与未决问题

- 图稿仅代表实际映射区域/状态；没有对应frame的项保持text_spec_only，未公布提交合同保持contract_blocked。图稿完备性不得用页面数判定。
- 本主题的具体frame及字段/动作/状态关系见随稿覆盖目录；未画状态明确保留text_spec_only，不能从概览推导。
- 动态DTO、未知action集合、部署能力、原生桌面及真实世界结果链仍待各专业owner在获准环境验证。
- 需求设计关系只覆盖表中适用范围，不自动将产品状态升为已覆盖；未核实的专业协议或目标CRUD保持明确缺口。

