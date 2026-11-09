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
| [REQ-PROMPT-003](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-003)；Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d | 五源标识、clear/no-op与完整三字段影响的提交前解释 | [五源与恢复](#prompt-source-clear-stale-consumer) | 原授权值投影/patch authority；Viewer；QA | 仅实际支持的 Prompt 表面；不升级旧稿/实现或跨会话保存 |
| [REQ-PROMPT-008](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-008)；Trace refs: O7-REQ-a9797fdb-39f7-48fb-9202-9700d9d86159 | 仍有控制时保草稿、读取/比较最新值及显式重编，失败不自动提交 | [五源与恢复](#prompt-source-clear-stale-consumer) | 原授权值投影/patch authority；Viewer；QA | 仅实际支持的 Prompt 表面；不升级旧稿/实现或跨会话保存 |
| [REQ-PROMPT-006](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-006)；Trace refs: O7-REQ-6f348a2d-cb87-4838-8447-21c8eebf6d07 | Chat/Prompt 的窄屏、低高度、IME/焦点和主动作/结果可达 | [输入与失败接收](#prompt-input-and-failure-consumer) | 原整体布局/输入、请求生命周期和权限；Viewer/QA | 不新增快捷键/模式/写权，不以旧输入测试或安全替代单例代签全部 |
| [REQ-PROMPT-010](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-010)；Trace refs: O7-REQ-3bbcade1-3f73-4899-b324-f857fd27b245 | 非 control_lost 的真实拒绝、有限下一步及恢复再次失败 | [输入与失败接收](#prompt-input-and-failure-consumer) | 原整体布局/输入、请求生命周期和权限；Viewer/QA | 不新增快捷键/模式/写权，不以旧输入测试或安全替代单例代签全部 |

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
- 新增设计目标：[五源、清除与完整影响状态稿](#prompt-state-sheets)；该交付不改变本条固定源码的实现判断。
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
- 新增设计目标：[执行器待读回、stale、读取再失败、unknown 与失权状态稿](#prompt-state-sheets)；仅表列场景已有具体画面。
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

<a id="prompt-source-clear-stale-consumer"></a>
### Prompt 五种来源、清除与过期草稿的具体接收

Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d；O7-REQ-a9797fdb-39f7-48fb-9202-9700d9d86159。本节是 R-CMD-09/11 对 REQ-PROMPT-003/008 的目标补充，消费[授权值投影](viewer-control-plane-split-live-playback.design.md#prompt-authorized-value-projection)。现有 `03-command.svg#prompt` 只画出了三项 override 与动作框架；新增五源、clear/no-change、完整影响及 stale/失败恢复的具体图稿见[Prompt 状态稿](#prompt-state-sheets)，由[交互原型的 Prompt 入口](assets/ui-ux/interactive/index.html#prompt)承接完整布局与操作路径。图稿和原型均为 `target_only` 设计交付，不将旧 SVG、固定源码或生产页面升级为已实现。不增加新的主导航、持续存储承诺或所有入口的 Prompt 修改能力。

**展开后的结构。** 沿 R-CMD-01 保持当前 Agent/控制资格在前，接当前来源与三字段编辑，之后才是完整变化、Preview/Apply/Rollback、结果和恢复。每项字段的来源与值相邻；默认/当前值在输入框外只读显示，不能将 placeholder 当当前配置。宽屏可并列来源与草稿，窄屏按同一顺序堆叠；长文本折叠/内部滚动仍须提供可达的全文和复制，按钮、原因及恢复路径保持可见。键盘、IME 与多行输入继续沿原整体设计，不新增 Enter 提交。

| 来源 | 必须展示的内容与目标文案 | 允许交互及不能混同的含义 |
| --- | --- | --- |
| 配置默认 | “已加载配置默认 / Loaded configuration default”、安全来源标签与 revision；值由投影 producer 提供 | 只读参考；不从本机当前文件/常量猜测，不把初始化覆盖称为 config 默认，不自动填入 `set` |
| 当前 effective | “Runner 当前已确认值 / Confirmed runner value”、实际来源与读取时点；更新未消费时另标“等待 runner 更新 / Awaiting runner update” | 仅展开该层获权值；与 authority profile 已应用分开，不能说正在执行的旧模型调用已切换 |
| profile override | “当前覆盖 / Current override”、profile 版本及三字段值；合法 None 显示“未设置 / Not set” | 编辑基准来自当前 profile。hidden/unavailable 显示其状态，不用空输入冒充 None；rollback 仍只指原 Prompt 历史版本 |
| 本地 draft | “草稿，尚未提交 / Draft; not submitted”、原 Agent 与加载版本、dirty 状态 | 编辑只改本地 draft；刷新不覆盖 dirty 内容。切换观察对象不重定向草稿，显式换收件人仍遵循原权限规则 |
| preset | 在实际支持该功能的表面显示“预设填充的草稿 / Draft from preset”及当前模板来源 | 填充不是已发送或配置已应用，随后编辑仍是 draft；沿原存储范围，不给缺此能力的入口补造 preset 或跨会话保存保证 |

**清除的明确表达。** “清除覆盖 / Clear override”只创建原 clear patch；“恢复默认”只有在本次真实候选确会回到已显示 config 值时才能用作解释。输入 trim 后为空时，就地显示“此操作将清除覆盖，不保存空字符串 / This clears the override; it does not store an empty string”，等待显式 Preview/Apply；未编辑字段保持 unchanged。

- profile 已有 C、Builtin config 为 A：清除候选显示“C → 未设置；成功下发后使用 A”，A 有来源与版本；不能显示回到初始化 B。
- profile 已是 None、当前 effective 是初始化 B，且其他字段不变：显示“没有 profile 变更；runner 仍使用初始化值 B”，不把该操作标为默认恢复。UI 可将单独清除显示为无须操作；协议收到该 exact patch 时仍按原 `accepted/no_change` 解释。
- 任何另一字段改变会触发原完整 profile 下发。完整变化区必须同时列出例如“system：当前值 → 新覆盖；short-term：profile 未设置保持不变，runner B → config A”，并区分 profile changed fields 和预计 effective 变化。不能只列用户点击字段、只看 `applied_fields` 或用笼统“应用成功”隐藏附带影响。
- 本地预告标记“预计，尚未应用”；显式 Preview 的原结果也不是实际应用。Apply 后按原 scope 呈现 authority 结果，runner 未读回时保留等待，不提前把预告涂成当前生效值。unsupported、尚未初始化或读取失败提供原配置/检查/重读入口，不能自动启动模型来补齐默认值。

**stale 与恢复。** 若当前仍有 control 且最新 V+1 取代基准 V，保留 D，错误区显示“当前配置已变化，草稿未应用 / Configuration changed; draft not applied”。“查看变化 / Compare”只比较当前有权显示的基准/最新 profile、effective 与 D；“重新编辑 / Re-edit”显式采用新基准后仍保持草稿；“重载 / Reload”须明确将放弃哪些未提交编辑；“放弃 / Discard”只丢草稿。任何选择都不自动 Apply，不将新版本 patch 当旧请求 retry。

读取失败时显示具体可披露原因及“重新读取 / Read again”或安全退出；保持本地 D、将旧值标为先前记录，不虚构最新值或差异。恢复动作本身失败时继续明确保留该失败与下一步，不回到无限 pending。control_lost/session 失效优先：隐藏无权再见的 default/profile/effective 与差异，只保留原规则允许的未提交安全草稿和重新认证/重新选择或放弃；不提供重放/转移。账号切换清除旧私有读回，迟到旧响应不更新新表面。ProviderBacked production 不支持、canonical 私有目标不可由 snapshot 取得等情况，均使用原明确边界，不借新来源区绕过它们。

**方法与交付接点。** 当前读 producer、runtime/runner 关联和原写规则由上游投影设计拥有；本表提供具体位置、字段、文案、动作和状态，不再定义第二份计算规则。按[来源/清除/恢复验收](viewer-manual.manual.md#prompt-source-clear-stale-validation)在同一候选的真实表面核对 PV01–PV10，保留桌面与窄屏的前态、完整预告、结果和失败恢复。以下 SVG 证明具体状态已落稿；交互原型只证明其本地示例交互，生产实现、真实授权读回与各平台输入仍须独立留证。

<a id="prompt-state-sheets"></a>
#### Prompt 状态稿与原型接点

两份可编辑 SVG 沿用 [03-command.svg](assets/ui-ux/full-function/account-agent/03-command.svg) 的暗色面板、金色主动作及错误语义。每帧均为独立的 1440×1024「高级 Prompt 已展开」状态；12 号稿高 3072，13 号稿高 4096，多帧纵排只用于设计文件浏览，不是实际界面的同时展开布局，也不代替默认 Player 的世界舞台。可操作组合入口为 [index.html#prompt](assets/ui-ux/interactive/index.html#prompt)，窄屏、低高度、软键盘与 200% 文本状态继续按 PV10/PV11 核对。

| 实际 SVG / frame | 已画字段、状态与显式动作 | 对应既有方法与边界 |
| --- | --- | --- |
| [12 / sources-clear](assets/ui-ux/full-function/account-agent/12-prompt-sources-clear.svg#sources-clear) | short 字段并列标识已加载默认 A、执行器已确认 C、profile C、基于 v7 的 clear 草稿与 preset-P 来源；system/long 保持项可展开。完整三字段预告、应用清除、预览、放弃分别可辨 | PV01/PV03/PV14 的特定前态。preset 填入后仍是本地编辑；C→未设置，预计消费后用 A。未画最终 runner 成功帧，不以预告代签 |
| [12 / clear-no-change](assets/ui-ux/full-function/account-agent/12-prompt-sources-clear.svg#clear-no-change) | 初始 profile 全部未设置，effective short 为初始化 B；原 clear 结果“无更改”，v0 不变且没有向执行器下发；返回、重新读取可见 | PV04。该完整候选无变化；不显示“已恢复 A”。trim 后空 set 同属该结果，输入规范化本身仍依原协议 |
| [12 / full-profile-awaiting](assets/ui-ux/full-function/account-agent/12-prompt-sources-clear.svg#full-profile-awaiting) | 仅 system 覆盖改为 D，已应用 profile v1；表格逐项分开新 profile、执行器已确认旧值与匹配读回后预计值。short 的 B→A 单独高亮；重新读取、返回可见 | PV05/PV08 的已提交、尚未确认消费状态。当前读回仍关联 v0；“配置已应用”不充当执行器 ACK，不改写在途模型调用 |
| [13 / stale-compare](assets/ui-ux/full-function/account-agent/13-prompt-stale-recovery.svg#stale-compare) | 原 v7/C、最新获权 v8/C2、原 Agent 的本地 D 三栏比较；当前 effective C2 的关联/时点。重新编辑采用 v8、重载、放弃及其后果相邻显示 | PV06。比较只读；重新编辑保留 D 并建立新基准，仍待显式提交。重载明确丢弃 D 并需确认，不能把 stale 当自动 retry |
| [13 / read-failed-again](assets/ui-ux/full-function/account-agent/13-prompt-stale-recovery.svg#read-failed-again) | 原 stale 保留；重读及再次重读失败已结束。D/原 Agent/v7 保留；C 标“先前记录”，最新比较不可用；再次读取、保留草稿返回、放弃可见 | PV06/PV12 的连接失败示例。原操作与恢复错误分开，不无限 pending、不清空草稿、不伪造最新值 |
| [13 / original-result-unknown](assets/ui-ux/full-function/account-agent/13-prompt-stale-recovery.svg#original-result-unknown) | E1 原 Apply 与 unknown 保留，E2 的当前配置及初始化 B 独立读取；旧操作不可在此重试，当前重读和退出仍可用 | PV09/PV12 的已换实例/epoch 示例。本例无安全原结果读取，不推定所有路由均无该能力；读当前值不裁决旧请求，不能换请求身份重放 |
| [13 / control-lost-hidden](assets/ui-ux/full-function/account-agent/13-prompt-stale-recovery.svg#control-lost-hidden) | default/profile/effective/版本差异全部隐藏；仅原规则允许的安全未提交 D 保留。检查当前绑定、放弃和返回可辨，旧私有正文、版本与 digest 不再展示 | PV07 及 R-CMD-13。只有原入口提供 recoveryAction 时才显示该检查动作；恢复不重发/转移草稿。账号切换清旧私有上下文，旧响应不能写入新主体 |

以上夹具沿 PV01–PV09/PV12 的 A/B/C/C2/D/P 关系；`cfg-1`、v0/v7/v8、E1/E2 与 t100/t120/t122 是安全示例标签，不新增后端字段或协议。五种来源在适用表面显示，缺少 Prompt 投影、preset 或原结果查询能力的入口仍使用原明确边界。主按钮可用性来自当前权限与请求生命周期；Preview 仅检查候选，不新增必须取得预览 token 的写入条件。Chat 仍按 R-CMD-02/03 独立发送和确认 canonical goal，观察对象变化不改草稿或原请求的目标。

**编辑与核查。** SVG 文本保持可编辑，字体为 `Noto Sans CJK SC`，未打包字体或外部图像；可在 Inkscape 中按 frame id 单独导出。以提交中的源文件和片段 ID 固定设计版本。已用 Inkscape 对七帧实际渲染，检查文本列宽、按钮标签及面板边界；这不替代交互原型浏览器核查，也不声称 PV01–PV12 的真实协议、模型、世界或平台输入已运行。新增稿只覆盖表列状态，未画的回滚、其他拒绝及入口组合仍按原条款保留缺口。

<a id="prompt-input-and-failure-consumer"></a>
### 对话/Prompt 输入与真实失败下一步的组合接收

Trace refs: O7-REQ-6f348a2d-cb87-4838-8447-21c8eebf6d07；O7-REQ-3bbcade1-3f73-4899-b324-f857fd27b245。本节接收 REQ-PROMPT-006/010；R-CMD-02 的多行 Chat、R-CMD-09/11 的 Prompt 编辑和本节来源区共同沿[原布局样本](viewer-page-module-design-2026-06-18.design.md#104-响应式和可用空间)与[输入/焦点规范](viewer-page-module-design-2026-06-18.design.md#uiux-input-access)工作，不新增快捷键、CSS 断点或模态。

在声明适用的桌面、窄屏、低高度与软键盘状态，保持“Agent/控制资格→输入/来源→显式主动作→结果/恢复”的可达顺序；长 CJK 与文本放大允许换行/纵向滚动，不把结果推到不可达区。Command 非模态时不加焦点锁；真正模态按原规范隔离背景，关闭返回原 invoker 或仍存在的导航。IME 组合态及 keyCode 229 不发送/关闭，Enter/Shift+Enter 继续编辑；只由已有显式 Send Chat/Preview/Apply 等对应动作提交。点击、选择复制、滚轮和拖拽编辑区不触发世界相机/拾取，切换尺寸和观察对象不暗中改草稿收件人。

所有失败先按[原请求生命周期](viewer-control-plane-split-live-playback.design.md#viewer-client-request-lifecycle-design)关联 operation/目标/玩家/连接与等待实例，之后才改变该请求的反馈。无归属错误只作受限协议诊断；旧错误不能结束新请求。非 control_lost 失败消费原 reason/code 与实际支持的 next_step，给出以下有限映射，不从文案推断权限：

| 原失败事实 | 当前表面给出的下一步 | 恢复再次失败时 |
| --- | --- | --- |
| 字段、未知 operation 或原 rollback target 被拒绝 | 就地说明错误；允许仍获权的重新编辑/选择有效原版本，或放弃；新编辑是新请求 | 保留原拒绝和新恢复错误，不覆盖已确认结果、不自动换目标/版本 |
| 认证/批准、session 或 binding 不满足 | 只提供该入口真实已有的重新认证/绑定检查；control_lost 仍按前节 hidden 优先 | 不借 UI 恢复授予新权限。原允许的 `session_not_found` 自动恢复预算至多一次，第二次失败停止自动循环；手动后续也核原结果/当前资格 |
| runner/模式不支持、尚未初始化或真实依赖失败 | 显示具体能力边界；已有配置/检查/授权读取可用时提供，或安全退出 | 不自动启动模型、切 Provider、转旧通道或合造成功；原独立可读能力保留 |
| 原结果未知、读取失败或 result ledger/容量受限 | 当前仍有读权时核对原身份；没有该协议的安全原结果读取就明确保持未知并停止本件新提交，可查看当前合法状态 | 读当前 profile 成功不证明旧请求结果；重连/重载不清原 nonce/ledger，不用新键绕过缓存满或 unknown。错误与安全停止持续可见，不能永久转圈 |

结果区始终区分原请求失败和本次恢复失败，并保留原对象、可披露原因与下一步；accepted/applied 及 scope 仍由原 authority 结果决定。具体执行沿[来源与输入/失败方法](viewer-manual.manual.md#prompt-source-clear-stale-validation)的 PV06–PV12；原 S6/Chat IME 方法继续适用，真实输入、低高度和失败恢复没有运行时保持未验证。

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
| [REQ-PROMPT-003](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-003)；Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d | [五源与恢复](#prompt-source-clear-stale-consumer) | 五源标识、clear/no-op与完整三字段影响的提交前解释 | [来源/清除/恢复方法](viewer-manual.manual.md#prompt-source-clear-stale-validation)：PV01–PV10，同候选真实桌面/窄屏及原协议接点 | 五源、完整预告、真实结果、stale/失权/刷新失败与恢复可达 | [表列状态已落稿](#prompt-state-sheets)；SVG/本地原型不代签生产 UI、真实模型或另一表面 |
| [REQ-PROMPT-008](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-008)；Trace refs: O7-REQ-a9797fdb-39f7-48fb-9202-9700d9d86159 | [五源与恢复](#prompt-source-clear-stale-consumer) | 仍有控制时保草稿、读取/比较最新值及显式重编，失败不自动提交 | [来源/清除/恢复方法](viewer-manual.manual.md#prompt-source-clear-stale-validation)：PV01–PV10，同候选真实桌面/窄屏及原协议接点 | 五源、完整预告、真实结果、stale/失权/刷新失败与恢复可达 | [表列状态已落稿](#prompt-state-sheets)；SVG/本地原型不代签生产 UI、真实模型或另一表面 |
| [REQ-PROMPT-006](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-006)；Trace refs: O7-REQ-6f348a2d-cb87-4838-8447-21c8eebf6d07 | [输入与失败接收](#prompt-input-and-failure-consumer) | Chat/Prompt 的窄屏、低高度、IME/焦点和主动作/结果可达 | [输入/失败方法](viewer-manual.manual.md#prompt-source-clear-stale-validation)：PV10–PV11，真实声明表面与受控异步故障分别核对 | 输入事件/请求数、焦点/可达性、原原因与恢复再失败、零越权重发 | 方法未执行；真实桌面/窄屏/低高度与当前权限分别留证 |
| [REQ-PROMPT-010](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-010)；Trace refs: O7-REQ-3bbcade1-3f73-4899-b324-f857fd27b245 | [输入与失败接收](#prompt-input-and-failure-consumer) | 非 control_lost 的真实拒绝、有限下一步及恢复再次失败 | [输入/失败方法](viewer-manual.manual.md#prompt-source-clear-stale-validation)：PV06–PV12，真实声明表面与受控异步故障分别核对 | 输入事件/请求数、焦点/可达性、原原因与恢复再失败、零越权重发 | 方法未执行；真实桌面/窄屏/低高度与当前权限分别留证 |

## 12. 决策、长期风险与未决问题

- 图稿仅代表实际映射区域/状态；没有对应frame的项保持text_spec_only，未公布提交合同保持contract_blocked。图稿完备性不得用页面数判定。
- 本主题的具体frame及字段/动作/状态关系见随稿覆盖目录及[新增 Prompt 状态稿](#prompt-state-sheets)；未画状态明确保留text_spec_only，不能从概览推导。
- 动态DTO、未知action集合、部署能力、原生桌面及真实世界结果链仍待各专业owner在获准环境验证。
- 需求设计关系只覆盖表中适用范围，不自动将产品状态升为已覆盖；未核实的专业协议或目标CRUD保持明确缺口。
